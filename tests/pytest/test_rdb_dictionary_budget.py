"""The string table budget of RDB encoding v5 on a real server (MOD-20882).

`json-rdb-dictionary-max-memory-percent` caps the string table of one save at a share of the memory
Redis uses when the save starts, and 0 turns sharing off. Each save reads it when it starts, and a
module argument outside 0 to 100 counts as its nearest bound. A table that fills is cleared before
the next document that defines strings. These tests check each rule on one plain server, through
the module's log lines and a reload of every document.
"""

import hashlib
import json
import os
import random
import re
import string
import time

from RLTest import Env
from redis.exceptions import ResponseError

from common import TimeLimit


WRITER_BUDGET = 'json-rdb-dictionary-max-memory-percent'
LOG_MARK = 'JSON RDB dictionary'
# Pairs of documents that hold the same strings, none of them shared with another pair, plus a few
# strings that every document holds.
PAIRS = 100
STRINGS_PER_PAIR = 1000
COMMON_STRINGS = 10
# What a fork child dirties besides the table: page rounding, allocator metadata and buffers.
COW_SLACK = 1 << 20


def _plain_server(env):
    """These tests read one server's log and settings, so they run only in the plain lane."""
    if env.isCluster() or env.useAof or env.useSlaves or env.env.endswith('existing-env'):
        env.skip()


class _ServerLog:
    """The module's RDB log lines that the server wrote since the watch started."""

    def __init__(self, env):
        self.env = env
        self.path = os.path.join(env.logDir, env.envRunner._getFileName('master', '.log'))
        try:
            self.start = len(self._lines())
        except OSError:
            # Without the log nothing shows what a save did.
            env.skip()

    def _lines(self):
        with open(self.path, errors='replace') as log:
            return [line.strip() for line in log if LOG_MARK in line]

    def lines(self):
        return self._lines()[self.start:]

    def last(self, phase):
        lines = [line for line in self.lines() if f'phase={phase}' in line]
        self.env.assertTrue(lines, message=f'no {phase}: {self.lines()}')
        return lines[-1] if lines else ''

    def summary_of(self, start):
        """The end of the save that logged `start`: only the lines of its process count."""
        pid = _log_pid(start)
        lines = [line for line in self.lines()
                 if 'phase=persistence_ended' in line and _log_pid(line) == pid]
        self.env.assertTrue(lines, message=f'no stream summary: {self.lines()}')
        return lines[-1] if lines else ''


def _field(line, name):
    match = re.search(rf'\b{name}=(\d+)', line)
    return int(match.group(1)) if match else -1


def _log_pid(line):
    match = re.match(r'(\d+):', line)
    return match and match.group(1)


def _retry_while_saving(operation):
    with TimeLimit(30):
        while True:
            try:
                return operation()
            except ResponseError as error:
                if 'Background save already in progress' not in str(error):
                    raise
                time.sleep(0.01)


def _wait_for_bgsave(env, connection):
    with TimeLimit(120):
        while connection.info('persistence')['rdb_bgsave_in_progress']:
            time.sleep(0.01)
    env.assertEqual(connection.info('persistence')['rdb_last_bgsave_status'], 'ok')


def _set_documents(env, documents):
    for key, document in documents.items():
        env.assertEqual(env.cmd('JSON.SET', key, '$', json.dumps(document)), 'OK')


def _assert_documents(env, documents):
    for key, document in documents.items():
        env.assertEqual(json.loads(env.cmd('JSON.GET', key)), document)


def _shared_documents(keys=50):
    """Documents that repeat two long strings under object keys long enough to be shared too."""
    rng = random.Random(20882)
    first = ''.join(rng.choices(string.ascii_letters + string.digits, k=2048))
    second = ''.join(rng.choices(string.ascii_letters + string.digits, k=512))
    return {f'budget:{i}': {'shared_budget_text': first, 'second_shared_field': second, 'n': i}
            for i in range(keys)}


def _pair_documents():
    """Many distinct strings, each held by the two documents of one pair. The save writes keys in
    no set order, so a cleared table rarely still holds the strings of a document's pair; the
    strings that every document holds are referenced after each reset."""
    common = [hashlib.sha256(f'common:{i}'.encode()).hexdigest()[:16] for i in range(COMMON_STRINGS)]
    documents = {}
    for pair in range(PAIRS):
        strings = [hashlib.sha256(f'{pair}:{i}'.encode()).hexdigest()[:16]
                   for i in range(STRINGS_PER_PAIR)]
        documents[f'pair:{pair}:a'] = {'common': common, 'own': strings}
        documents[f'pair:{pair}:b'] = {'common': common, 'own': strings}
    return documents


def _reload_and_check(env, documents, min_refs):
    """`DEBUG RELOAD NOSAVE` loads the RDB on disk: a stream, with every document as written."""
    log = _ServerLog(env)
    env.assertEqual(env.cmd('DEBUG', 'RELOAD', 'NOSAVE'), 'OK')
    _assert_documents(env, documents)
    env.assertGreaterEqual(_field(log.last('loading_ended'), 'refs'), min_refs)
    env.assertFalse(any('status=refused' in line for line in log.lines()), message=str(log.lines()))


