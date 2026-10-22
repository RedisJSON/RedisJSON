import functools
import hashlib
import json
import os
import random
import re
import shutil
import string
import struct
import subprocess
import tempfile
import time

from RLTest import Defaults
from redis import Redis
from redis.exceptions import ConnectionError as RedisConnectionError
from redis.exceptions import ResponseError
from redis.exceptions import TimeoutError as RedisTimeoutError

from common import TimeLimit, no_san
from includes import VALGRIND


Defaults.decode_responses = True

WRITER_BUDGET = 'json-rdb-dictionary-max-bytes'
LOG_MARK = 'JSON RDB dictionary'

RDB_OPCODE_AUX = 250
RDB_OPCODE_RESIZEDB = 251
RDB_OPCODE_SELECTDB = 254
RDB_OPCODE_EOF = 255
RDB_TYPE_MODULE_2 = 7
RDB_MODULE_OPCODE_EOF = 0
RDB_MODULE_OPCODE_STRING = 5
# The first byte of a value record that belongs to a stream. A self-contained one starts with 0.
MARKER_STREAM = 1
MARKER_RESET = 2


def _binary_connection(env):
    options = dict(env.getConnection().connection_pool.connection_kwargs)
    options['decode_responses'] = False
    return Redis(**options)


def _is_flex(env):
    return env.getConnection().config_get('bigredis-enabled').get('bigredis-enabled') == 'yes'


def _values_in_ram(test):
    """Keeps the values of a test in RAM on Flex. The Flex harness runs with
    `--bigredis-max-ram-keys 0`, which evicts every value after the command that touched it, and
    the writer dedups the values that are in RAM; what Flex keeps on flash has tests of its own.
    Changes nothing elsewhere."""
    @functools.wraps(test)
    def wrapper(env, *args, **kwargs):
        if not _is_flex(env):
            return test(env, *args, **kwargs)
        connection = env.getConnection()
        limit = connection.config_get('bigredis-max-ram-keys')['bigredis-max-ram-keys']
        connection.config_set('bigredis-max-ram-keys', 1_000_000)
        try:
            return test(env, *args, **kwargs)
        finally:
            connection.config_set('bigredis-max-ram-keys', limit)
    return wrapper


def _rdb_path(env):
    connection = env.getConnection()
    directory = connection.config_get('dir')['dir']
    filename = connection.config_get('dbfilename')['dbfilename']
    return os.path.join(directory, filename)


def _rdb_size(env):
    return os.path.getsize(_rdb_path(env))


def _rdb_bytes(env):
    with open(_rdb_path(env), 'rb') as rdb:
        return rdb.read()


def _retry_while_saving(operation):
    # Replica synchronization can start a background save between commands.
    with TimeLimit(30):
        while True:
            try:
                return operation()
            except ResponseError as error:
                if 'Background save already in progress' not in str(error):
                    raise
                time.sleep(0.01)


def _save(env, command='SAVE'):
    return _retry_while_saving(lambda: env.cmd(command))


def _wait_for_bgsave(env):
    with TimeLimit(60):
        while env.getConnection().info('persistence')['rdb_bgsave_in_progress']:
            time.sleep(0.01)
    env.assertEqual(env.getConnection().info('persistence')['rdb_last_bgsave_status'], 'ok')


def _dump_and_reload(env):
    if env.useSlaves:
        # Redis refuses replica SAVE before its first full synchronization.
        env.assertEqual(env.cmd('WAIT', 1, 30000), 1)
    _retry_while_saving(env.dumpAndReload)


def _save_sizes(env):
    """RDB sizes with the stream writer on and off. The stream RDB is left on disk."""
    original = env.getConnection().config_get(WRITER_BUDGET)[WRITER_BUDGET]
    try:
        env.expect('CONFIG', 'SET', WRITER_BUDGET, '0').ok()
        env.assertEqual(_save(env), True)
        inline_size = _rdb_size(env)
    finally:
        env.expect('CONFIG', 'SET', WRITER_BUDGET, original).ok()
    env.assertEqual(_save(env), True)
    return _rdb_size(env), inline_size


