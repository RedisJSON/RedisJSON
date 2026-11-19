"""Resharding with MIGRATE and RDB encoding v5 (MOD-20882).

`redis-cli --cluster reshard` moves a slot with MIGRATE: every key travels as a DUMP payload that
the target RESTOREs without a load session, so the payload must be self-contained. These tests
start their own cluster of 2 shards, move a slot of documents that share long strings with the
target's own documents, and check that the documents arrive intact, that the target refuses no
record, and that both shards still save and load a stream afterwards.
"""

import json
import os
import random
import re
import string
import time
from contextlib import contextmanager

from RLTest import Env

from common import TimeLimit


SHARDS = 2
LOG_MARK = 'JSON RDB dictionary'
# The hash tag puts every moving key in one slot.
MOVING = '{moving}:'
MOVING_KEYS = 100
OTHER_KEYS = 200


def _cluster_env():
    env = Env(shardsCount=SHARDS, decodeResponses=True)
    if env.env != 'oss-cluster':
        env.skip()
    return env


def _shared_text(seed, size):
    rng = random.Random(seed)
    return ''.join(rng.choices(string.ascii_letters + string.digits, k=size))


def _documents():
    """Documents in the moving slot, and documents spread over both shards, that all repeat the
    same two long strings under the same object keys."""
    first, second = _shared_text(20882, 2048), _shared_text(3, 512)
    documents = [{'shared_checkpoint_text': first, 'second_shared_field': second, 'n': i}
                 for i in range(MOVING_KEYS + OTHER_KEYS)]
    moving = {f'{MOVING}{i}': documents[i] for i in range(MOVING_KEYS)}
    other = {f'other:{i}': documents[MOVING_KEYS + i] for i in range(OTHER_KEYS)}
    return moving, other


def _load_documents(env, documents):
    cluster = env.getClusterConnectionIfNeeded()
    for key, document in documents.items():
        env.assertEqual(cluster.execute_command('JSON.SET', key, '$', json.dumps(document)), 'OK')


def _source_and_target(env):
    """The slot of the moving keys, the shard that holds it and the other shard."""
    slot = env.getClusterConnectionIfNeeded().execute_command('CLUSTER', 'KEYSLOT', f'{MOVING}0')
    shards = env.envRunner.shards
    source = next(shard for shard in shards
                  if shard.getConnection().execute_command('CLUSTER', 'COUNTKEYSINSLOT', slot))
    target = next(shard for shard in shards if shard is not source)
    return slot, source, target


def _keys(connection):
    return set(connection.scan_iter(count=1000))


def _assert_documents(env, connection, documents, keys):
    """The server holds exactly `keys`, each as `documents` has it."""
    env.assertEqual(_keys(connection), keys)
    for key in keys:
        env.assertEqual(json.loads(connection.execute_command('JSON.GET', key)), documents[key],
                        message=key)


@contextmanager
def _moving_slot(env, slot, source, target):
    """The steps of `redis-cli --cluster reshard` around the MIGRATE calls."""
    source_id = source.execute_command('CLUSTER', 'MYID')
    target_id = target.execute_command('CLUSTER', 'MYID')
    target.execute_command('CLUSTER', 'SETSLOT', slot, 'IMPORTING', source_id)
    source.execute_command('CLUSTER', 'SETSLOT', slot, 'MIGRATING', target_id)
    try:
        yield
    finally:
        for connection in (target, source):
            connection.execute_command('CLUSTER', 'SETSLOT', slot, 'NODE', target_id)
    env.assertEqual(source.execute_command('CLUSTER', 'COUNTKEYSINSLOT', slot), 0)


def _migrate(source, target, keys, *options):
    port = target.info('server')['tcp_port']
    return source.execute_command('MIGRATE', 'localhost', port, '', 0, 10000, *options, 'KEYS', *keys)


@contextmanager
def _key_save_delay(connection, microseconds):
    """Makes a save child sleep `microseconds` after each key."""
    original = connection.config_get('rdb-key-save-delay').get('rdb-key-save-delay')
    connection.config_set('rdb-key-save-delay', microseconds)
    try:
        yield
    finally:
        connection.config_set('rdb-key-save-delay', original or 0)


class _ServerLog:
    """The module's RDB log lines that one master wrote since the watch started."""

    def __init__(self, env, shard):
        self.path = os.path.join(env.logDir, shard._getFileName('master', '.log'))
        try:
            self.start = len(self._lines())
        except OSError:
            # Without the log nothing shows that a stream was written or read.
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


