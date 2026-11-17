"""Truncated and partial RDB encoding v5 input (MOD-20882).

`test_rdb_dictionary.py` refuses a stream that lost its first definition, has an unknown marker or
has a reset record that defines nothing. `test_short_read.py` cuts only an RDB that RedisJSON 2.0.0
wrote. These tests cut a v5 stream RDB short, drop records from the middle of one, and cut a
replica's full sync while it loads the stream from the socket. A server that loads a damaged file
must stop without crashing, and a replica whose sync was cut must retry and load the whole stream.
"""

import json
import os
import random
import re
import shutil
import socket
import string
import struct
import subprocess
import tempfile
import threading
import time

from redis import Redis
from redis.exceptions import ConnectionError as RedisConnectionError
from redis.exceptions import ResponseError
from redis.exceptions import TimeoutError as RedisTimeoutError

from RLTest import Env

from common import TimeLimit, VALGRIND


WRITER_BUDGET = 'json-rdb-dictionary-max-memory-percent'
LOG_MARK = 'JSON RDB dictionary'
# How Redis reports an RDB file that ends early, and how a server that crashed logs it.
SHORT_READ = 'Short read or OOM loading DB'
CRASH_MARKS = ('crashed by signal', 'BUG REPORT', 'panicked')

RDB_OPCODE_AUX = 250
RDB_OPCODE_RESIZEDB = 251
RDB_OPCODE_SELECTDB = 254
RDB_OPCODE_EOF = 255
RDB_TYPE_MODULE_2 = 7
RDB_MODULE_OPCODE_STRING = 5
RDB_MODULE_OPCODE_EOF = 0
# The first byte of a value record: 1 and 2 belong to a stream and carry a base id after it.
MARKER_STREAM = 1
MARKER_RESET = 2

# At 1% of a test server's memory the table of these keys fills up and resets.
RESET_KEYS = 3000
# Keys of the replica test.
SYNC_KEYS = 400


def _shared_text(seed, size):
    rng = random.Random(seed)
    return ''.join(rng.choices(string.ascii_letters + string.digits, k=size))


def _set_documents(env, documents):
    for key, document in documents.items():
        env.expect('JSON.SET', key, '$', json.dumps(document)).ok()


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


def _uncompressed_rdb(env, budget=None):
    """The RDB of a SAVE without compression, so that its records are plain bytes, written with
    the writer percentage `budget` when given."""
    connection = env.getConnection()
    original = {name: connection.config_get(name)[name] for name in ('rdbcompression', WRITER_BUDGET)}
    try:
        connection.config_set('rdbcompression', 'no')
        if budget is not None:
            connection.config_set(WRITER_BUDGET, budget)
        env.assertTrue(_retry_while_saving(connection.save))
        path = os.path.join(connection.config_get('dir')['dir'],
                            connection.config_get('dbfilename')['dbfilename'])
        with open(path, 'rb') as rdb:
            return rdb.read()
    finally:
        for name, value in original.items():
            connection.config_set(name, value)


def _rdb_load_len(buf, off):
    """Decode an rdbSaveLen-encoded integer; returns (value, next_offset)."""
    b = buf[off]
    kind = (b & 0xC0) >> 6
    if kind == 0:
        return b & 0x3F, off + 1
    elif kind == 1:
        return ((b & 0x3F) << 8) | buf[off + 1], off + 2
    elif b == 0x80:
        return struct.unpack('>I', buf[off + 1:off + 5])[0], off + 5
    elif b == 0x81:
        return struct.unpack('>Q', buf[off + 1:off + 9])[0], off + 9
    raise ValueError('unexpected rdb length encoding: 0x%02x' % b)


def _rdb_load_string(buf, off):
    """Decode a string saved without compression; returns (bytes, next_offset)."""
    if buf[off] >> 6 == 3:  # an integer stored in 8, 16 or 32 bits
        size = {0: 1, 1: 2, 2: 4}[buf[off] & 0x3F]
        return bytes(buf[off + 1:off + 1 + size]), off + 1 + size
    length, off = _rdb_load_len(buf, off)
    return bytes(buf[off:off + length]), off + length


