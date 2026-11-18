"""RDB encoding v5 with unusual data shapes (MOD-20882).

Each test saves a stream, reads the counters the module logged for it, and reloads every document
exactly. The rules under test: only strings of 8 UTF-8 bytes or more that two or more owners hold
are shared, object keys share the same strings as values, a document larger than the string table
keeps the rest of its strings inline, and the table is per save, so changes between saves need no
care.
"""

import json
import os
import random
import re
import string

from RLTest import Env

from common import TimeLimit


WRITER_BUDGET = 'json-rdb-dictionary-max-memory-percent'
LOG_MARK = 'JSON RDB dictionary'


def _is_flex(env):
    return env.getConnection().config_get('bigredis-enabled').get('bigredis-enabled') == 'yes'


def _plain_env():
    """A single server with the server log on. Replicas would start saves of their own, and Flex
    saves each value on its own (`mode=local reason=flex`), so it writes no stream to check."""
    env = Env(decodeResponses=True)
    if (env.isCluster() or env.useSlaves or env.useAof or env.env.endswith('existing-env') or
            _is_flex(env)):
        env.skip()
    env.expect('FLUSHALL').equal(True)
    return env


def _text(seed, size=64):
    rng = random.Random(seed)
    return ''.join(rng.choices(string.ascii_letters + string.digits, k=size))


class _ServerLog:
    """The module's RDB log lines that the server wrote since the watch started."""

    def __init__(self, env):
        self.path = os.path.join(env.logDir, env.envRunner._getFileName('master', '.log'))
        try:
            self.start = len(self._lines())
        except OSError:
            # Without the log nothing shows how the strings were saved.
            env.skip()

    def _lines(self):
        with open(self.path, errors='replace') as log:
            return [line.strip() for line in log if LOG_MARK in line]

    def lines(self):
        return self._lines()[self.start:]


def _log_pid(line):
    match = re.match(r'(\d+):', line)
    return match and match.group(1)


def _counters(env, lines):
    """The counters of the last stream that a save wrote, from the process that ran that save."""
    started = [line for line in lines if 'phase=persistence_start' in line]
    env.assertTrue(started, message=f'no save started: {lines}')
    if not started:
        return {}
    env.assertContains('mode=stream', started[-1])
    pid = _log_pid(started[-1])
    ended = [line for line in lines if 'phase=persistence_ended' in line and _log_pid(line) == pid]
    env.assertTrue(ended, message=f'no stream summary: {lines}')
    if not ended:
        return {}
    fields = dict(re.findall(r'\b(\w+)=(\S+)', ended[-1].split('phase=persistence_ended', 1)[1]))
    env.assertEqual(fields.get('poisoned'), 'false', message=ended[-1])
    return {name: int(value) for name, value in fields.items() if value.isdigit()}


def _save(env):
    """SAVE, then the counters of the stream it wrote."""
    log = _ServerLog(env)
    env.assertTrue(env.getConnection().save())
    return _counters(env, log.lines())


def _set(env, documents):
    for key, document in documents.items():
        env.expect('JSON.SET', key, '$', json.dumps(document)).ok()


def _reload_and_compare(env, documents):
    """`DEBUG RELOAD` saves and loads in one process; every document must come back as written."""
    log = _ServerLog(env)
    with TimeLimit(120):
        env.expect('DEBUG', 'RELOAD').ok()
    lines = log.lines()
    env.assertFalse(any('status=refused' in line for line in lines), message=str(lines))
    for key, document in documents.items():
        env.assertEqual(json.loads(env.cmd('JSON.GET', key)), document, message=key)
    return lines


def _rdb_size(env):
    connection = env.getConnection()
    directory = connection.config_get('dir')['dir']
    return os.path.getsize(os.path.join(directory, connection.config_get('dbfilename')['dbfilename']))


def test_only_strings_of_eight_bytes_or_more_are_shared():
    env = _plain_env()
    # The limit counts UTF-8 bytes: 'a' plus three 2-byte letters is 7 bytes, 'ab' plus three is 8.
    seven, eight = 'aééé', 'abééé'
    env.assertEqual((len(seven.encode()), len(eight.encode())), (7, 8))
    short = {f'short:{i}': {'seven': seven, 'empty': '', 'tiny': 'abc', 'n': i} for i in range(3)}
    _set(env, short)
    counters = _save(env)
    env.assertEqual((counters.get('defs'), counters.get('refs')), (0, 0), message=str(counters))

    emoji, cjk = '\U0001F600 shared ☃ text', '共有される文字列'
    shared = {f'shared:{i}': {'eight': eight, 'emoji': emoji, 'cjk': cjk, 'n': i} for i in range(3)}
    _set(env, shared)
    counters = _save(env)
    # The three long strings are defined by the first document and referenced by the other two.
    # The object keys are shorter than 8 bytes, so they stay inline.
    env.assertEqual((counters.get('defs'), counters.get('refs')), (3, 6), message=str(counters))
    _reload_and_compare(env, {**short, **shared})


def test_a_document_larger_than_the_table_keeps_the_rest_inline():
    env = _plain_env()
    connection = env.getConnection()
    strings = [_text(seed, 16) for seed in range(20000)]
    # Each string appears twice in the one document, so it has two owners and may be shared.
    documents = {'big': {'first': strings, 'second': strings}}
    _set(env, documents)
    original = connection.config_get(WRITER_BUDGET)[WRITER_BUDGET]
    try:
        # 1% of used_memory holds far fewer than 20,000 strings.
        connection.config_set(WRITER_BUDGET, '1')
        counters = _save(env)
        env.assertGreater(counters.get('defs', 0), 0, message=str(counters))
        env.assertLess(counters.get('defs', len(strings)), len(strings), message=str(counters))
        # Only the strings defined before the table filled are referenced in the second list.
        env.assertEqual(counters.get('refs'), counters.get('defs'), message=str(counters))
        env.assertEqual(counters.get('resets'), 0, message=str(counters))

        # A second document after the full one starts with a cleared table.
        env.expect('COPY', 'big', 'big:copy').equal(True)
        documents['big:copy'] = documents['big']
        counters = _save(env)
        env.assertGreaterEqual(counters.get('resets', 0), 1, message=str(counters))
        _reload_and_compare(env, documents)
    finally:
        connection.config_set(WRITER_BUDGET, original)


