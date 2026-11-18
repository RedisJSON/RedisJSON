"""RDB encoding v5 with atomic slot migration (MOD-20882).

A node that exports slots writes no cross-key stream, since the destination loads the slot
snapshot one key at a time, without a load session. The slot-export logic is unit-tested only
against a fake module API. These tests start their own 2-shard cluster and check, on a real
server, that a BGSAVE during an export
is self-contained (`reason=asm`), that the importing node keeps streaming, that every document
arrives intact, and that both nodes stream again once the export ends, fails or is cancelled.
"""

import json
import os
import random
import re
import string
import time

from redis.exceptions import ResponseError
from RLTest import Env

from common import TimeLimit


LOG_MARK = 'JSON RDB dictionary'
# Enough keys that the migrated range holds dozens of them.
KEYS = 300
# The source sends each snapshot key this slowly (microseconds), so the importing node has time
# for a save and the writes below land after the snapshot fork.
SNAPSHOT_KEY_DELAY = 50000
# The destination waits this long before each block of the writes it buffered during the snapshot
# (about 10 blocks here), which keeps the source in `send-stream` for seconds: still exporting,
# with no fork child, so a BGSAVE can start.
BUFFER_APPLY_DELAY = 300000
# Migrated keys written during the snapshot; each write adds a key too, so the buffer holds twice
# as many commands.
WRITES_DURING_SNAPSHOT = 30


def _cluster_env():
    env = Env(shardsCount=2, decodeResponses=True)
    if env.env != 'oss-cluster':
        env.skip()
    return env


def _shared_text(seed, size):
    rng = random.Random(seed)
    return ''.join(rng.choices(string.ascii_letters + string.digits, k=size))


FIRST, SECOND = _shared_text(20882, 2048), _shared_text(3, 512)


def _document(i, first=FIRST, second=SECOND):
    return {'shared_checkpoint_text': first, 'second_shared_field': second, 'n': i}


def _load_documents(env):
    documents = {f'checkpoint:{i}': _document(i) for i in range(KEYS)}
    cluster = env.getClusterConnectionIfNeeded()
    for key, document in documents.items():
        env.assertEqual(cluster.execute_command('JSON.SET', key, '$', json.dumps(document)), 'OK')
    return documents


def _shard_keys(connection):
    return set(connection.scan_iter(count=1000))


def _assert_documents(env, connection, documents, keys):
    """The server holds exactly `keys`, each as written."""
    env.assertEqual(_shard_keys(connection), keys)
    for key in keys:
        env.assertEqual(json.loads(connection.execute_command('JSON.GET', key)), documents[key])


class _ServerLog:
    """The module's RDB log lines that one server wrote since the watch started."""

    def __init__(self, env, shard):
        self.path = os.path.join(env.logDir, shard._getFileName('master', '.log'))
        try:
            self.start = len(self._lines())
        except OSError:
            # Without the log nothing shows which form a save wrote.
            env.skip()

    def _lines(self):
        with open(self.path, errors='replace') as log:
            return [line.strip() for line in log if LOG_MARK in line]

    def lines(self):
        return self._lines()[self.start:]


def _field(line, name):
    return int(re.search(rf'\b{name}=(\d+)', line).group(1))


def _log_pid(line):
    match = re.match(r'(\d+):', line)
    return match and match.group(1)


def _last_save_start(env, log):
    started = [line for line in log.lines() if 'phase=persistence_start' in line]
    env.assertTrue(started, message=f'no save started: {log.lines()}')
    return started[-1] if started else ''


def _expect_self_contained_save(env, log):
    """The last save since the watch started had no stream, because the node exported slots."""
    env.assertContains('mode=local reason=asm', _last_save_start(env, log))


def _expect_stream_save(env, log, min_refs):
    """The last save since the watch started wrote a stream that defined and referenced strings."""
    started = _last_save_start(env, log)
    env.assertContains('mode=stream', started)
    pid = _log_pid(started)
    ended = [line for line in log.lines() if 'phase=persistence_ended' in line and _log_pid(line) == pid]
    env.assertTrue(ended, message=f'no stream summary: {log.lines()}')
    if ended:
        env.assertGreaterEqual(_field(ended[-1], 'defs'), 1)
        env.assertGreaterEqual(_field(ended[-1], 'refs'), min_refs)
        env.assertContains('poisoned=false', ended[-1])