def _expect_no_refusal(env, log):
    lines = log.lines()
    env.assertFalse(any('status=refused' in line for line in lines), message=str(lines))


def _expect_stream_save(env, log, min_refs):
    """The last save since the watch started wrote a stream, counted by the process that ran it."""
    lines = log.lines()
    started = [line for line in lines if 'phase=persistence_start' in line]
    env.assertTrue(started, message=f'no save started: {lines}')
    if not started:
        return
    env.assertContains('mode=stream', started[-1])
    pid = _log_pid(started[-1])
    ended = [line for line in lines if 'phase=persistence_ended' in line and _log_pid(line) == pid]
    env.assertTrue(ended, message=f'no stream summary: {lines}')
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


def _reload_and_check(env, shard, documents):
    """`DEBUG RELOAD` saves a stream and loads it back, and the documents come back."""
    connection = shard.getConnection()
    keys = _keys(connection)
    env.assertGreater(len(keys), 1)
    log = _ServerLog(env, shard)
    env.assertEqual(connection.execute_command('DEBUG', 'RELOAD'), 'OK')
    _assert_documents(env, connection, documents, keys)
    _expect_stream_save(env, log, min_refs=len(keys) - 1)
    _expect_stream_load(env, log, min_refs=len(keys) - 1)


def test_a_resharded_slot_keeps_its_shared_string_documents():
    env = _cluster_env()
    moving, other = _documents()
    _load_documents(env, {**moving, **other})
    slot, source, target = _source_and_target(env)
    source_connection, target_connection = source.getConnection(), target.getConnection()
    # The target holds documents of its own, read from a stream, before the slot arrives.
    _reload_and_check(env, target, other)
    source_keys, target_keys = _keys(source_connection), _keys(target_connection)
    target_log = _ServerLog(env, target)

    keys = sorted(moving)
    with _moving_slot(env, slot, source_connection, target_connection):
        port = target_connection.info('server')['tcp_port']
        env.assertEqual(source_connection.execute_command(
            'MIGRATE', 'localhost', port, keys[0], 0, 10000), 'OK')
        env.assertEqual(_migrate(source_connection, target_connection, keys[1:], 'COPY'), 'OK')
        env.assertEqual(source_connection.execute_command('CLUSTER', 'COUNTKEYSINSLOT', slot),
                        len(keys) - 1)
        # REPLACE writes the source's changed documents over the copies.
        for key in keys[1:]:
            moving[key]['n'] = -moving[key]['n']
            source_connection.execute_command('JSON.SET', key, '$.n', moving[key]['n'])
        env.assertEqual(_migrate(source_connection, target_connection, keys[1:], 'REPLACE'), 'OK')

    _expect_no_refusal(env, target_log)
    documents = {**moving, **other}
    # Every moved key is on the target, and none is left on the source.
    _assert_documents(env, target_connection, documents, target_keys | set(moving))
    _assert_documents(env, source_connection, documents, source_keys - set(moving))
    for shard in (source, target):
        _reload_and_check(env, shard, documents)


def test_a_slot_moves_while_the_source_runs_a_bgsave():
    env = _cluster_env()
    moving, other = _documents()
    documents = {**moving, **other}
    _load_documents(env, documents)
    slot, source, target = _source_and_target(env)
    source_connection, target_connection = source.getConnection(), target.getConnection()
    source_keys = source_connection.dbsize()
    target_keys = _keys(target_connection)
    source_log, target_log = _ServerLog(env, source), _ServerLog(env, target)

    # The child takes several seconds over the source's keys, so the slot moves during it.
    with _key_save_delay(source_connection, 20000):
        env.assertTrue(source_connection.bgsave())
        with _moving_slot(env, slot, source_connection, target_connection):
            env.assertEqual(_migrate(source_connection, target_connection, sorted(moving)), 'OK')
        env.assertEqual(source_connection.info('persistence')['rdb_bgsave_in_progress'], 1)
        with TimeLimit(60):
            while source_connection.info('persistence')['rdb_bgsave_in_progress']:
                time.sleep(0.01)
    env.assertEqual(source_connection.info('persistence')['rdb_last_bgsave_status'], 'ok')

    # The child saved the keys it had when it forked, the moving ones included.
    _expect_stream_save(env, source_log, min_refs=source_keys - 1)
    _expect_no_refusal(env, target_log)
    _assert_documents(env, target_connection, documents, target_keys | set(moving))
    _reload_and_check(env, target, documents)