def test_a_config_change_applies_from_the_next_save(env):
    _plain_server(env)
    connection = env.getConnection()
    documents = _shared_documents()
    _set_documents(env, documents)
    original = connection.config_get(WRITER_BUDGET)[WRITER_BUDGET]
    try:
        # The table budget is the configured share of the memory Redis uses when the save starts.
        connection.config_set(WRITER_BUDGET, 7)
        log = _ServerLog(env)
        used_memory = connection.info('memory')['used_memory']
        env.assertTrue(_retry_while_saving(connection.save))
        start = log.last('persistence_start')
        env.assertContains('mode=stream percent=7 ', start)
        expected = used_memory * 7 // 100
        env.assertAlmostEqual(_field(start, 'budget_bytes'), expected, expected // 10, message=start)

        # A save that runs keeps the setting it started with: turning sharing off meanwhile
        # changes nothing in it.
        log = _ServerLog(env)
        connection.config_set('rdb-key-save-delay', 10000)
        try:
            env.assertTrue(_retry_while_saving(connection.bgsave))
        finally:
            connection.config_set('rdb-key-save-delay', 0)
        connection.config_set(WRITER_BUDGET, 0)
        env.assertEqual(connection.info('persistence')['rdb_bgsave_in_progress'], 1)
        _wait_for_bgsave(env, connection)
        start = log.last('persistence_start')
        env.assertContains('mode=stream percent=7 ', start)
        summary = log.summary_of(start)
        env.assertGreaterEqual(_field(summary, 'refs'), len(documents) - 1, message=summary)
        _reload_and_check(env, documents, min_refs=len(documents) - 1)

        # The next save takes the new setting.
        log = _ServerLog(env)
        env.assertTrue(_retry_while_saving(connection.save))
        env.assertContains('mode=plain reason=off percent=0 budget_bytes=0', log.last('persistence_start'))
        env.assertFalse(any('phase=persistence_ended' in line for line in log.lines()),
                        message=str(log.lines()))
    finally:
        connection.config_set(WRITER_BUDGET, original)


def _save_with_module_argument(argument, start_line):
    """A server whose module argument is out of range: its save logs `start_line`, and every
    document reloads. Returns the env, its documents, the log, the save's start line and the
    used_memory just before the save."""
    env = Env(moduleArgs=f'{WRITER_BUDGET} {argument}', noDefaultModuleArgs=True,
              decodeResponses=True)
    _plain_server(env)
    connection = env.getConnection()
    documents = _shared_documents()
    _set_documents(env, documents)
    log = _ServerLog(env)
    used_memory = connection.info('memory')['used_memory']
    env.assertTrue(_retry_while_saving(connection.save))
    start = log.last('persistence_start')
    env.assertContains(start_line, start)
    env.expect('DEBUG', 'RELOAD').ok()
    _assert_documents(env, documents)
    # CONFIG SET still checks the range.
    env.expect('CONFIG', 'SET', WRITER_BUDGET, argument).raiseError()
    env.expect('CONFIG', 'SET', WRITER_BUDGET, '100').ok()
    return env, documents, log, start, used_memory


def test_a_module_argument_above_100_counts_as_100():
    env, documents, log, start, used_memory = _save_with_module_argument('150', 'mode=stream percent=100 ')
    # The whole of used_memory, not 150% of it.
    env.assertAlmostEqual(_field(start, 'budget_bytes'), used_memory, used_memory // 10, message=start)
    env.assertGreaterEqual(_field(log.summary_of(start), 'refs'), len(documents) - 1)


def test_a_negative_module_argument_turns_sharing_off():
    _save_with_module_argument('-5', 'mode=plain reason=off percent=0 budget_bytes=0')


def test_a_table_that_fills_resets_and_stays_within_its_budget(env):
    _plain_server(env)
    connection = env.getConnection()
    documents = _pair_documents()
    _set_documents(env, documents)
    distinct = PAIRS * STRINGS_PER_PAIR + COMMON_STRINGS
    references = PAIRS * STRINGS_PER_PAIR + COMMON_STRINGS * (len(documents) - 1)
    original = connection.config_get(WRITER_BUDGET)[WRITER_BUDGET]
    cow, starts, summaries = {}, {}, {}
    try:
        for percent in (0, 100, 1):
            connection.config_set(WRITER_BUDGET, percent)
            log = _ServerLog(env)
            env.assertTrue(_retry_while_saving(connection.bgsave))
            _wait_for_bgsave(env, connection)
            cow[percent] = connection.info('persistence')['rdb_last_cow_size']
            starts[percent] = log.last('persistence_start')
            if percent:
                summaries[percent] = log.summary_of(starts[percent])
    finally:
        connection.config_set(WRITER_BUDGET, original)

    # A table with room for every string defines each one once.
    env.assertEqual(_field(summaries[100], 'defs'), distinct, message=summaries[100])
    env.assertEqual(_field(summaries[100], 'refs'), references, message=summaries[100])
    env.assertEqual(_field(summaries[100], 'resets'), 0, message=summaries[100])
    # A 1% table fills, is cleared and fills again, and still shares strings.
    env.assertGreater(_field(summaries[1], 'resets'), 0, message=summaries[1])
    env.assertGreater(_field(summaries[1], 'refs'), 0, message=summaries[1])
    env.assertContains('poisoned=false', summaries[1])

    # The fork child holds the table, so its copy-on-write memory shows how big the table got.
    # Only Linux reports it, and only jemalloc builds keep it free of sanitizer or valgrind noise.
    allocator = connection.info('memory')['mem_allocator']
    if allocator.startswith('jemalloc') and cow[0] > 0:
        budget = _field(starts[1], 'budget_bytes')
        message = f'cow {cow}, 1% budget {budget}'
        env.assertGreater(cow[100] - cow[0], budget + COW_SLACK, message=message)
        env.assertLessEqual(cow[1] - cow[0], budget + COW_SLACK, message=message)

    # The RDB of the 1% save, with its resets, loads every document.
    _reload_and_check(env, documents, min_refs=1)