def _parse_rdb(data):
    """The module's value records of an RDB saved without compression, in file order. Each has
    the `span` of the whole value and the `record` span of the one string the module saved."""
    assert data[:5] == b'REDIS', 'not an RDB file'
    values = []
    off = 9  # the signature and the four digit version
    while data[off] != RDB_OPCODE_EOF:
        opcode = data[off]
        if opcode == RDB_OPCODE_AUX:
            _, off = _rdb_load_string(data, off + 1)
            _, off = _rdb_load_string(data, off)
        elif opcode == RDB_OPCODE_SELECTDB:
            _, off = _rdb_load_len(data, off + 1)
        elif opcode == RDB_OPCODE_RESIZEDB:
            _, off = _rdb_load_len(data, off + 1)
            _, off = _rdb_load_len(data, off)
        elif opcode == RDB_TYPE_MODULE_2:
            start = off
            _, off = _rdb_load_string(data, off + 1)  # the key
            _, off = _rdb_load_len(data, off)  # the module id
            kind, off = _rdb_load_len(data, off)
            assert kind == RDB_MODULE_OPCODE_STRING
            length, record_start = _rdb_load_len(data, off)
            off = record_start + length
            kind, off = _rdb_load_len(data, off)
            assert kind == RDB_MODULE_OPCODE_EOF
            values.append({'span': (start, off), 'record': (record_start, record_start + length)})
        else:
            raise ValueError(f'unexpected RDB opcode {opcode} at offset {off}')
    return values


def _marker(data, value):
    return data[value['record'][0]]


def _base_id(data, value):
    start = value['record'][0] + 1
    return struct.unpack('<I', data[start:start + 4])[0]


def _stream_values(data):
    return [value for value in _parse_rdb(data) if _marker(data, value) in (MARKER_STREAM, MARKER_RESET)]


def _without(data, span):
    return data[:span[0]] + data[span[1]:]


def _wait_for_load(process, connection):
    """'up' once the server answers, 'exited' when it stops first, as one that refuses its file
    does, or 'crashed' when a signal killed it."""
    deadline = time.monotonic() + 60
    while time.monotonic() < deadline:
        if process.poll() is not None:
            return 'exited' if process.returncode >= 0 else 'crashed'
        try:
            connection.ping()
            return 'up'
        except (RedisConnectionError, RedisTimeoutError):
            time.sleep(0.05)
    return 'timeout'


def _load_rdb_in_new_server(env, rdb, inspect=None):
    """Starts a server on a copy of `rdb`, with the module settings of `env`, then stops it.
    Returns how the start went, 'up', 'exited', 'crashed' or 'timeout', and the server log.
    `inspect` gets a connection while it is up."""
    directory = tempfile.mkdtemp(prefix='rdbcut')
    socket_path = os.path.join(directory, 'redis.sock')
    log_path = os.path.join(directory, 'redis.log')
    with open(os.path.join(directory, 'dump.rdb'), 'wb') as dump:
        dump.write(rdb)
    runner = env.envRunner
    command = list(runner.masterCmdArgs) + [
        '--port', '0', '--unixsocket', socket_path, '--dir', directory, '--dbfilename', 'dump.rdb',
        '--logfile', log_path, '--loglevel', 'notice', '--rdbchecksum', 'no', '--save', '',
        '--appendonly', 'no']
    process = subprocess.Popen(command, cwd=directory, env=runner.masterOSEnv,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    connection = Redis(unix_socket_path=socket_path, decode_responses=True, socket_timeout=5)
    try:
        outcome = _wait_for_load(process, connection)
        if outcome == 'up' and inspect:
            inspect(connection)
    finally:
        connection.close()
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=30)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        try:
            with open(log_path, errors='replace') as log:
                lines = [line.strip() for line in log]
        except OSError:
            lines = []
        shutil.rmtree(directory, ignore_errors=True)
    return outcome, lines


def _skip_unless_new_servers_can_start(env):
    """The tests that load an edited file start a server of their own with the harness' command."""
    env.skipOnCluster()
    if (env.useAof or VALGRIND or env.useTLS or env.env.endswith('existing-env') or
            not hasattr(env.envRunner, 'masterCmdArgs')):
        env.skip()


def _assert_loads(env, rdb, documents):
    """The file as saved is the control: if it did not load, a stop below would show nothing."""
    def read_documents(connection):
        for key, document in documents.items():
            env.assertEqual(json.loads(connection.execute_command('JSON.GET', key)), document)

    outcome, lines = _load_rdb_in_new_server(env, rdb, read_documents)
    env.assertEqual(outcome, 'up', message=str(lines))
    env.assertTrue(any('phase=loading_ended' in line for line in lines), message=str(lines))