def test_megabyte_strings_and_typed_arrays_next_to_shared_strings():
    env = _plain_env()
    big = _text(seed=9, size=2 * 1024 * 1024)
    floats = [i + 0.5 for i in range(64)]
    documents = {f'mixed:{i}': {'big': big, 'floats': floats, 'shared': _text(seed=10), 'n': i}
                 for i in range(3)}
    _set(env, documents)
    env.expect('JSON.SET', 'untyped', '$', json.dumps(floats)).ok()
    for key in documents:
        env.expect('JSON.SET', key, '$.floats', json.dumps(floats), 'FPHA', 'FP32').ok()
    typed = env.cmd('JSON.DEBUG', 'MEMORY', 'mixed:0', '$.floats')
    untyped = env.cmd('JSON.DEBUG', 'MEMORY', 'untyped', '$')
    # Without a hint ijson picks the smallest element type that holds these numbers, so the FP32
    # hint shows as another size. After the reload each array must keep its own.
    env.assertNotEqual(typed, untyped)

    counters = _save(env)
    # The big string and the 64-byte one, each defined once and referenced by the other two.
    env.assertEqual((counters.get('defs'), counters.get('refs')), (2, 4), message=str(counters))
    # Inline, the big string would be written three times.
    env.assertLess(_rdb_size(env), 2 * len(big))

    documents['untyped'] = floats
    _reload_and_compare(env, documents)
    for key in documents:
        if key != 'untyped':
            env.assertEqual(env.cmd('JSON.DEBUG', 'MEMORY', key, '$.floats'), typed, message=key)
    env.assertEqual(env.cmd('JSON.DEBUG', 'MEMORY', 'untyped', '$'), untyped)


def test_copy_merge_and_a_key_used_as_a_value_add_owners():
    env = _plain_env()
    unique, key_text = _text(seed=4), 'an_object_key_long_enough'
    documents = {'source': {'only': unique}, 'merged': {'other': 1}, 'keyed': {key_text: 1}}
    _set(env, documents)
    counters = _save(env)
    env.assertEqual((counters.get('defs'), counters.get('refs')), (0, 0), message=str(counters))

    # COPY and JSON.MERGE add owners of `unique`; a value equal to an object key shares its string.
    env.expect('COPY', 'source', 'copy').equal(True)
    env.expect('JSON.MERGE', 'merged', '$', json.dumps({'added': unique})).ok()
    env.expect('JSON.SET', 'valued', '$', json.dumps({'value': key_text})).ok()
    documents.update(copy={'only': unique}, merged={'other': 1, 'added': unique},
                     valued={'value': key_text})
    counters = _save(env)
    env.assertEqual((counters.get('defs'), counters.get('refs')), (2, 3), message=str(counters))

    # Changing the copy after the save leaves the source as it was.
    env.expect('JSON.SET', 'copy', '$.only', json.dumps(_text(seed=5))).ok()
    documents['copy'] = {'only': _text(seed=5)}
    _reload_and_compare(env, documents)


def test_documents_changed_between_saves_round_trip():
    env = _plain_env()
    shared, other = _text(seed=6), _text(seed=7)
    documents = {f'changed:{i}': {'text': shared, 'note': other, 'n': i} for i in range(10)}
    _set(env, documents)
    counters = _save(env)
    env.assertEqual((counters.get('defs'), counters.get('refs')), (2, 18), message=str(counters))

    # Each change frees or replaces a shared string after the first save wrote its table.
    env.expect('JSON.STRAPPEND', 'changed:0', '$.text', json.dumps('-tail')).equal([len(shared) + 5])
    documents['changed:0']['text'] = shared + '-tail'
    env.expect('JSON.DEL', 'changed:1', '$.text').equal(1)
    del documents['changed:1']['text']
    env.expect('JSON.SET', 'changed:2', '$.text', json.dumps(other)).ok()
    documents['changed:2']['text'] = other
    env.expect('DEL', 'changed:3').equal(1)
    del documents['changed:3']
    env.expect('JSON.SET', 'changed:new', '$', json.dumps({'text': shared + '-tail'})).ok()
    documents['changed:new'] = {'text': shared + '-tail'}

    counters = _save(env)
    # `shared` and `other` still have several owners, so the second save shares them again.
    env.assertGreaterEqual(counters.get('defs', 0), 2, message=str(counters))
    _reload_and_compare(env, documents)


def test_a_shared_string_at_the_nesting_limit_round_trips():
    env = _plain_env()
    leaf = _text(seed=11)

    def nested(depth):
        document = {'leaf': leaf}
        for _ in range(depth):
            document = {'nested': document}
        return document

    # 126 levels above the leaf object is the deepest document JSON.SET accepts.
    env.expect('JSON.SET', 'too-deep', '$', json.dumps(nested(127))).raiseError().contains('recursion limit')
    documents = {f'deep:{i}': nested(126) for i in range(3)}
    _set(env, documents)
    counters = _save(env)
    env.assertEqual((counters.get('defs'), counters.get('refs')), (1, 2), message=str(counters))
    _reload_and_compare(env, documents)