def _assert_shrinks(env, stream_size, inline_size, factor):
    """The stream RDB is `factor` times smaller than the inline one. Flex has no stream: its values
    stand alone, and a string that no value repeats saves nothing."""
    message = f'stream RDB {stream_size} vs inline RDB {inline_size}'
    if _is_flex(env):
        env.assertLessEqual(stream_size, inline_size + inline_size // 20, message=message)
    else:
        env.assertLess(stream_size * factor, inline_size, message=message)


def _pipelined(connection, commands, batch=1000):
    replies = []
    for start in range(0, len(commands), batch):
        pipeline = connection.pipeline(transaction=False)
        for command in commands[start:start + batch]:
            pipeline.execute_command(*command)
        replies.extend(pipeline.execute())
    return replies


def _module_log(env, role='master'):
    """What the module logged about RDB streams, or None when the server log is not captured."""
    try:
        path = os.path.join(env.logDir, env.envRunner._getFileName(role, '.log'))
        with open(path, errors='replace') as log:
            return [line.strip() for line in log if LOG_MARK in line]
    except (AttributeError, OSError):
        return None


class _LogWatch:
    """The module's RDB log lines written since the watch started."""

    def __init__(self, env, role='master'):
        self.env = env
        self.role = role
        lines = _module_log(env, role)
        if lines is None:
            # Without the log nothing shows that a stream was written, so the test would check
            # little more than a round trip.
            env.skip()
        self.start = len(lines)

    def lines(self):
        return _module_log(self.env, self.role)[self.start:]


def _field(line, name):
    return int(re.search(rf'\b{name}=(\d+)', line).group(1))


def _log_pid(line):
    """The id of the process that wrote a server log line."""
    match = re.match(r'(\d+):', line)
    return match and match.group(1)


def _last_summary(env, lines, pid=None):
    """The last line that ends a stream, of the process `pid` if given; None after a failure."""
    if _is_flex(env):
        # Flex writes every value self-contained and ends no stream.
        return None
    summaries = [line for line in lines
               if 'phase=persistence_ended' in line and (pid is None or _log_pid(line) == pid)]
    env.assertTrue(summaries, message=f'no stream summary: {lines}')
    return summaries[-1] if summaries else None


def _expect_stream_logs(env, watch, min_refs=1, min_defs=1):
    """A save wrote a stream that defined and referenced strings, the log says."""
    # A save that a replica's synchronization starts can log in the same window. Only the lines of
    # the process that ran the last save count.
    summary = _summary_of_last_save(env, watch)
    if summary is None:
        return
    env.assertGreaterEqual(_field(summary, 'defs'), min_defs)
    env.assertGreaterEqual(_field(summary, 'refs'), min_refs)
    env.assertContains('poisoned=false', summary)


def _rdb_save_len(n):
    """Encode an integer the way Redis' rdbSaveLen does."""
    if n < (1 << 6):
        return bytes([n])
    elif n < (1 << 14):
        return bytes([0x40 | (n >> 8), n & 0xFF])
    elif n <= 0xFFFFFFFF:
        return bytes([0x80]) + struct.pack('>I', n)
    return bytes([0x81]) + struct.pack('>Q', n)


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


def _crc64_jones(data):
    """Redis' CRC-64/Jones (poly 0xad93d23594c935a9, reflected in and out, init 0)."""
    reflected_poly = 0
    for i in range(64):
        if (0xad93d23594c935a9 >> i) & 1:
            reflected_poly |= 1 << (63 - i)
    crc = 0
    for byte in data:
        crc ^= byte
        for _ in range(8):
            crc = (crc >> 1) ^ reflected_poly if crc & 1 else crc >> 1
    return crc & 0xFFFFFFFFFFFFFFFF


# The legacy-RESTORE tests in test.py check the same implementation against this known value.
assert _crc64_jones(b'123456789') == 0xe9c6d914c4b8d9ca


def _restore_payload(genuine_dump, record):
    """A DUMP payload of the module type whose only value record is `record`."""
    assert genuine_dump[0] == RDB_TYPE_MODULE_2, 'expected a module value'
    module_id, _ = _rdb_load_len(genuine_dump, 1)
    body = (_rdb_save_len(RDB_MODULE_OPCODE_STRING) + _rdb_save_len(len(record)) + record +
            _rdb_save_len(RDB_MODULE_OPCODE_EOF))
    payload = (bytes([RDB_TYPE_MODULE_2]) + bytes([0x81]) + struct.pack('>Q', module_id) + body +
               genuine_dump[-10:-8])  # the running server's RDB version
    return payload + struct.pack('<Q', _crc64_jones(payload))


def _rdb_load_string(buf, off):
    """Decode a string saved without compression; returns (bytes, next_offset)."""
    if buf[off] >> 6 == 3:  # an integer stored in 8, 16 or 32 bits
        size = {0: 1, 1: 2, 2: 4}[buf[off] & 0x3F]
        return bytes(buf[off + 1:off + 1 + size]), off + 1 + size
    length, off = _rdb_load_len(buf, off)
    return bytes(buf[off:off + length]), off + length


def _parse_rdb(data):
    """Where the module's value records are in an RDB saved without compression.

    Returns values in file order.
    A value has its `span` and the `record` span of the one string the module saved for it.
    """
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
            length_start = off
            length, record_start = _rdb_load_len(data, off)
            off = record_start + length
            kind, off = _rdb_load_len(data, off)
            assert kind == RDB_MODULE_OPCODE_EOF
            values.append({'span': (start, off), 'length_start': length_start,
                           'record': (record_start, record_start + length)})
        else:
            raise ValueError(f'unexpected RDB opcode {opcode} at offset {off}')
    return values


def _without(data, span):
    return data[:span[0]] + data[span[1]:]


def _with_record(data, value, record):
    return (data[:value['length_start']] + _rdb_save_len(len(record)) + record +
            data[value['record'][1]:])


def _damaged_stream_rdbs(rdb):
    """Malformed streams, each paired with the expected refusal reason."""
    values = _parse_rdb(rdb)
    streamed = [value for value in values if rdb[value['record'][0]] == MARKER_STREAM]
    first, second = streamed[:2]
    first_bytes = rdb[first['span'][0]:first['span'][1]]
    definition = rdb[slice(*first['record'])]
    reference = rdb[slice(*second['record'])]
    assert b'\xc7\x00' in reference, 'expected a reference to the first definition'
    return {
        'first definition dropped': (
            _without(rdb, first['span']),
            r'phase=load status=refused reason=base_id_mismatch expected=0 found=[1-9]'),
        'first definition repeated': (
            rdb[:first['span'][1]] + first_bytes + rdb[first['span'][1]:],
            r'phase=load status=refused reason=base_id_mismatch expected=[1-9]\d* found=0'),
        'first base nonzero': (
            _with_record(rdb, first, definition[:1] + struct.pack('<I', 7) + definition[5:]),
            r'phase=load status=refused reason=base_id_mismatch expected=0 found=7'),
        'invalid reference': (
            _with_record(rdb, second, reference.replace(b'\xc7\x00', b'\xc7\x17', 1)),
            r'phase=load status=refused reason=cbor_InvalidStringReference'),
        'short header': (
            _with_record(rdb, first, bytes([MARKER_STREAM, 0])),
            r'phase=load status=refused reason=cbor_'),
        'invalid CBOR': (
            _with_record(rdb, first, definition[:5] + b'\xff'),
            r'phase=load status=refused reason=cbor_'),
        'unknown marker': (
            _with_record(rdb, first, b'\xff' + definition[1:]),
            r'phase=load status=refused reason=marker_255'),
    }


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


def _load_rdb_in_new_server(env, rdb, inspect=None, binary=None, extra_args=()):
    """Starts a server on a copy of `rdb`, with the module settings of `env`, then stops it.

    Returns how the start went, 'up', 'exited', 'crashed' or 'timeout', and the server log. A
    server whose module refuses part of the file stops, and its exit status says little: after a
    refused record the core runs its RDB checker, which cannot read module data and can find the
    file fine. The module's log line is what says why. `inspect` gets a connection while it is up.
    `binary` replaces the server program of `env`, and `extra_args` follow the other arguments:
    together they start the Flex core without the harness wrapper, which fixes some of its settings.
    """
    directory = tempfile.mkdtemp(prefix='rdbfault')
    socket_path = os.path.join(directory, 'redis.sock')
    log_path = os.path.join(directory, 'redis.log')
    with open(os.path.join(directory, 'dump.rdb'), 'wb') as dump:
        dump.write(rdb)
    runner = env.envRunner
    command = list(runner.masterCmdArgs)
    if binary:
        command[0] = binary
    command += [
        '--port', '0', '--unixsocket', socket_path, '--dir', directory, '--dbfilename', 'dump.rdb',
        '--logfile', log_path, '--loglevel', 'notice', '--rdbchecksum', 'no', '--save', '',
        '--appendonly', 'no', *extra_args]
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


def _shared_text(seed=217194, size=4096):
    rng = random.Random(seed)
    return ''.join(rng.choices(string.ascii_letters + string.digits, k=size))


@_values_in_ram
def test_dictionary_resets_with_tiny_table_round_trip(env):
    env.skipOnCluster()
    env.expect('FLUSHALL').equal(True)
    connection = env.getConnection()
    settings = (WRITER_BUDGET, 'rdbcompression', 'aof-use-rdb-preamble',
                'repl-diskless-sync', 'repl-diskless-sync-delay')
    original = {name: connection.config_get(name)[name] for name in settings}
    documents = {}
    for i in range(120):
        text = hashlib.sha256(f'reset:{i}'.encode()).hexdigest()
        documents[f'reset:{i}'] = {'a': text, 'b': text}
    try:
        connection.config_set(WRITER_BUDGET, 84)
        connection.config_set('rdbcompression', 'no')
        for key, document in documents.items():
            env.expect('JSON.SET', key, '$', json.dumps(document)).ok()
        if env.useAof:
            connection.config_set('aof-use-rdb-preamble', 'yes')
            with TimeLimit(30):
                while connection.info('persistence')['aof_rewrite_in_progress']:
                    time.sleep(0.01)
            connection.bgrewriteaof()
            with TimeLimit(30):
                while connection.info('persistence')['aof_rewrite_in_progress']:
                    time.sleep(0.01)
            env.assertEqual(connection.info('persistence')['aof_last_bgrewrite_status'], 'ok')
            env.expect('DEBUG', 'LOADAOF').ok()
        else:
            for command in ('SAVE', 'BGSAVE'):
                summary = _save_and_read_summary(env, command)
                if summary is not None:
                    env.assertGreater(_field(summary, 'defs'), 3, message=summary)
                if not _is_flex(env):
                    rdb = _rdb_bytes(env)
                    env.assertTrue(any(rdb[value['record'][0]] == MARKER_RESET
                                       for value in _parse_rdb(rdb)))
                env.expect('DEBUG', 'RELOAD', 'NOSAVE').ok()
                for key, document in documents.items():
                    env.assertEqual(json.loads(env.cmd('JSON.GET', key)), document)
            if _is_flex(env):
                # Different compression settings force cold values through the module callbacks.
                connection.config_set('rdbcompression', 'yes')
                for command in ('SAVE', 'BGSAVE'):
                    _put_values_on_flash(env, documents, keep_in_ram=20)
                    summary = _save_and_read_summary(env, command)
                    if summary is not None:
                        env.assertGreater(_field(summary, 'defs'), 3, message=summary)
                    env.expect('DEBUG', 'RELOAD', 'NOSAVE').ok()
                    for key, document in documents.items():
                        env.assertEqual(json.loads(env.cmd('JSON.GET', key)), document)
        for key, document in documents.items():
            env.assertEqual(json.loads(env.cmd('JSON.GET', key)), document)
        if env.useSlaves:
            replica = env.getSlaveConnection()
            load_mode = replica.config_get('repl-diskless-load')['repl-diskless-load']
            try:
                for diskless, loading in (('no', 'disabled'), ('yes', 'on-empty-db')):
                    connection.config_set('repl-diskless-sync', diskless)
                    connection.config_set('repl-diskless-sync-delay', 0)
                    replica.config_set('repl-diskless-load', loading)
                    replica.execute_command('REPLICAOF', 'NO', 'ONE')
                    replica.flushall()
                    env.expect('DEBUG', 'CHANGE-REPL-ID').ok()
                    previous = connection.info('stats')['sync_full']
                    replica.execute_command('REPLICAOF', '127.0.0.1',
                                            connection.info('server')['tcp_port'])
                    with TimeLimit(60):
                        while (connection.info('stats')['sync_full'] <= previous or
                               replica.info('replication')['master_link_status'] != 'up'):
                            time.sleep(0.01)
                    for key, document in documents.items():
                        env.assertEqual(json.loads(replica.execute_command('JSON.GET', key)), document)
            finally:
                replica.config_set('repl-diskless-load', load_mode)
    finally:
        for name, value in original.items():
            connection.config_set(name, value)


def test_dictionary_reset_boundaries_are_checked_when_loading(env):
    env.skipOnCluster()
    if (env.useAof or _is_flex(env) or VALGRIND or env.useTLS or
            not hasattr(env.envRunner, 'masterCmdArgs')):
        env.skip()
    connection = env.getConnection()
    original = {name: connection.config_get(name)[name]
                for name in (WRITER_BUDGET, 'rdbcompression')}
    env.expect('FLUSHALL').equal(True)
    try:
        connection.config_set(WRITER_BUDGET, 84)
        connection.config_set('rdbcompression', 'no')
        for i in range(16):
            text = f'string that is repeated within key {i:04}'
            env.expect('JSON.SET', f'reset:{i}', '$', json.dumps([text, text])).ok()
        env.assertEqual(_save(env), True)
        rdb = _rdb_bytes(env)
    finally:
        for name, value in original.items():
            connection.config_set(name, value)
    outcome, lines = _load_rdb_in_new_server(env, rdb)
    env.assertEqual(outcome, 'up', message=str(lines))
    boundary = next(value for value in _parse_rdb(rdb)
                    if rdb[value['record'][0]] == MARKER_RESET)
    start, end = boundary['span']
    record = rdb[slice(*boundary['record'])]
    cases = [
        (_without(rdb, boundary['span']), 'base_id_mismatch'),
        (rdb[:end] + rdb[start:end] + rdb[end:], 'base_id_mismatch'),
        (_with_record(rdb, boundary, record[:5] + b'\xc7\x00'), 'cbor_InvalidStringReference'),
        (_with_record(rdb, boundary, record[:5] + b'\xf6'), 'cbor_InvalidValue'),
    ]
    for damaged, reason in cases:
        outcome, lines = _load_rdb_in_new_server(env, damaged)
        env.assertEqual(outcome, 'exited', message=str(lines))
        env.assertTrue(any(f'reason={reason}' in line for line in lines), message=str(lines))


@_values_in_ram
def test_dictionary_writer_budget_does_not_limit_reads(env):
    env.skipOnCluster()
    if env.useAof:
        env.skip()
    env.expect('FLUSHALL').equal(True)
    config = WRITER_BUDGET
    original = env.getConnection().config_get(config)[config]
    shared = _shared_text()
    documents = {f'budget:{i}': {'shared': shared, 'step': i} for i in range(32)}
    try:
        env.expect('CONFIG', 'SET', config, '2gb').ok()
        env.assertEqual(env.getConnection().config_get(config)[config], str(2 * 1024 * 1024 * 1024))
        for invalid in ('-1', str(1024 ** 4 + 1)):
            env.expect('CONFIG', 'SET', config, invalid).raiseError()
        for key, document in documents.items():
            env.expect('JSON.SET', key, '$', json.dumps(document)).ok()
        env.expect('CONFIG', 'SET', config, '0').ok()
        env.assertEqual(_save(env), True)
        inline_size = _rdb_size(env)

        # The address table of one shared string fits in a megabyte in either
        # save mode.
        env.expect('CONFIG', 'SET', config, '1mb').ok()
        env.assertEqual(_save(env), True)
        _assert_shrinks(env, _rdb_size(env), inline_size, 4)

        # Too small a budget defines nothing: every record stays self-contained,
        # and the reader loads it all the same.
        watch = _LogWatch(env)
        env.expect('CONFIG', 'SET', config, '1').ok()
        env.assertEqual(_save(env), True)
        env.assertLessEqual(_rdb_size(env), inline_size + inline_size // 20)
        env.assertGreater(_rdb_size(env) * 2, inline_size)
        summary = _last_summary(env, watch.lines())
        if summary is not None:
            env.assertEqual(_field(summary, 'defs'), 0, message=summary)
            env.assertEqual(_field(summary, 'refs'), 0, message=summary)
            env.assertEqual(_field(summary, 'inline_docs'), len(documents), message=summary)

        env.expect('CONFIG', 'SET', config, '0').ok()
        env.expect('DEBUG', 'RELOAD', 'NOSAVE').ok()
        for key, document in documents.items():
            env.assertEqual(json.loads(env.cmd('JSON.GET', key)), document)
    finally:
        env.cmd('CONFIG', 'SET', config, original)


@_values_in_ram
def test_rdb_stream_shrinks_repeated_strings_and_survives_repeated_reloads(env):
    env.skipOnCluster()
    if env.useAof:
        env.skip()
    env.expect('FLUSHALL').equal(True)
    shared = _shared_text()
    documents = {f'budget:{i}': {'shared': shared, 'step': i} for i in range(32)}
    # Documents that share no string stay self-contained inside the stream.
    documents['unshared:text'] = {'only': 'a string that no other document holds ' * 8}
    documents['unshared:numbers'] = [1, 2.5, None, True]
    for key, document in documents.items():
        env.expect('JSON.SET', key, '$', json.dumps(document)).ok()

    watch = _LogWatch(env)
    stream_size, inline_size = _save_sizes(env)
    _assert_shrinks(env, stream_size, inline_size, 8)
    # One definition and 31 references; SAVE runs in the server process with an address table.
    _expect_stream_logs(env, watch, min_refs=31)

    for _ in range(2):
        _dump_and_reload(env)
        for key, document in documents.items():
            env.assertEqual(json.loads(env.cmd('JSON.GET', key)), document)


@_values_in_ram
def test_strings_of_eight_bytes_or_more_are_shared_object_keys_included(env):
    env.skipOnCluster()
    if env.useAof:
        env.skip()
    env.expect('FLUSHALL').equal(True)
    # An 18 byte key, a 20 byte value and a value of exactly 8 bytes are shared. A value of 7 bytes
    # sits inside its handle and stays inline, and so do the short keys.
    count = 32
    document = {'checkpoint_content': 'v' * 20, 'eight': 'abcdefgh', 'seven': 'abcdefg'}
    for i in range(count):
        env.expect('JSON.SET', f'short:{i}', '$', json.dumps(dict(document, n=i))).ok()
    summary = _save_and_read_summary(env, 'SAVE')
    if summary is not None:
        env.assertEqual(_field(summary, 'defs'), 3, message=summary)
        env.assertEqual(_field(summary, 'refs'), 3 * (count - 1), message=summary)
        env.assertEqual(_field(summary, 'inline_docs'), 0, message=summary)
    env.expect('DEBUG', 'RELOAD', 'NOSAVE').ok()
    for i in range(count):
        env.assertEqual(json.loads(env.cmd('JSON.GET', f'short:{i}')), dict(document, n=i))


@no_san
@_values_in_ram
def test_stream_covers_more_distinct_strings_than_a_65536_entry_table_holds(env):
    env.skipOnCluster()
    if env.useAof or VALGRIND:
        env.skip()
    env.expect('FLUSHALL').equal(True)
    # A table of 65,536 entries would not hold them all. Each of these is 128 bytes and
    # shared by four documents, whose index decides which strings they hold.
    distinct = 70_000
    strings = [hashlib.sha256(f'{i}:a'.encode()).hexdigest() +
               hashlib.sha256(f'{i}:b'.encode()).hexdigest() for i in range(distinct)]

    def document(index):
        return {name: strings[(index + offset) % distinct] for offset, name in enumerate('abcd')}

    connection = env.getConnection()
    commands = [('JSON.SET', f'many:{i}', '$', json.dumps(document(i), separators=(',', ':')))
                for i in range(distinct)]
    try:
        env.assertEqual(set(_pipelined(connection, commands)), {'OK'})
        watch = _LogWatch(env)
        stream_size, inline_size = _save_sizes(env)
        _assert_shrinks(env, stream_size, inline_size, 2)
        # A table that stopped at 65,536 strings would still halve the RDB, so the log counts
        # them: every string is defined once and referenced by the three other documents.
        _expect_stream_logs(env, watch, min_refs=3 * distinct, min_defs=distinct)

        for save in ('RELOAD', 'BGSAVE'):
            if save == 'BGSAVE':
                watch = _LogWatch(env)
                _save(env, 'BGSAVE')
                _wait_for_bgsave(env)
                _assert_shrinks(env, _rdb_size(env), inline_size, 2)
                _expect_stream_logs(env, watch, min_refs=3 * distinct, min_defs=distinct)
            env.expect('DEBUG', 'RELOAD', 'NOSAVE').ok()
            replies = _pipelined(connection, [('JSON.GET', f'many:{i}') for i in range(distinct)])
            matching = sum(json.loads(reply) == document(i) for i, reply in enumerate(replies))
            env.assertEqual(matching, distinct)
    finally:
        env.expect('FLUSHALL').equal(True)


@_values_in_ram
def test_writer_off_saves_self_contained_records_and_still_loads_streams(env):
    env.skipOnCluster()
    if env.useAof:
        env.skip()
    env.expect('FLUSHALL').equal(True)
    shared = _shared_text(seed=1, size=2048)
    documents = {f'off:{i}': {'shared': shared, 'step': i} for i in range(24)}
    for key, document in documents.items():
        env.expect('JSON.SET', key, '$', json.dumps(document)).ok()
    original = env.getConnection().config_get(WRITER_BUDGET)[WRITER_BUDGET]
    try:
        env.assertEqual(_save(env), True)
        stream_size = _rdb_size(env)

        # The reader needs nothing from the writer setting.
        env.expect('CONFIG', 'SET', WRITER_BUDGET, '0').ok()
        env.expect('DEBUG', 'RELOAD', 'NOSAVE').ok()
        for key, document in documents.items():
            env.assertEqual(json.loads(env.cmd('JSON.GET', key)), document)

        # With the writer off a save has no module AUX and every record stands alone.
        watch = _LogWatch(env)
        env.assertEqual(_save(env), True)
        if not _is_flex(env):
            env.assertGreater(_rdb_size(env), stream_size * 4)
        lines = watch.lines()
        env.assertTrue(any('phase=persistence_start mode=plain reason=off' in line and 'budget_bytes=0' in line for line in lines),
                       message=str(lines))
    finally:
        env.expect('CONFIG', 'SET', WRITER_BUDGET, original).ok()


@_values_in_ram
def test_debug_reload_saves_and_loads_a_stream(env):
    env.skipOnCluster()
    if env.useAof:
        env.skip()
    env.expect('FLUSHALL').equal(True)
    shared = _shared_text(seed=2, size=1024)
    documents = {f'reload:{i}': {'shared': shared, 'step': i} for i in range(16)}
    for key, document in documents.items():
        env.expect('JSON.SET', key, '$', json.dumps(document)).ok()
    watch = _LogWatch(env)
    # DEBUG RELOAD saves inside the server process and loads the result.
    for _ in range(2):
        # Finish replica synchronization and reap its child before requiring a sync stream.
        if env.useSlaves:
            env.assertEqual(env.cmd('WAIT', 1, 30000), 1)
        _wait_for_bgsave(env)
        env.expect('DEBUG', 'RELOAD').ok()
        for key, document in documents.items():
            env.assertEqual(json.loads(env.cmd('JSON.GET', key)), document)
    _expect_stream_logs(env, watch, min_refs=15)


@_values_in_ram
def test_dump_restore_are_self_contained_before_during_and_after_a_save(env):
    env.skipOnCluster()
    if env.useAof:
        env.skip()
    env.expect('FLUSHALL').equal(True)
    shared = _shared_text(seed=3)
    document = {'shared': shared}
    for i in range(24):
        env.expect('JSON.SET', f'dump:{i}', '$', json.dumps(document)).ok()
    connection = _binary_connection(env)
    delay = env.getConnection().config_get('rdb-key-save-delay').get('rdb-key-save-delay')
    try:
        payloads = {'before': connection.execute_command('DUMP', 'dump:0')}
        # The child needs about half a second, long enough for a DUMP in the parent.
        env.cmd('CONFIG', 'SET', 'rdb-key-save-delay', 20000)
        _save(env, 'BGSAVE')
        payloads['during'] = connection.execute_command('DUMP', 'dump:1')
        _wait_for_bgsave(env)
        env.cmd('CONFIG', 'SET', 'rdb-key-save-delay', delay or 0)
        env.assertEqual(_save(env), True)
        payloads['after'] = connection.execute_command('DUMP', 'dump:2')

        env.expect('FLUSHALL').equal(True)
        for name, payload in payloads.items():
            # A stream record would be a few dozen bytes; a self-contained one holds the string.
            env.assertGreater(len(payload), len(shared) // 2, message=name)
            connection.execute_command('RESTORE', f'restored:{name}', 0, payload)
            env.assertEqual(json.loads(env.cmd('JSON.GET', f'restored:{name}')), document)
    finally:
        env.cmd('CONFIG', 'SET', 'rdb-key-save-delay', delay or 0)
        connection.close()


@_values_in_ram
def test_restore_refuses_a_stream_record(env):
    env.skipOnCluster()
    if env.useAof:
        env.skip()
    env.expect('JSON.SET', 'genuine', '$', '"value"').ok()
    connection = _binary_connection(env)
    text = b'0123456789abcdefghijklmnopqrstuvwxyzABCD'
    plain = bytes([0xA1, 0x61, 0x6B, 0x61, 0x76])  # {"k": "v"}
    defining = bytes([0xA1, 0x61, 0x6B, 0xC6, 0x78, len(text)]) + text
    referencing = bytes([0xA1, 0x61, 0x6B, 0xC7, 0x00])
    try:
        genuine = connection.execute_command('DUMP', 'genuine')
        # The crafted payload is well formed: a self-contained record restores.
        connection.execute_command('RESTORE', 'crafted', 0, _restore_payload(genuine, b'\x00' + plain))
        env.assertEqual(json.loads(env.cmd('JSON.GET', 'crafted')), {'k': 'v'})
        # A stream record, marker 1 and a base id, needs a load that announced a stream.
        for name, document in (('defining', defining), ('referencing', referencing)):
            record = bytes([MARKER_STREAM]) + struct.pack('<I', 0) + document
            payload = _restore_payload(genuine, record)
            query = env.expect('RESTORE', f'refused:{name}', 0, payload)
            env.assertTrue(query.errorRaised, message=f'{name}: {query.res!r}')
            env.assertEqual(env.cmd('EXISTS', f'refused:{name}'), 0, message=name)
        # A self-contained record, marker 3, restores as well.
        connection.execute_command('RESTORE', 'standalone', 0,
                                   _restore_payload(genuine, bytes([3]) + plain))
        env.assertEqual(json.loads(env.cmd('JSON.GET', 'standalone')), {'k': 'v'})
        # The reserved markers are refused as well.
        for marker in (2, 4):
            payload = _restore_payload(genuine, bytes([marker]) + plain)
            query = env.expect('RESTORE', f'refused:marker{marker}', 0, payload)
            env.assertTrue(query.errorRaised, message=f'marker {marker}: {query.res!r}')
        env.assertEqual(env.cmd('PING'), True)
        env.assertEqual(json.loads(env.cmd('JSON.GET', 'genuine')), 'value')
    finally:
        connection.close()


def test_a_damaged_stream_rdb_is_refused_when_it_loads(env):
    env.skipOnCluster()
    if (env.useAof or _is_flex(env) or VALGRIND or env.useTLS or
            not hasattr(env.envRunner, 'masterCmdArgs')):
        env.skip()
    env.expect('FLUSHALL').equal(True)
    shared = _shared_text(seed=6, size=2048)
    documents = {f'damaged:{i}': {'shared': shared, 'step': i} for i in range(6)}
    for key, document in documents.items():
        env.expect('JSON.SET', key, '$', json.dumps(document)).ok()
    connection = env.getConnection()
    original = connection.config_get('rdbcompression')['rdbcompression']
    try:
        # Without compression the records are plain bytes. The checksum cannot be switched off at
        # run time, but the server that loads an edited file does not verify it (`--rdbchecksum no`
        # in `_load_rdb_in_new_server`), so the file still loads as far as the core is concerned.
        connection.config_set('rdbcompression', 'no')
        env.assertEqual(_save(env), True)
        rdb = _rdb_bytes(env)
    finally:
        connection.config_set('rdbcompression', original)

    def read_documents(connection):
        for key, document in documents.items():
            env.assertEqual(json.loads(connection.execute_command('JSON.GET', key)), document)

    # The file as saved is the control: if it did not load, a refusal below would show nothing.
    outcome, lines = _load_rdb_in_new_server(env, rdb, read_documents)
    env.assertEqual(outcome, 'up', message=str(lines))
    env.assertTrue(any('phase=loading_ended' in line for line in lines),
                   message=str(lines))

    # A server that is refused part of its file stops, and its exit status says little, so the
    # module's log line is what shows that the refusal happened and why.
    for name, (damaged, reason) in _damaged_stream_rdbs(rdb).items():
        outcome, lines = _load_rdb_in_new_server(env, damaged)
        env.assertEqual(outcome, 'exited', message=f'{name}: {lines}')
        env.assertTrue(any(re.search(reason, line) for line in lines),
                       message=f'{name}: expected {reason}, got {lines}')


@_values_in_ram
def test_bgsave_under_concurrent_updates_reloads_the_fork_time_snapshot(env):
    env.skipOnCluster()
    if env.useAof:
        env.skip()
    env.expect('FLUSHALL').equal(True)
    pool = [f'pool string {i:02d} ' + hashlib.sha256(str(i).encode()).hexdigest() for i in range(24)]
    snapshot = {f'snapshot:{i}': {'a': pool[i % 24], 'b': pool[(i + 5) % 24], 'n': i}
                for i in range(300)}
    for key, document in snapshot.items():
        env.expect('JSON.SET', key, '$', json.dumps(document)).ok()
    delay = env.getConnection().config_get('rdb-key-save-delay').get('rdb-key-save-delay')
    watch = _LogWatch(env)
    try:
        # Slow the child down so that the parent changes keys while it saves.
        env.cmd('CONFIG', 'SET', 'rdb-key-save-delay', 1000)
        _save(env, 'BGSAVE')
        for i in range(0, 300, 3):
            env.expect('JSON.SET', f'snapshot:{i}', '$.a', json.dumps(f'replacement {i} ' + 'z' * 100)).ok()
        for i in range(1, 300, 7):
            env.expect('DEL', f'snapshot:{i}').equal(1)
        env.expect('JSON.SET', 'created-after-fork', '$', json.dumps({'a': pool[0]})).ok()
        env.cmd('CONFIG', 'SET', 'rdb-key-save-delay', delay or 0)
        _wait_for_bgsave(env)
    finally:
        env.cmd('CONFIG', 'SET', 'rdb-key-save-delay', delay or 0)
    _expect_stream_logs(env, watch)

    env.expect('DEBUG', 'RELOAD', 'NOSAVE').ok()
    for key, document in snapshot.items():
        env.assertEqual(json.loads(env.cmd('JSON.GET', key)), document, message=key)
    env.expect('EXISTS', 'created-after-fork').equal(0)


@_values_in_ram
def test_expired_key_holding_the_only_definition_still_resolves_later_references(env):
    env.skipOnCluster()
    if env.useAof:
        env.skip()
    env.expect('FLUSHALL').equal(True)
    pairs = 40
    texts = [f'text defined by the first key saved {i:03d} ' + hashlib.sha256(str(i).encode()).hexdigest()
             for i in range(pairs)]
    for i, text in enumerate(texts):
        env.expect('JSON.SET', f'expiring:{i}', '$', json.dumps({'text': text})).ok()
        env.expect('JSON.SET', f'durable:{i}', '$', json.dumps({'text': text, 'id': i})).ok()
    # Which key of a pair is saved first depends on the hash order. Over so many pairs the
    # expired key is first for about half, and an expired key is still written and loaded.
    env.expect('DEBUG', 'SET-ACTIVE-EXPIRE', 0).ok()
    try:
        for i in range(pairs):
            env.expect('PEXPIRE', f'expiring:{i}', 100).equal(1)
        time.sleep(0.3)
        env.assertEqual(_save(env), True)
        env.expect('DEBUG', 'RELOAD', 'NOSAVE').ok()
    finally:
        env.cmd('DEBUG', 'SET-ACTIVE-EXPIRE', 1)
    for i, text in enumerate(texts):
        env.expect('EXISTS', f'expiring:{i}').equal(0)
        env.assertEqual(json.loads(env.cmd('JSON.GET', f'durable:{i}')), {'text': text, 'id': i})


def test_rdb_dictionary_round_trip_across_databases(env):
    env.skipOnCluster()
    # Flex has a single database.
    if env.useAof or _is_flex(env):
        env.skip()
    env.expect('FLUSHALL').equal(True)
    document = {'shared_checkpoint_field': _shared_text(seed=4, size=2048)}
    connection = env.getConnection()
    try:
        for db in (0, 1, 2):
            connection.execute_command('SELECT', db)
            for key in ('checkpoint', 'second'):
                connection.execute_command('JSON.SET', key, '$', json.dumps(document))
        connection.execute_command('SELECT', 0)
        # One stream spans every database: the later ones only reference what the first defined, the
        # key and the string.
        stream_size, inline_size = _save_sizes(env)
        _assert_shrinks(env, stream_size, inline_size, 4)
        _dump_and_reload(env)
        for db in (0, 1, 2):
            connection.execute_command('SELECT', db)
            for key in ('checkpoint', 'second'):
                env.assertEqual(json.loads(connection.execute_command('JSON.GET', key)), document)
    finally:
        for db in (1, 2):
            connection.execute_command('SELECT', db)
            connection.execute_command('FLUSHDB')
        connection.execute_command('SELECT', 0)


# The two Flex tests below keep values in RAM at first and then spill them to flash while an RDB
# is loaded, and write the cold values out again.
def test_flex_inline_rdb_load_then_spill_and_cold_rdb_round_trip(env):
    env.skipOnCluster()
    if env.useAof or not _is_flex(env):
        env.skip()
    connection = env.getConnection()
    original = {name: connection.config_get(name)[name] for name in
                ('bigredis-max-ram-keys', 'bigredis-load-to-disk-threshold')}
    binary = _binary_connection(env)
    shared = _shared_text()
    documents = [{'checkpoint_content': shared, 'step': i} for i in range(32)]
    try:
        connection.config_set('bigredis-max-ram-keys', '10000')
        connection.config_set('bigredis-load-to-disk-threshold', '100')
        for i, document in enumerate(documents):
            env.expect('JSON.SET', f'checkpoint:{i}', '$', json.dumps(document)).ok()
        payload = binary.execute_command('DUMP', 'checkpoint:0')
        binary.execute_command('RESTORE', 'before-save', 0, payload)
        env.expect('DEL', 'before-save').equal(1)
        env.assertEqual(_save(env), True)
        env.expect('DEBUG', 'RELOAD', 'NOSAVE').ok()
        for i, document in enumerate(documents):
            env.assertEqual(json.loads(env.cmd('JSON.GET', f'checkpoint:{i}')), document)

        payload = binary.execute_command('DUMP', 'checkpoint:0')
        binary.execute_command('RESTORE', 'standalone', 0, payload)
        env.assertEqual(json.loads(env.cmd('JSON.GET', 'standalone')), documents[0])

        env.cmd('DEBUG', 'BIGREDIS-SWAPOUT')
        env.assertContains('data-in-ram:no', env.cmd('DEBUG', 'BIGREDIS-KEY', 'checkpoint:0'))
        # Cold values take Flex's serialized-byte export path and stay portable.
        env.assertEqual(_save(env), True)
        env.expect('DEBUG', 'RELOAD', 'NOSAVE').ok()
        for i, document in enumerate(documents):
            env.assertEqual(json.loads(env.cmd('JSON.GET', f'checkpoint:{i}')), document)
        env.assertEqual(json.loads(env.cmd('JSON.GET', 'standalone')), documents[0])
    finally:
        binary.close()
        for name, value in original.items():
            connection.config_set(name, value)


def test_flex_inline_rdb_load_spills_under_a_tiny_ram_limit(env):
    env.skipOnCluster()
    if env.useAof or not _is_flex(env):
        env.skip()
    connection = env.getConnection()
    original = connection.config_get('bigredis-max-ram-keys')['bigredis-max-ram-keys']
    document = {'checkpoint_content': 'shared contents survive spill during loading ' * 256}
    try:
        connection.config_set('bigredis-max-ram-keys', '10000')
        for i in range(64):
            env.expect('JSON.SET', f'checkpoint:{i}', '$', json.dumps(document)).ok()
        env.assertEqual(_save(env), True)
        connection.config_set('bigredis-max-ram-keys', '2')
        env.expect('DEBUG', 'RELOAD', 'NOSAVE').ok()
        cold = sum('data-in-ram:no' in env.cmd('DEBUG', 'BIGREDIS-KEY', f'checkpoint:{i}')
                   for i in range(64))
        env.assertGreater(cold, 0, message='Load must spill with a two-key RAM limit')
        for i in range(64):
            env.assertEqual(json.loads(env.cmd('JSON.GET', f'checkpoint:{i}')), document)
    finally:
        connection.config_set('bigredis-max-ram-keys', original)


def _wait_until_on_flash(env, keys):
    """Waits until Flex has written every one of `keys` to flash and freed its value."""
    connection = env.getConnection()
    with TimeLimit(60):
        while any('data-in-ram:yes' in connection.execute_command('DEBUG', 'BIGREDIS-KEY', key)
                  for key in keys):
            time.sleep(0.05)


def _put_values_on_flash(env, keys, keep_in_ram=0):
    """Moves every value to flash, then reads `keep_in_ram` of them back into RAM."""
    env.cmd('DEBUG', 'BIGREDIS-SWAPOUT')
    _wait_until_on_flash(env, keys)
    for key in list(keys)[:keep_in_ram]:
        env.cmd('JSON.GET', key)


def _summary_of_last_save(env, watch):
    """The summary of the last stream that a save wrote since the watch
    began, or None. A save that a replica's synchronization starts can log in the same window."""
    lines = watch.lines()
    started = [line for line in lines
               if 'phase=persistence_start' in line]
    env.assertTrue(started, message=f'no stream started: {lines}')
    if _is_flex(env):
        env.assertContains('mode=local reason=flex', started[-1])
    return _last_summary(env, lines, _log_pid(started[-1])) if started else None


def _save_and_read_summary(env, command):
    watch = _LogWatch(env)
    env.assertEqual(_save(env, command), True)
    if command == 'BGSAVE':
        _wait_for_bgsave(env)
    return _summary_of_last_save(env, watch)


@_values_in_ram
def test_shared_dictionary_in_aof_rdb_preamble(env):
    env.skipOnCluster()
    if not env.useAof:
        env.skip()
    connection = env.getConnection()
    original = connection.config_get('aof-use-rdb-preamble')['aof-use-rdb-preamble']
    document = {'checkpoint_content': 'shared AOF preamble contents ' * 256}
    tail = {'checkpoint_content': document['checkpoint_content'], 'written': 'after the rewrite'}
    try:
        connection.config_set('aof-use-rdb-preamble', 'yes')
        for i in range(32):
            env.expect('JSON.SET', f'checkpoint:{i}', '$', json.dumps(document)).ok()
        with TimeLimit(30):
            while connection.info('persistence')['aof_rewrite_in_progress']:
                time.sleep(0.01)
        watch = _LogWatch(env)
        connection.bgrewriteaof()
        with TimeLimit(30):
            while connection.info('persistence')['aof_rewrite_in_progress']:
                time.sleep(0.01)
        env.assertEqual(connection.info('persistence')['aof_last_bgrewrite_status'], 'ok')
        # The rewrite child wrote the preamble as a stream: the key and the string are each defined
        # once and referenced by the other 31 documents.
        _expect_stream_logs(env, watch, min_refs=31)
        # Commands after the rewrite are the tail of the AOF; they share strings with the base.
        for i in range(8):
            env.expect('JSON.SET', f'tail:{i}', '$', json.dumps(tail)).ok()
        env.expect('JSON.SET', 'checkpoint:0', '$.written', '"updated in the tail"').ok()
        env.expect('DEBUG', 'LOADAOF').ok()
        for i in range(1, 32):
            env.assertEqual(json.loads(env.cmd('JSON.GET', f'checkpoint:{i}')), document)
        env.assertEqual(json.loads(env.cmd('JSON.GET', 'checkpoint:0')),
                        dict(document, written='updated in the tail'))
        for i in range(8):
            env.assertEqual(json.loads(env.cmd('JSON.GET', f'tail:{i}')), tail)
    finally:
        connection.config_set('aof-use-rdb-preamble', original)


def _full_sync(env, master_diskless, replica_load, on_flash=False):
    master = env.getConnection()
    replica = env.getSlaveConnection()
    port = master.info('server')['tcp_port']
    document = {'checkpoint_content': f'shared {master_diskless} {replica_load} contents ' * 256}
    original = {
        'master': {name: master.config_get(name)[name]
                   for name in ('repl-diskless-sync', 'repl-diskless-sync-delay')},
        'replica': replica.config_get('repl-diskless-load')['repl-diskless-load'],
    }
    master_watch = _LogWatch(env, 'master')
    replica_watch = _LogWatch(env, 'slave')
    try:
        master.config_set('repl-diskless-sync', master_diskless)
        master.config_set('repl-diskless-sync-delay', '0')
        replica.config_set('repl-diskless-load', replica_load)
        replica.execute_command('REPLICAOF', 'NO', 'ONE')
        replica.flushall()
        for i in range(32):
            env.expect('JSON.SET', f'checkpoint:{i}', '$', json.dumps(document)).ok()
        if on_flash:
            # Four values stay in RAM and define the string; the others are on flash.
            _put_values_on_flash(env, [f'checkpoint:{i}' for i in range(32)], keep_in_ram=4)
        # Invalidate both replication IDs so reconnect must transfer a full RDB.
        env.expect('DEBUG', 'CHANGE-REPL-ID').ok()
        full_syncs = master.info('stats')['sync_full']
        replica.execute_command('REPLICAOF', '127.0.0.1', port)
        with TimeLimit(60):
            while (master.info('stats')['sync_full'] <= full_syncs or
                   replica.info('replication')['master_link_status'] != 'up'):
                time.sleep(0.01)
        for i in range(32):
            env.assertEqual(json.loads(replica.execute_command('JSON.GET', f'checkpoint:{i}')),
                            document)
        # The master saved a stream in a child, and the replica completed its load. With the values
        # on flash only the four in RAM are certain to reference the definitions (the key and the
        # string): Flex copies the bytes of the others for a replica that loads straight from the
        # socket.
        _expect_stream_logs(env, master_watch, min_refs=3 if on_flash else 31)
        lines = replica_watch.lines()
        # Flex arms no load session, so it logs no end of a stream load.
        if not _is_flex(env):
            env.assertTrue(any('phase=loading_ended' in line for line in lines),
                           message=str(lines))
    finally:
        master.config_set('repl-diskless-sync', original['master']['repl-diskless-sync'])
        master.config_set('repl-diskless-sync-delay', original['master']['repl-diskless-sync-delay'])
        replica.config_set('repl-diskless-load', original['replica'])
        replica.execute_command('REPLICAOF', '127.0.0.1', port)


@_values_in_ram
def test_shared_dictionary_full_replication_load(env):
    env.skipOnCluster()
    if not env.useSlaves:
        env.skip()
    # The master writes the RDB to disk or straight to the socket, and the replica loads it from
    # a file or straight from the socket. The module does not declare async replication loading,
    # so the core loads a 'swapdb' replica from a file as well ("Skipping diskless-load"); that
    # variant covers a disk load under that setting, not a load that serves the old data.
    variants = [('no', 'disabled'), ('yes', 'disabled'), ('yes', 'on-empty-db'), ('yes', 'swapdb')]
    if _is_flex(env):
        # Flex refuses repl-diskless-load swapdb.
        variants = variants[:-1]
    for master_diskless, replica_load in variants:
        _full_sync(env, master_diskless, replica_load)


@_values_in_ram
def test_failed_save_releases_dictionary_and_next_save_restarts(env):
    env.skipOnCluster()
    if env.useAof:
        env.skip()
    env.expect('FLUSHALL').equal(True)
    document = {'shared': _shared_text(seed=71, size=2048)}
    for i in range(12):
        env.expect('JSON.SET', f'retry:{i}', '$', json.dumps(document)).ok()
    connection = env.getConnection()
    original = connection.config_get('dbfilename')['dbfilename']
    directory = connection.config_get('dir')['dir']
    _wait_for_bgsave(env)
    with tempfile.TemporaryDirectory(dir=directory) as target:
        try:
            connection.config_set('dbfilename', os.path.basename(target))
            watch = _LogWatch(env)
            env.expect('SAVE').raiseError()
            lines = watch.lines()
            if not _is_flex(env):
                env.assertTrue(any('phase=persistence_failed' in line for line in lines),
                               message=str(lines))
        finally:
            connection.config_set('dbfilename', original)
    summary = _save_and_read_summary(env, 'SAVE')
    if summary is not None:
        env.assertGreater(_field(summary, 'defs'), 0, message=summary)
        env.assertGreater(_field(summary, 'refs'), 0, message=summary)
    env.expect('DEBUG', 'RELOAD', 'NOSAVE').ok()
    for i in range(12):
        env.assertEqual(json.loads(env.cmd('JSON.GET', f'retry:{i}')), document)


@_values_in_ram
def test_dump_writes_a_string_that_repeats_inside_one_value_once(env):
    env.skipOnCluster()
    if env.useAof:
        env.skip()
    env.expect('FLUSHALL').equal(True)
    key_name = 'a repeated object key that is long enough to be shared'
    document = {'items': [{key_name: i} for i in range(200)]}
    env.expect('JSON.SET', 'doc', '$', json.dumps(document)).ok()
    connection = _binary_connection(env)
    config = env.getConnection()
    compression = config.config_get('rdbcompression')['rdbcompression']
    budget = config.config_get(WRITER_BUDGET)[WRITER_BUDGET]
    try:
        # LZF would shrink the repeated keys of the payload by itself.
        env.expect('CONFIG', 'SET', 'rdbcompression', 'no').ok()
        env.expect('CONFIG', 'SET', WRITER_BUDGET, '0').ok()
        written_in_full = connection.execute_command('DUMP', 'doc')
        env.expect('CONFIG', 'SET', WRITER_BUDGET, budget).ok()
        shared = connection.execute_command('DUMP', 'doc')
        if not _is_flex(env):
            # Flex compresses the value it keeps, which hides the repeats from this comparison.
            env.assertLess(len(shared), len(written_in_full) // 2)
        # The payload stands alone: a server that holds no other key reads it back.
        env.expect('FLUSHALL').equal(True)
        connection.execute_command('RESTORE', 'restored', 0, shared)
        env.assertEqual(json.loads(env.cmd('JSON.GET', 'restored')), document)
    finally:
        env.cmd('CONFIG', 'SET', WRITER_BUDGET, budget)
        env.cmd('CONFIG', 'SET', 'rdbcompression', compression)
        connection.close()
