"""RDB encoding v5 with several shards (MOD-20882).

`test_rdb_dictionary.py` checks one server and skips cluster mode, and the CI cluster lane runs one
shard. These tests start their own cluster of 3 shards, each with a replica, and check that every
shard writes and loads its own stream: a save, a restart, `DEBUG RELOAD`, a replica full sync and a
failover. Each shard interns its own strings, so each one's stream shares only its own keys' strings.
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


SHARDS = 3
WRITER_BUDGET = 'json-rdb-dictionary-max-memory-percent'
LOG_MARK = 'JSON RDB dictionary'
# Enough keys that every shard holds dozens of them.
KEYS = 300


def _cluster_env():
    env = Env(shardsCount=SHARDS, useSlaves=True, decodeResponses=True)
    if env.env != 'oss-cluster':
        env.skip()
    return env


def _shared_text(seed, size=2048):
    rng = random.Random(seed)
    return ''.join(rng.choices(string.ascii_letters + string.digits, k=size))


def _documents():
    """Documents that repeat two long strings, under object keys long enough to be shared too."""
    first, second = _shared_text(seed=20882), _shared_text(seed=3, size=512)
    return {f'checkpoint:{i}': {'shared_checkpoint_text': first, 'second_shared_field': second, 'n': i}
            for i in range(KEYS)}


def _load_documents(env, documents):
    env.flush()
    cluster = env.getClusterConnectionIfNeeded()
    for key, document in documents.items():
        env.assertEqual(cluster.execute_command('JSON.SET', key, '$', json.dumps(document)), 'OK')
    for shard in env.envRunner.shards:
        # Every shard needs keys of its own, or it writes no stream to check.
        env.assertGreater(shard.getConnection().dbsize(), 1)


def _shard_keys(connection):
    return list(connection.scan_iter(count=1000))


def _assert_shard_documents(env, connection, documents):
    """The server holds exactly the documents of its slots, as they were written."""
    keys = _shard_keys(connection)
    env.assertGreater(len(keys), 1)
    for key in keys:
        env.assertEqual(json.loads(connection.execute_command('JSON.GET', key)), documents[key])
    return keys


class _ServerLog:
    """The module's RDB log lines that one server wrote since the watch started."""

    def __init__(self, env, shard, role):
        self.path = os.path.join(env.logDir, shard._getFileName(role, '.log'))
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


def _expect_stream_save(env, log, min_refs):
    """The last save since the watch started wrote a stream that defined and referenced strings.
    Only the lines of the process that ran that save count, since a replica's synchronization can
    start another save in the same window."""
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
    """A load since the watch started read a stream that referenced strings."""
    lines = log.lines()
    loaded = [line for line in lines if 'phase=loading_ended' in line]
    env.assertTrue(loaded, message=f'no stream load: {lines}')
    if loaded:
        env.assertGreaterEqual(_field(loaded[-1], 'refs'), min_refs)
    env.assertFalse(any('status=refused' in line for line in lines), message=str(lines))


def _retry_while_saving(operation):
    # A replica's synchronization can start a background save between commands.
    with TimeLimit(30):
        while True:
            try:
                return operation()
            except ResponseError as error:
                if 'Background save already in progress' not in str(error):
                    raise
                time.sleep(0.01)


def _rdb_size(connection):
    directory = connection.config_get('dir')['dir']
    filename = connection.config_get('dbfilename')['dbfilename']
    return os.path.getsize(os.path.join(directory, filename))


def _wait_for_replica(env, master):
    """Redis refuses SAVE on a replica before its first full synchronization."""
    env.assertEqual(master.execute_command('WAIT', 1, 30000), 1)


def _wait_for_link(replica):
    with TimeLimit(60):
        while replica.info('replication').get('master_link_status') != 'up':
            time.sleep(0.05)


