"""RDB encoding v5 on every full-sync path of a replica (MOD-20882).

`test_rdb_dictionary.py` syncs a replica twice: the master writes the RDB to disk and the replica
loads it from a file, or the master writes to the socket over the RDB channel and an empty replica
loads from it. These tests cover the other paths: the socket sync on the replication link itself, a
replica asked for `swapdb`, a replica of a replica, and a replica that restarts on its own RDB
before it syncs again. Each path must save a stream on the sending side and load it on the
receiving side.

`swapdb` never runs with RedisJSON loaded: Redis loads into a temporary database only when every
module handles async loading, which RedisJSON doesn't declare, so the replica loads from a file. The
test pins that, since a module that declared it would load through events the load session doesn't
handle.

They run in the `--use-slaves` lane.
"""

import json
import os
import random
import re
import shutil
import string
import subprocess
import tempfile
import time

from redis import Redis
from redis.exceptions import ConnectionError as RedisConnectionError
from RLTest import Env

from common import TimeLimit


LOG_MARK = 'JSON RDB dictionary'
KEYS = 200


def _replication_env():
    env = Env(decodeResponses=True)
    if not env.useSlaves or env.isCluster() or env.env.endswith('existing-env'):
        env.skip()
    return env


def _shared_text(seed, size):
    rng = random.Random(seed)
    return ''.join(rng.choices(string.ascii_letters + string.digits, k=size))


def _documents(prefix, seed):
    """Documents that repeat two long strings, under object keys long enough to be shared too."""
    first, second = _shared_text(seed, 2048), _shared_text(seed + 1, 512)
    return {f'{prefix}:{i}': {'shared_checkpoint_text': first, 'second_shared_field': second, 'n': i}
            for i in range(KEYS)}


def _set_documents(env, connection, documents):
    for key, document in documents.items():
        env.assertEqual(connection.execute_command('JSON.SET', key, '$', json.dumps(document)), 'OK')


def _assert_documents(env, connection, documents):
    """The server holds exactly these documents, as they were written."""
    env.assertEqual(connection.dbsize(), len(documents))
    for key, document in documents.items():
        env.assertEqual(json.loads(connection.execute_command('JSON.GET', key)), document)


class _Log:
    """The module's RDB log lines that one server wrote since the watch started."""

    def __init__(self, env, path, from_start=False):
        self.path = path
        try:
            self.start = 0 if from_start else len(self._lines())
        except OSError:
            # Without the log nothing shows that a stream was written or read.
            env.skip()

    def _lines(self):
        with open(self.path, errors='replace') as log:
            return [line.strip() for line in log if LOG_MARK in line]

    def lines(self):
        return self._lines()[self.start:]


def _server_log(env, role):
    return _Log(env, os.path.join(env.logDir, env.envRunner._getFileName(role, '.log')))


def _field(line, name):
    return int(re.search(rf'\b{name}=(\d+)', line).group(1))


def _log_pid(line):
    match = re.match(r'(\d+):', line)
    return match and match.group(1)


def _expect_stream_save(env, log, min_refs):
    """The last save since the watch started wrote a stream that defined and referenced strings.
    Only the lines of the process that ran that save count."""
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


def _wait_for_link(connection):
    with TimeLimit(60):
        while connection.info('replication').get('master_link_status') != 'up':
            time.sleep(0.05)


def _wait_for_replica(env, master):
    env.assertEqual(master.execute_command('WAIT', 1, 30000), 1)


# The master's child streams the RDB in every case. Replica `repl-diskless-load`, `repl-rdb-channel`
# on both sides, whether the replica still holds keys of its own when it syncs, and whether it reads
# the RDB straight off the socket or from a file it writes first.
FULL_SYNCS = [
    # The RDB comes on its own connection, and the replica writes it to a file, then loads it.
    ('disabled', 'yes', True, 'file'),
    # The RDB comes on the replication link itself, and the empty replica parses it off the socket.
    ('on-empty-db', 'no', False, 'socket'),
    # `swapdb` falls back to a file (see the module docstring).
    ('swapdb', 'yes', True, 'file'),
]


def test_every_full_sync_mode_loads_a_stream():
    env = _replication_env()
    master = env.getConnection()
    replica = env.getSlaveConnection()
    port = master.info('server')['tcp_port']
    documents = _documents('checkpoint', seed=20882)
    _set_documents(env, master, documents)
    _wait_for_replica(env, master)
    master_settings = ('repl-diskless-sync', 'repl-diskless-sync-delay', 'repl-rdb-channel')
    original_master = {name: master.config_get(name)[name] for name in master_settings}
    replica_settings = ('repl-diskless-load', 'repl-rdb-channel')
    original_replica = {name: replica.config_get(name)[name] for name in replica_settings}
    try:
        for diskless_load, rdb_channel, replica_keeps_keys, reads_from in FULL_SYNCS:
            case = f'diskless-load={diskless_load} rdb-channel={rdb_channel}'
            master.config_set('repl-diskless-sync', 'yes')
            master.config_set('repl-diskless-sync-delay', '0')
            master.config_set('repl-rdb-channel', rdb_channel)
            replica.config_set('repl-diskless-load', diskless_load)
            replica.config_set('repl-rdb-channel', rdb_channel)
            replica.execute_command('REPLICAOF', 'NO', 'ONE')
            replica.flushall()
            if replica_keeps_keys:
                # Keys of its own, with strings of their own, that the sync must replace.
                _set_documents(env, replica, _documents('old', seed=7))
            master_log = _server_log(env, 'master')
            replica_log = _server_log(env, 'slave')
            with open(replica_log.path, errors='replace') as log:
                replica_lines_before = sum(1 for _ in log)
            # A new replication id makes the reconnection a full synchronization.
            master.execute_command('DEBUG', 'CHANGE-REPL-ID')
            full_syncs = master.info('stats')['sync_full']
            replica.execute_command('REPLICAOF', '127.0.0.1', port)
            env.debugPrint(case, force=True)
            with TimeLimit(60):
                while (master.info('stats')['sync_full'] <= full_syncs or
                       replica.info('replication')['master_link_status'] != 'up'):
                    time.sleep(0.01)
            _expect_stream_save(env, master_log, min_refs=KEYS - 1)
            _expect_stream_load(env, replica_log, min_refs=KEYS - 1)
            _assert_documents(env, replica, documents)
            # The replica's log says which connection carried the RDB and where it read it from.
            with open(replica_log.path, errors='replace') as log:
                lines = log.readlines()[replica_lines_before:]
            used_channel = any('initialize RDB channel' in line for line in lines)
            env.assertEqual(used_channel, rdb_channel == 'yes', message=case)
            from_socket = any('EOF to parser' in line for line in lines)
            env.assertEqual(from_socket, reads_from == 'socket', message=case)
            if diskless_load == 'swapdb':
                env.assertTrue(any('not aware of async replication' in line for line in lines),
                               message=case)
    finally:
        for name, value in original_master.items():
            master.config_set(name, value)
        for name, value in original_replica.items():
            replica.config_set(name, value)
        replica.execute_command('REPLICAOF', '127.0.0.1', port)
        _wait_for_link(replica)