def _expect_stream_load(env, log, min_refs):
    lines = log.lines()
    loaded = [line for line in lines if 'phase=loading_ended' in line]
    env.assertTrue(loaded, message=f'no stream load: {lines}')
    if loaded:
        env.assertGreaterEqual(_field(loaded[-1], 'refs'), min_refs)
    _expect_no_refusal(env, log)


def _expect_no_refusal(env, log):
    env.assertFalse(any('status=refused' in line for line in log.lines()), message=str(log.lines()))


def _bgsave(connection):
    """Runs a BGSAVE to the end. The slot snapshot child can still be exiting when it starts."""
    with TimeLimit(60):
        while True:
            try:
                connection.bgsave()
                break
            except ResponseError as error:
                if 'already in progress' not in str(error):
                    raise
                time.sleep(0.01)
        while connection.info('persistence')['rdb_bgsave_in_progress']:
            time.sleep(0.01)
    assert connection.info('persistence')['rdb_last_bgsave_status'] == 'ok'


def _node_slots(connection):
    """The slot ranges that the node behind `connection` owns, as (start, end) pairs."""
    for line in connection.execute_command('CLUSTER', 'NODES').splitlines():
        node_id, _addr, flags, *rest = line.split()
        if 'myself' in flags.split(','):
            return {tuple(map(int, slots.split('-'))) for slots in rest[5:] if '-' in slots}


def _migrated_range(source):
    """The middle third of the source's slots."""
    (start, end), = _node_slots(source)
    third = (end - start) // 3
    return start + third, end - third


def _task_status(connection, task_id):
    """The task as one node sees it; the source knows it only once the destination connects."""
    for status in connection.execute_command('CLUSTER', 'MIGRATION', 'STATUS', 'ID', task_id):
        return dict(zip(status[0::2], status[1::2]))
    return {}


def _task_state(connection, task_id):
    return _task_status(connection, task_id).get('state')


def _wait_for_state(connection, task_id, states, timeout=60):
    with TimeLimit(timeout):
        while _task_state(connection, task_id) not in states:
            time.sleep(0.01)


def _keys_in_range(connection, keys, first, last):
    return {key for key in keys if first <= connection.execute_command('CLUSTER', 'KEYSLOT', key) <= last}


def _start_held_export(env, source, destination, documents):
    """Starts an import of the middle third of the source's slots and holds the export.

    While the snapshot runs slowly, it writes to the migrated keys. The destination buffers those
    writes and applies them slowly once the snapshot ends, so the source stays in `send-stream`.
    Returns the task and the migrated keys, with `documents` updated to what they hold now."""
    first, last = _migrated_range(source)
    migrated = _keys_in_range(source, _shard_keys(source), first, last)
    env.assertGreater(len(migrated), WRITES_DURING_SNAPSHOT)
    source.config_set('rdb-key-save-delay', str(SNAPSHOT_KEY_DELAY))
    destination.config_set('key-load-delay', str(BUFFER_APPLY_DELAY))
    task_id = destination.execute_command('CLUSTER', 'MIGRATION', 'IMPORT', first, last)
    # The source has forked its snapshot, so later writes reach the destination as a stream.
    _wait_for_state(source, task_id, {'send-bulk-and-stream'})
    cluster = env.getClusterConnectionIfNeeded()
    # Swap the two shared strings of some migrated keys, and add keys that share them, so the
    # destination receives these as writes, not in the snapshot.
    for key in sorted(migrated)[:WRITES_DURING_SNAPSHOT]:
        document = _document(documents[key]['n'], first=SECOND, second=FIRST)
        cluster.execute_command('JSON.SET', key, '$', json.dumps(document))
        documents[key] = document
        extra = f'{{{key}}}:written-during-export'
        cluster.execute_command('JSON.SET', extra, '$', json.dumps(document))
        documents[extra] = document
        migrated.add(extra)
    return task_id, migrated


def _wait_for_send_stream(source, task_id):
    """The snapshot child has exited and the source still exports. Its own saves must not be slow."""
    _wait_for_state(source, task_id, {'send-stream'})
    source.config_set('rdb-key-save-delay', '0')


def _release_export(source, destination):
    source.config_set('rdb-key-save-delay', '0')
    destination.config_set('key-load-delay', '0')


def _wait_for_keys(connection, count):
    """The node trims the keys of the slots it no longer owns in the background."""
    with TimeLimit(60):
        while connection.dbsize() != count:
            time.sleep(0.05)


