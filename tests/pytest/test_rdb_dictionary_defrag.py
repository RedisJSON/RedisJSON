"""RDB encoding v5 with active defrag, and an AOF stream under the memory checkers (MOD-20882).

While Redis loads, it defragments only during an AOF load. A stream load holds the strings it
interned, so the module keeps its string cache while one runs; a defrag cycle after it moves each
string into one new copy. Either way, the next save must still write each shared string once.

Active defrag needs Redis built with its jemalloc. The macOS, sanitizer and valgrind builds use
libc malloc, so the defrag tests skip there. The AOF test needs no defrag, and runs in the plain
lane, the only one that the sanitizer job runs; `test_rdb_dictionary.py` checks the AOF preamble
only in the `--use-aof` lane.
"""

import json
import os
import random
import re
import string
import tempfile
import threading
import time

from redis.exceptions import ResponseError
from RLTest import Env

from common import TimeLimit


WRITER_BUDGET = 'json-rdb-dictionary-max-memory-percent'
LOG_MARK = 'JSON RDB dictionary'
KEYS = 1000
# Every document holds two long string values under two object keys of 8 bytes or more.
SHARED_STRINGS = 4


def _plain_env():
    env = Env(decodeResponses=True)
    # Other lanes run the AOF and replication paths in test_rdb_dictionary.py.
    if env.isCluster() or env.useSlaves or env.useAof or env.env.endswith('existing-env'):
        env.skip()
    return env


def _shared_text(seed, size):
    rng = random.Random(seed)
    return ''.join(rng.choices(string.ascii_letters + string.digits, k=size))


def _documents(prefix='checkpoint', count=KEYS):
    """Documents that repeat two long strings, under object keys long enough to be shared too."""
    first, second = _shared_text(seed=20882, size=2048), _shared_text(seed=3, size=512)
    return {f'{prefix}:{i}': {'shared_checkpoint_text': first, 'second_shared_field': second, 'n': i}
            for i in range(count)}


def _set_documents(env, connection, documents):
    pipe = connection.pipeline(transaction=False)
    for key, document in documents.items():
        pipe.execute_command('JSON.SET', key, '$', json.dumps(document))
    env.assertEqual(set(pipe.execute()), {'OK'})


def _assert_documents(env, connection, documents):
    """The server holds exactly `documents`, as they were written."""
    env.assertEqual(connection.dbsize(), len(documents))
    pipe = connection.pipeline(transaction=False)
    for key in documents:
        pipe.execute_command('JSON.GET', key)
    for key, reply in zip(documents, pipe.execute()):
        env.assertEqual(json.loads(reply), documents[key], message=key)


class _ServerLog:
    """The module's RDB log lines that the server wrote since the watch started. Redis reopens a
    relative log file name in its current dir, so the watch reads the file of the dir it is in."""

    def __init__(self, env, connection):
        logfile = connection.config_get('logfile')['logfile']
        if logfile in ('', '/dev/null'):
            # Without the log nothing shows that a stream was written or read.
            env.skip()
        self.path = os.path.join(connection.config_get('dir')['dir'], logfile)
        self.start = len(self._lines())

    def _lines(self):
        try:
            with open(self.path, errors='replace') as log:
                return [line.strip() for line in log if LOG_MARK in line]
        except FileNotFoundError:
            return []

    def lines(self):
        return self._lines()[self.start:]


def _field(line, name):
    return int(re.search(rf'\b{name}=(\d+)', line).group(1))


def _log_pid(line):
    match = re.match(r'(\d+):', line)
    return match and match.group(1)


def _expect_stream_save(env, log, min_refs, defs=None):
    """The last save since the watch started wrote a stream that defined and referenced strings,
    `defs` of them when given. Only the lines of the process that ran that save count."""
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
        if defs is not None:
            env.assertEqual(_field(ended[-1], 'defs'), defs, message=ended[-1])
        env.assertGreaterEqual(_field(ended[-1], 'refs'), min_refs)
        env.assertContains('poisoned=false', ended[-1])


def _expect_stream_load(env, log, min_refs):
    lines = log.lines()
    loaded = [line for line in lines if 'phase=loading_ended' in line]
    env.assertTrue(loaded, message=f'no stream load: {lines}')
    if loaded:
        env.assertGreaterEqual(_field(loaded[-1], 'refs'), min_refs)
    env.assertFalse(any('status=refused' in line for line in lines), message=str(lines))


def _wait_until(condition, timeout=60):
    with TimeLimit(timeout):
        while not condition():
            time.sleep(0.05)


def _rdb_size(connection):
    directory = connection.config_get('dir')['dir']
    filename = connection.config_get('dbfilename')['dbfilename']
    return os.path.getsize(os.path.join(directory, filename))