def _start_replica_of(env, port):
    """Starts a server with the module settings of `env` that replicates the server on `port`.
    Returns its process, a connection, its directory and its log path."""
    directory = tempfile.mkdtemp(prefix='rdbchain')
    socket_path = os.path.join(directory, 'redis.sock')
    log_path = os.path.join(directory, 'redis.log')
    runner = env.envRunner
    command = list(runner.masterCmdArgs) + [
        '--port', '0', '--unixsocket', socket_path, '--dir', directory, '--dbfilename', 'dump.rdb',
        '--logfile', log_path, '--loglevel', 'notice', '--save', '', '--appendonly', 'no',
        '--replicaof', '127.0.0.1', str(port)]
    process = subprocess.Popen(command, cwd=directory, env=runner.masterOSEnv,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    connection = Redis(unix_socket_path=socket_path, decode_responses=True, socket_timeout=10)
    with TimeLimit(60):
        while True:
            try:
                connection.ping()
                break
            except RedisConnectionError:
                env.assertIsNone(process.poll(), message='the replica of the replica stopped')
                time.sleep(0.05)
    return process, connection, directory, log_path


def _stop(process, connection, directory):
    connection.close()
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=30)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
    shutil.rmtree(directory, ignore_errors=True)


def test_a_replica_of_a_replica_syncs_a_stream_from_the_replica():
    env = _replication_env()
    master = env.getConnection()
    replica = env.getSlaveConnection()
    documents = _documents('checkpoint', seed=20882)
    _set_documents(env, master, documents)
    _wait_for_replica(env, master)
    delay = replica.config_get('repl-diskless-sync-delay')['repl-diskless-sync-delay']
    replica.config_set('repl-diskless-sync-delay', '0')
    replica_log = _server_log(env, 'slave')
    process, chained, directory, log_path = _start_replica_of(env, replica.info('server')['tcp_port'])
    try:
        chained_log = _Log(env, log_path, from_start=True)
        _wait_for_link(chained)
        # The replica's child writes a stream from the strings the replica interned as it loaded.
        _expect_stream_save(env, replica_log, min_refs=KEYS - 1)
        _expect_stream_load(env, chained_log, min_refs=KEYS - 1)
        _assert_documents(env, chained, documents)
        # Writes keep flowing down the chain after the full sync.
        later = {'checkpoint:later': dict(documents['checkpoint:0'], n=-1)}
        _set_documents(env, master, later)
        with TimeLimit(30):
            while not chained.exists('checkpoint:later'):
                time.sleep(0.05)
        _assert_documents(env, chained, dict(documents, **later))
    finally:
        _stop(process, chained, directory)
        replica.config_set('repl-diskless-sync-delay', delay)


def test_a_restarted_replica_reloads_its_own_stream_then_resyncs():
    env = _replication_env()
    master = env.getConnection()
    replica = env.getSlaveConnection()
    documents = _documents('checkpoint', seed=20882)
    _set_documents(env, master, documents)
    _wait_for_replica(env, master)
    replica_log = _server_log(env, 'slave')
    env.assertTrue(replica.save())
    _expect_stream_save(env, replica_log, min_refs=KEYS - 1)

    env.envRunner.stopEnv(masters=False, slaves=True)
    # Written while the replica is down, so it must catch up after it loads its own RDB.
    later = {f'later:{i}': dict(documents['checkpoint:0'], n=-i) for i in range(10)}
    _set_documents(env, master, later)
    stats = master.info('stats')
    replica_log = _server_log(env, 'slave')
    env.envRunner.startEnv(masters=False, slaves=True)
    replica = env.getSlaveConnection()
    _wait_for_link(replica)
    _wait_for_replica(env, master)
    _expect_stream_load(env, replica_log, min_refs=KEYS - 1)
    # The RDB carried the replication offset, so the replica resumed without a new full sync, and
    # the documents it holds are the ones it read from its own stream.
    after = master.info('stats')
    env.assertEqual(after['sync_full'], stats['sync_full'])
    env.assertGreater(after['sync_partial_ok'], stats['sync_partial_ok'])
    _assert_documents(env, replica, dict(documents, **later))