def test_a_stream_rdb_cut_short_stops_the_load_without_a_crash():
    env = Env(decodeResponses=True)
    _skip_unless_new_servers_can_start(env)
    env.expect('FLUSHALL').equal(True)
    first, second = _shared_text(seed=37, size=2048), _shared_text(seed=38, size=512)
    documents = {f'cut:{i}': {'first_shared_text': first, 'second_shared_text': second, 'n': i}
                 for i in range(40)}
    _set_documents(env, documents)
    rdb = _uncompressed_rdb(env)
    _assert_loads(env, rdb, documents)

    values = _parse_rdb(rdb)
    env.assertEqual(len(values), len(documents))
    # The first value defines the shared strings and the second refers to them.
    env.assertEqual([_marker(rdb, value) for value in values[:2]], [MARKER_STREAM, MARKER_STREAM])
    defining, referring = values[0], values[1]

    def middle(span):
        return (span[0] + span[1]) // 2

    cuts = {
        'before the first value': defining['span'][0],
        'inside the first key name': defining['span'][0] + 3,
        'inside the base id of the first record': defining['record'][0] + 2,
        'inside the first definition': middle(defining['record']),
        'between the first and the second value': referring['span'][0],
        'inside a record that refers to strings': middle(referring['record']),
        'before the end mark of a value': referring['record'][1],
        'between two values in the middle': values[len(values) // 2]['span'][0],
        'after the last value': values[-1]['span'][1],
    }
    for name, cut in cuts.items():
        outcome, lines = _load_rdb_in_new_server(env, rdb[:cut])
        message = f'{name} (byte {cut} of {len(rdb)}): {lines}'
        # Redis stops at the end of the file, wherever it falls. The module reads a value that
        # was cut as a short read, which is not a refused record, and keeps nothing from it.
        env.assertEqual(outcome, 'exited', message=message)
        env.assertTrue(any(SHORT_READ in line for line in lines), message=message)
        env.assertFalse(any('status=refused' in line for line in lines), message=message)
        env.assertFalse(any(mark in line for line in lines for mark in CRASH_MARKS), message=message)


def test_a_stream_rdb_missing_a_record_from_its_middle_is_refused():
    env = Env(decodeResponses=True)
    _skip_unless_new_servers_can_start(env)
    env.expect('FLUSHALL').equal(True)
    # Two groups of keys with a shared string each, so that some value after the first of the file
    # defines a string.
    groups = [_shared_text(seed=41, size=1024), _shared_text(seed=42, size=1024)]
    documents = {f'middle:{i}': {'group_shared_text': groups[i % 2], 'n': i} for i in range(20)}
    _set_documents(env, documents)
    rdb = _uncompressed_rdb(env)
    _assert_loads(env, rdb, documents)

    # A record after the first that defines strings: the base id after it is higher.
    stream = _stream_values(rdb)
    pairs = list(zip(stream, stream[1:]))
    dropped, following = next((record, after) for record, after in pairs[1:]
                              if _base_id(rdb, after) > _base_id(rdb, record))
    cases = {'a later definition dropped': (rdb, dropped, following)}

    # A reset record. At 1% of the memory the table of these keys resets.
    env.expect('FLUSHALL').equal(True)
    resets = {}
    for i in range(RESET_KEYS):
        text = f'string that is repeated within key {i:04}'
        resets[f'reset:{i}'] = [text, text]
    _set_documents(env, resets)
    reset_rdb = _uncompressed_rdb(env, budget=1)
    _assert_loads(env, reset_rdb, {})
    stream = _stream_values(reset_rdb)
    index = next(i for i, value in enumerate(stream) if _marker(reset_rdb, value) == MARKER_RESET)
    env.assertLess(index + 1, len(stream), message='no stream record after the first reset')
    cases['a reset record dropped'] = (reset_rdb, stream[index], stream[index + 1])

    for name, (data, dropped, following) in cases.items():
        # Base ids count every definition of the stream, across resets as well, so the record
        # after the gap names more definitions than the reader has seen.
        expected, found = _base_id(data, dropped), _base_id(data, following)
        env.assertLess(expected, found, message=name)
        reason = (f'phase=load status=refused reason=base_id_mismatch '
                  f'expected={expected} found={found}')
        outcome, lines = _load_rdb_in_new_server(env, _without(data, dropped['span']))
        env.assertEqual(outcome, 'exited', message=f'{name}: {lines}')
        env.assertTrue(any(reason in line for line in lines),
                       message=f'{name}: expected {reason}, got {lines}')
        env.assertFalse(any(mark in line for line in lines for mark in CRASH_MARKS),
                        message=f'{name}: {lines}')


class _ServerLog:
    """The lines one server of `env` logged since the watch started."""

    def __init__(self, env, role):
        self.path = os.path.join(env.logDir, env.envRunner._getFileName(role, '.log'))
        try:
            self.start = len(self._lines())
        except OSError:
            # Without the log nothing shows where the load stopped and how it ended.
            env.skip()

    def _lines(self):
        with open(self.path, errors='replace') as log:
            return [line.strip() for line in log]

    def lines(self):
        return self._lines()[self.start:]


def _field(line, name):
    return int(re.search(rf'\b{name}=(\d+)', line).group(1))


def _read_command(reader):
    """One command a replica sends: a RESP array of bulk strings."""
    header = reader.readline()
    if not header.startswith(b'*'):
        raise ConnectionError(f'unexpected replica input {header!r}')
    arguments = []
    for _ in range(int(header[1:])):
        length = int(reader.readline()[1:])
        arguments.append(reader.read(length + 2)[:-2])
    return arguments


class _MockMaster:
    """A master that sends each replica connection one of `payloads`, the bytes of an RDB that
    may be cut short, announced with the length of the whole file. It closes every connection
    after its payload but the last, which stays open as a healthy link does."""

    def __init__(self, rdb, payloads):
        self.rdb, self.payloads = rdb, payloads
        self.server = socket.create_server(('127.0.0.1', 0))
        self.server.settimeout(60)
        self.port = self.server.getsockname()[1]
        self.connections = []
        self.error = None
        self.thread = threading.Thread(target=self._serve, daemon=True)
        self.thread.start()

    def _serve(self):
        try:
            for index, payload in enumerate(self.payloads):
                connection, _ = self.server.accept()
                self.connections.append(connection)
                self._handshake(connection)
                connection.sendall(b'$%d\r\n' % len(self.rdb) + payload)
                if index + 1 < len(self.payloads):
                    connection.close()
        except Exception as error:  # reported by close()
            self.error = error

    @staticmethod
    def _handshake(connection):
        reader = connection.makefile('rb')
        while True:
            name = _read_command(reader)[0].upper()
            if name == b'PING':
                connection.sendall(b'+PONG\r\n')
            elif name == b'PSYNC':
                connection.sendall(b'+FULLRESYNC ' + b'0' * 40 + b' 0\r\n')
                return
            else:
                connection.sendall(b'+OK\r\n')

    def close(self):
        for connection in self.connections:
            connection.close()
        self.server.close()
        self.thread.join(timeout=10)
        return self.error


def test_a_full_sync_cut_while_the_replica_loads_is_retried_and_loads_the_whole_stream():
    env = Env(decodeResponses=True)
    env.skipOnCluster()
    # The server of the plain lane becomes the replica of a mock master.
    if env.useSlaves or env.useAof or env.env.endswith('existing-env'):
        env.skip()
    connection = env.getConnection()
    original = connection.config_get('repl-diskless-load')['repl-diskless-load']
    env.expect('FLUSHALL').equal(True)
    shared = _shared_text(seed=43, size=2048)
    documents = {f'sync:{i}': {'shared_sync_text': shared, 'n': i} for i in range(SYNC_KEYS)}
    _set_documents(env, documents)
    rdb = _uncompressed_rdb(env)
    values = _parse_rdb(rdb)
    middle = values[len(values) // 2]
    env.assertEqual(_marker(rdb, middle), MARKER_STREAM)
    # The first transfer ends inside a record that refers to the shared strings, so the module
    # reads it short; the second ends between two records, so the load fails with the module's
    # stream half read. Only the third transfer is whole.
    cuts = [(middle['record'][0] + middle['record'][1]) // 2, middle['span'][0]]
    log = _ServerLog(env, 'master')
    env.expect('FLUSHALL').equal(True)
    master = _MockMaster(rdb, [rdb[:cut] for cut in cuts] + [rdb])
    try:
        # The replica loads straight from the socket into its empty database.
        connection.config_set('repl-diskless-load', 'on-empty-db')
        connection.execute_command('REPLICAOF', '127.0.0.1', master.port)
        with TimeLimit(60):
            while (connection.info('replication').get('master_link_status') != 'up' or
                   connection.info('persistence')['loading']):
                time.sleep(0.05)

        lines = log.lines()
        failed = [i for i, line in enumerate(lines) if 'Failure loading rdb format from socket' in line]
        env.assertEqual(len(failed), len(cuts), message=str(lines))
        env.assertFalse(any(mark in line for line in lines for mark in CRASH_MARKS), message=str(lines))
        # The last load read the whole stream from a clean start: state left from a cut load would
        # make it refuse a record.
        ended = [line for line in lines[failed[-1] if failed else 0:] if 'phase=loading_ended' in line]
        env.assertTrue(ended, message=str(lines))
        if ended:
            env.assertGreaterEqual(_field(ended[-1], 'refs'), SYNC_KEYS - 1)
        env.assertFalse(any('status=refused' in line for line in lines), message=str(lines))
        env.assertEqual(connection.dbsize(), SYNC_KEYS)
        for key, document in documents.items():
            env.assertEqual(json.loads(connection.execute_command('JSON.GET', key)), document)
    finally:
        connection.execute_command('REPLICAOF', 'NO', 'ONE')
        connection.config_set('repl-diskless-load', original)
        error = master.close()
        env.assertIsNone(error, message=repr(error))
        env.expect('FLUSHALL').equal(True)