def _inline_rdb_size(connection):
    """The size of an RDB of the current data with sharing off."""
    original = connection.config_get(WRITER_BUDGET)[WRITER_BUDGET]
    try:
        connection.config_set(WRITER_BUDGET, '0')
        connection.save()
        return _rdb_size(connection)
    finally:
        connection.config_set(WRITER_BUDGET, original)


def _enable_active_defrag(env, connection):
    """Makes active defrag start as often and work as hard as it can, as test_defrag.py does, or
    skips where the server can't defragment."""
    connection.config_set('hz', '100')
    connection.config_set('active-defrag-ignore-bytes', '1')
    connection.config_set('active-defrag-threshold-lower', '0')
    connection.config_set('active-defrag-cycle-min', '99')
    try:
        connection.config_set('activedefrag', 'yes')
    except ResponseError:
        env.skip()


def _defrag_info(connection):
    reply = connection.execute_command('JSON.DEBUG', 'DEFRAG_INFO')
    return reply if isinstance(reply, dict) else dict(zip(reply[::2], reply[1::2]))


def _wait_for_defrag_cycles(connection, cycles):
    """Waits until `cycles` more defrag cycles started and ended, and reached every key."""
    before = _defrag_info(connection)
    keys = connection.dbsize()

    def done():
        now = _defrag_info(connection)
        return (now['defrag_started'] >= before['defrag_started'] + cycles and
                now['defrag_ended'] >= before['defrag_ended'] + cycles and
                now['keys_defrag'] >= before['keys_defrag'] + cycles * keys)
    _wait_until(done)


class _DefragWhileLoading(threading.Thread):
    """From a connection of its own, turns active defrag on once a load starts, and off again once
    a cycle runs, while the load still does. So the cycle starts and stops inside the load, and
    nothing moves a string after it."""

    def __init__(self, connection):
        super().__init__(daemon=True)
        self.connection = connection
        self.ran_while_loading = False
        self.stopped_while_loading = False
        self.error = None

    def run(self):
        try:
            deadline = time.monotonic() + 60
            while not self.connection.info('persistence')['loading']:
                if time.monotonic() > deadline:
                    return
                time.sleep(0.001)
            self.connection.config_set('activedefrag', 'yes')
            while True:
                info = self.connection.info('everything')
                if not info['loading']:
                    return
                # A loading server serves clients only between batches of replayed commands, so
                # a cycle seen running here started during the load.
                if info['active_defrag_running'] > 0:
                    self.ran_while_loading = True
                    self.connection.config_set('activedefrag', 'no')
                    self.stopped_while_loading = bool(self.connection.info('persistence')['loading'])
                    return
                time.sleep(0.001)
        except Exception as error:
            self.error = error
        finally:
            self.connection.config_set('activedefrag', 'no')


def _fillers(prefix, count):
    """Small documents, each with a string of its own."""
    return {f'{prefix}:{i}': {'filler': f'{prefix} {i:08d} ' * 8} for i in range(count)}


def _write_fillers(env, connection, prefix, count, fragment=False):
    """Writes filler documents. With `fragment`, deletes every other one, which leaves holes amid
    live data: defrag starts only on a fragmented heap. Returns the documents kept."""
    fillers = _fillers(prefix, count)
    _set_documents(env, connection, fillers)
    if fragment:
        deleted = list(fillers)[::2]
        connection.delete(*deleted)
        for key in deleted:
            del fillers[key]
    return fillers


def _aof_stream_round_trip(env, connection, documents, while_loading=None):
    """Turns AOF on, which writes a base file whose RDB preamble streams `documents`, adds a tail
    of commands that reuse the strings, loads the AOF again and turns it off. Returns the tail.

    With `while_loading`, a thread that runs during the load, the tail also fragments the heap and
    goes on writing before and after its shared-string documents. The preamble fills a fresh heap,
    where defrag finds nothing to do, so the replay of the tail is where it runs during the load:
    a cycle starts before the shared-string documents replay, and stops before the load ends."""
    old_dir = connection.config_get('dir')['dir']
    # The servers of a run share one dir, and an AOF turned on at runtime keeps its default name,
    # so it goes in a dir of its own, as in test_auto_create_paths.py.
    connection.config_set('dir', tempfile.mkdtemp(prefix='aof-', dir=old_dir))
    try:
        connection.config_set('aof-use-rdb-preamble', 'yes')
        log = _ServerLog(env, connection)
        rewrites = connection.info('persistence')['aof_rewrites']
        connection.config_set('appendonly', 'yes')
        _wait_until(lambda: (connection.info('persistence')['aof_rewrites'] > rewrites and
                             not connection.info('persistence')['aof_rewrite_in_progress']))
        env.assertEqual(connection.info('persistence')['aof_last_bgrewrite_status'], 'ok')
        _expect_stream_save(env, log, min_refs=len(documents) - 1)
        # The tail replays after the preamble, while the load still holds the stream's strings.
        tail = {}
        if while_loading:
            tail.update(_write_fillers(env, connection, 'fragmented', 20000, fragment=True))
            tail.update(_write_fillers(env, connection, 'before', 10000))
        shared = _documents(prefix='tail', count=100)
        _set_documents(env, connection, shared)
        tail.update(shared)
        connection.execute_command('JSON.SET', 'checkpoint:0', '$.n', '"updated in the tail"')
        if while_loading:
            tail.update(_write_fillers(env, connection, 'after', 10000))
        log = _ServerLog(env, connection)
        if while_loading:
            while_loading.start()
        env.expect('DEBUG', 'LOADAOF').ok()
        if while_loading:
            while_loading.join(timeout=60)
        _expect_stream_load(env, log, min_refs=len(documents) - 1)
        return tail
    finally:
        connection.config_set('appendonly', 'no')
        connection.config_set('dir', old_dir)