def test_a_save_during_a_slot_export_is_self_contained():
    env = _cluster_env()
    documents = _load_documents(env)
    destination_shard, source_shard = env.envRunner.shards
    destination, source = destination_shard.getConnection(), source_shard.getConnection()
    source_keys, destination_keys = _shard_keys(source), _shard_keys(destination)
    import_log = _ServerLog(env, destination_shard)
    try:
        task_id, migrated = _start_held_export(env, source, destination, documents)

        # The importing node owns none of the migrated slots yet, and keeps streaming.
        log = _ServerLog(env, destination_shard)
        _bgsave(destination)
        _expect_stream_save(env, log, min_refs=len(destination_keys) - 1)

        # The source no longer has a fork child but still exports, so its save shares nothing.
        _wait_for_send_stream(source, task_id)
        log = _ServerLog(env, source_shard)
        _bgsave(source)
        _expect_self_contained_save(env, log)
        env.assertNotEqual(_task_state(source, task_id), 'completed', message='the export ended too soon')
    finally:
        _release_export(source, destination)
    _wait_for_state(destination, task_id, {'completed'})
    _wait_for_state(source, task_id, {'completed'})

    # The destination restored the snapshot and the writes key by key, with nothing refused, and
    # the source dropped the keys it gave away.
    source_keys -= migrated
    destination_keys |= migrated
    _wait_for_keys(source, len(source_keys))
    _assert_documents(env, destination, documents, destination_keys)
    _assert_documents(env, source, documents, source_keys)
    _expect_no_refusal(env, import_log)

    # Once the export ends, both nodes stream again, and each loads its own stream.
    for shard, keys in ((source_shard, source_keys), (destination_shard, destination_keys)):
        connection = shard.getConnection()
        log = _ServerLog(env, shard)
        _bgsave(connection)
        _expect_stream_save(env, log, min_refs=len(keys) - 1)
        log = _ServerLog(env, shard)
        connection.execute_command('DEBUG', 'RELOAD')
        _assert_documents(env, connection, documents, keys)
        _expect_stream_load(env, log, min_refs=len(keys) - 1)


def test_the_source_streams_again_after_a_cancelled_or_failed_export():
    env = _cluster_env()
    documents = _load_documents(env)
    destination_shard, source_shard = env.envRunner.shards
    destination, source = destination_shard.getConnection(), source_shard.getConnection()
    source_keys = _shard_keys(source)

    # Cancelled: the export was running, as the save during it shows, until both nodes cancel it.
    try:
        task_id, migrated = _start_held_export(env, source, destination, documents)
        _wait_for_send_stream(source, task_id)
        log = _ServerLog(env, source_shard)
        _bgsave(source)
        _expect_self_contained_save(env, log)
        # The destination is busy applying its buffer, so the source cancels first.
        source.execute_command('CLUSTER', 'MIGRATION', 'CANCEL', 'ID', task_id)
        _wait_for_state(source, task_id, {'canceled'})
        destination.execute_command('CLUSTER', 'MIGRATION', 'CANCEL', 'ID', task_id)
    finally:
        _release_export(source, destination)
    # The source keeps every key, including those written during the export.
    source_keys |= migrated
    log = _ServerLog(env, source_shard)
    _bgsave(source)
    _expect_stream_save(env, log, min_refs=len(source_keys) - 1)
    _assert_documents(env, source, documents, source_keys)

    # Failed: a fail point stops the export at its handoff, after the snapshot. A failed task
    # retries, so its error shows the failure more reliably than its state, and it is cancelled.
    first, last = _migrated_range(source)
    env.assertOk(source.execute_command('DEBUG', 'ASM-FAILPOINT', 'migrate-main-channel', 'handoff'))
    try:
        task_id = destination.execute_command('CLUSTER', 'MIGRATION', 'IMPORT', first, last)
        with TimeLimit(60):
            while not any('handoff' in (_task_status(node, task_id).get('last_error') or '')
                          for node in (source, destination)):
                time.sleep(0.01)
        source.execute_command('CLUSTER', 'MIGRATION', 'CANCEL', 'ID', task_id)
        destination.execute_command('CLUSTER', 'MIGRATION', 'CANCEL', 'ID', task_id)
        _wait_for_state(source, task_id, {'canceled'})
    finally:
        source.execute_command('DEBUG', 'ASM-FAILPOINT', '', '')
    log = _ServerLog(env, source_shard)
    _bgsave(source)
    _expect_stream_save(env, log, min_refs=len(source_keys) - 1)
    log = _ServerLog(env, source_shard)
    source.execute_command('DEBUG', 'RELOAD')
    _assert_documents(env, source, documents, source_keys)
    _expect_stream_load(env, log, min_refs=len(source_keys) - 1)