def test_every_shard_saves_and_reloads_its_own_stream():
    env = _cluster_env()
    documents = _documents()
    _load_documents(env, documents)
    for shard in env.envRunner.shards:
        master = shard.getConnection()
        keys = len(_shard_keys(master))
        original = master.config_get(WRITER_BUDGET)[WRITER_BUDGET]
        try:
            master.config_set(WRITER_BUDGET, '0')
            env.assertTrue(_retry_while_saving(master.save))
            inline_size = _rdb_size(master)
        finally:
            master.config_set(WRITER_BUDGET, original)
        log = _ServerLog(env, shard, 'master')
        env.assertTrue(_retry_while_saving(master.save))
        stream_size = _rdb_size(master)
        # Every key after the first refers to the two strings and the two object keys it repeats.
        _expect_stream_save(env, log, min_refs=keys - 1)
        env.assertLess(stream_size * 4, inline_size,
                       message=f'stream RDB {stream_size} vs inline RDB {inline_size}')

    # A restart loads the RDB that the last SAVE wrote.
    for shard in env.envRunner.shards:
        log = _ServerLog(env, shard, 'master')
        shard.stopEnv()
        shard.startEnv()
        env.waitCluster()
        keys = _assert_shard_documents(env, shard.getConnection(), documents)
        _expect_stream_load(env, log, min_refs=len(keys) - 1)

    # DEBUG RELOAD saves and loads again in one process, on the master and its replica.
    for index, shard in enumerate(env.envRunner.shards, start=1):
        _wait_for_replica(env, shard.getConnection())
        log = _ServerLog(env, shard, 'master')
        _retry_while_saving(lambda: env.dumpAndReload(shardId=index))
        keys = _assert_shard_documents(env, shard.getConnection(), documents)
        _expect_stream_save(env, log, min_refs=len(keys) - 1)
        _expect_stream_load(env, log, min_refs=len(keys) - 1)


def test_every_replica_full_syncs_a_stream_from_its_master():
    env = _cluster_env()
    documents = _documents()
    _load_documents(env, documents)
    for shard in env.envRunner.shards:
        master = shard.getConnection()
        replica = shard.getSlaveConnection()
        _wait_for_replica(env, master)
        delay = master.config_get('repl-diskless-sync-delay')['repl-diskless-sync-delay']
        master_log = _ServerLog(env, shard, 'master')
        replica_log = _ServerLog(env, shard, 'slave')
        try:
            master.config_set('repl-diskless-sync-delay', '0')
            full_syncs = master.info('stats')['sync_full']
            # A new replication id makes the replica's reconnection a full synchronization.
            master.execute_command('DEBUG', 'CHANGE-REPL-ID')
            master.execute_command('CLIENT', 'KILL', 'TYPE', 'replica')
            with TimeLimit(60):
                while (master.info('stats')['sync_full'] <= full_syncs or
                       replica.info('replication')['master_link_status'] != 'up'):
                    time.sleep(0.01)
        finally:
            master.config_set('repl-diskless-sync-delay', delay)
        keys = _shard_keys(master)
        _expect_stream_save(env, master_log, min_refs=len(keys) - 1)
        _expect_stream_load(env, replica_log, min_refs=len(keys) - 1)
        for key in keys:
            env.assertEqual(json.loads(replica.execute_command('JSON.GET', key)), documents[key])


def test_a_promoted_replica_saves_and_reloads_its_own_stream():
    env = _cluster_env()
    documents = _documents()
    _load_documents(env, documents)
    shard = env.envRunner.shards[0]
    master = shard.getConnection()
    replica = shard.getSlaveConnection()
    _wait_for_replica(env, master)
    # RLTest starts each shard's replica outside cluster mode, so it is promoted with REPLICAOF.
    replica.execute_command('REPLICAOF', 'NO', 'ONE')
    try:
        # The promoted server writes its own stream from the strings it interned while it replicated.
        log = _ServerLog(env, shard, 'slave')
        env.assertTrue(_retry_while_saving(replica.save))
        keys = _assert_shard_documents(env, replica, documents)
        _expect_stream_save(env, log, min_refs=len(keys) - 1)
        log = _ServerLog(env, shard, 'slave')
        _retry_while_saving(lambda: replica.execute_command('DEBUG', 'RELOAD'))
        _assert_shard_documents(env, replica, documents)
        _expect_stream_load(env, log, min_refs=len(keys) - 1)
    finally:
        # Attach it back to its master, as the other tests expect.
        replica.execute_command('REPLICAOF', 'localhost', master.info('server')['tcp_port'])
        _wait_for_link(replica)