def test_an_aof_preamble_streams_and_loads_in_the_plain_lane():
    """The AOF preamble path, in the lane that the sanitizer and valgrind jobs run."""
    env = _plain_env()
    connection = env.getConnection()
    documents = _documents()
    _set_documents(env, connection, documents)
    tail = _aof_stream_round_trip(env, connection, documents)
    documents['checkpoint:0'] = dict(documents['checkpoint:0'], n='updated in the tail')
    _assert_documents(env, connection, {**documents, **tail})


def test_active_defrag_during_an_aof_stream_load_keeps_the_strings_shared():
    """A defrag cycle that starts while the preamble streams in must leave the string cache to the
    load. Had it reset the cache, the documents decoded before and after it, and those the tail
    writes, would hold separate copies of the same strings, and the next save would define each
    shared string more than once."""
    env = _plain_env()
    connection = env.getConnection()
    _enable_active_defrag(env, connection)
    connection.config_set('activedefrag', 'no')
    documents = _documents()
    _set_documents(env, connection, documents)
    started = _defrag_info(connection)['defrag_started']
    defrag = _DefragWhileLoading(env.getConnection())
    tail = _aof_stream_round_trip(env, connection, documents, while_loading=defrag)
    env.assertIsNone(defrag.error)
    env.assertTrue(defrag.ran_while_loading, message='no defrag cycle ran during the load')
    # Stopped before the load ended, nothing moved a string after the load.
    env.assertTrue(defrag.stopped_while_loading, message='defrag still ran when the load ended')
    env.assertGreater(_defrag_info(connection)['defrag_started'], started)
    documents['checkpoint:0'] = dict(documents['checkpoint:0'], n='updated in the tail')
    documents.update(tail)
    _assert_documents(env, connection, documents)

    log = _ServerLog(env, connection)
    connection.save()
    sharing = sum('shared_checkpoint_text' in document for document in documents.values())
    _expect_stream_save(env, log, min_refs=sharing - 1, defs=SHARED_STRINGS)


def test_defrag_between_two_saves_keeps_the_strings_shared():
    """Defrag moves every string to a new address between two saves. Since the save encoder knows
    a string by its address, the second save must still find one copy of each shared string."""
    env = _plain_env()
    connection = env.getConnection()
    _enable_active_defrag(env, connection)
    connection.config_set('activedefrag', 'no')
    documents = _documents()
    _set_documents(env, connection, documents)
    inline_size = _inline_rdb_size(connection)
    log = _ServerLog(env, connection)
    connection.save()
    _expect_stream_save(env, log, min_refs=KEYS - 1)
    first_size = _rdb_size(connection)

    # Freed filler documents leave holes in the heap, so defrag has strings to move.
    filler = _fillers('filler', KEYS)
    _set_documents(env, connection, filler)
    connection.delete(*filler)
    connection.config_set('activedefrag', 'yes')
    _wait_for_defrag_cycles(connection, cycles=2)

    # Defrag pauses while a child runs, so the BGSAVE child writes the strings where it left them.
    log = _ServerLog(env, connection)
    saves = connection.info('persistence')['rdb_saves']
    connection.bgsave()
    _wait_until(lambda: (connection.info('persistence')['rdb_saves'] > saves and
                         not connection.info('persistence')['rdb_bgsave_in_progress']))
    env.assertEqual(connection.info('persistence')['rdb_last_bgsave_status'], 'ok')
    connection.config_set('activedefrag', 'no')
    _expect_stream_save(env, log, min_refs=KEYS - 1)
    second_size = _rdb_size(connection)
    env.assertLess(second_size * 4, inline_size,
                   message=f'stream RDB {second_size} after defrag, {first_size} before, '
                           f'inline {inline_size}')

    # Load that RDB as the child wrote it.
    log = _ServerLog(env, connection)
    env.expect('DEBUG', 'RELOAD', 'NOSAVE').ok()
    _expect_stream_load(env, log, min_refs=KEYS - 1)
    _assert_documents(env, connection, documents)
