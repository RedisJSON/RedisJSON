# -*- coding: utf-8 -*-
"""
Auto-creation of deep paths.

`json-auto-create-deep-paths` is IMMUTABLE, so it cannot be toggled inside a
running env. Rather than keep two parallel files, every case below states its
expectation for *both* modes in one place, and two thin runners replay the whole
table against an enabled and a disabled env. The disabled column doubles as the
regression guard for today's behavior.

RLTest reuses one server per set of env params, so the two modes cannot be live
at the same time -- hence one env per test function rather than a pair inside
each test.
"""

from collections import namedtuple

from RLTest import Env, Defaults

Defaults.decode_responses = True

AUTO_CREATE_ARGS = 'json-auto-create-deep-paths yes'

KEY = 'k'

#: Expected outcomes. `OK`/`NIL` are literal replies; `ERR(fragment)` requires an
#: error whose message contains `fragment`.
OK = 'OK'
NIL = None

Err = namedtuple('Err', 'fragment')


def ERR(fragment):
    return Err(fragment)


#: doc     -- JSON to seed KEY with, or None to leave the key absent
#: cmd     -- command args after the key
#: on/off  -- expected reply with the config enabled / disabled
#: doc_on  -- expected `JSON.GET KEY $` afterwards with the config enabled
#: doc_off -- ditto, disabled. None means "don't check".
Case = namedtuple('Case', 'doc cmd on off doc_on doc_off')


def case(doc, cmd, on, off, doc_on=None, doc_off=None):
    return Case(doc, cmd, on, off, doc_on, doc_off)


def _env(enabled):
    # noDefaultModuleArgs: don't merge the global --module-args on top of ours.
    if enabled:
        return Env(moduleArgs=AUTO_CREATE_ARGS, noDefaultModuleArgs=True)
    return Env(noDefaultModuleArgs=True)


def _check(env, c, expected, expected_doc, mode):
    """Run one case against one env and assert that mode's expectation."""
    env.cmd('DEL', KEY)
    if c.doc is not None:
        env.expect('JSON.SET', KEY, '$', c.doc).ok()

    label = '[%s] %s %r on %r' % (mode, c.cmd[0], c.cmd[1:], c.doc)
    attempt = env.expect(*(c.cmd[:1] + (KEY,) + c.cmd[1:]))
    if isinstance(expected, Err):
        attempt.raiseError().contains(expected.fragment)
    elif expected is NIL:
        attempt.equal(None)
    else:
        attempt.equal(expected)

    if expected_doc is not None:
        env.assertEqual(env.cmd('JSON.GET', KEY, '$'), expected_doc, message=label)


DEEP = '$.' + '.'.join('a%d' % i for i in range(200))

CASES = [
    # ---------------- JSON.SET, existing key ----------------
    # a whole missing chain
    case('{"a":{}}', ('JSON.SET', '$.a.b.c', '5'),
         OK, NIL, '[{"a":{"b":{"c":5}}}]', '[{"a":{}}]'),
    case('{}', ('JSON.SET', '$.a.b.c.d', '5'),
         OK, NIL, '[{"a":{"b":{"c":{"d":5}}}}]', '[{}]'),
    # only the final key missing: has always worked
    case('{"a":{}}', ('JSON.SET', '$.a.b', '5'),
         OK, OK, '[{"a":{"b":5}}]', '[{"a":{"b":5}}]'),
    # siblings survive
    case('{"a":{"keep":1}}', ('JSON.SET', '$.a.b.c', '5'),
         OK, NIL, '[{"a":{"keep":1,"b":{"c":5}}}]', '[{"a":{"keep":1}}]'),
    # a non-object in the way is a nil, never an error
    case('{"a":3}', ('JSON.SET', '$.a.b.c', '5'),
         NIL, NIL, '[{"a":3}]', '[{"a":3}]'),
    case('{"a":[1,2]}', ('JSON.SET', '$.a.b', '5'),
         NIL, NIL, '[{"a":[1,2]}]', '[{"a":[1,2]}]'),
    # descent through an existing array element is fine
    case('{"a":[{},{}]}', ('JSON.SET', '$.a[1].b.c', '5'),
         OK, NIL, '[{"a":[{},{"b":{"c":5}}]}]', '[{"a":[{},{}]}]'),

    # ---------------- JSON.SET, absent key ----------------
    case(None, ('JSON.SET', '$.a.b.c', '5'),
         OK, ERR('new objects must be created at the root'),
         '[{"a":{"b":{"c":5}}}]'),
    case(None, ('JSON.SET', '$.a', '5'),
         OK, ERR('new objects must be created at the root'), '[{"a":5}]'),
    case(None, ('JSON.SET', '$.a.b', '{"d":{}}'),
         OK, ERR('new objects must be created at the root'),
         '[{"a":{"b":{"d":{}}}}]'),
    # a segment we cannot invent keeps the error in both modes
    case(None, ('JSON.SET', '$.a[0].b', '5'),
         ERR('new objects must be created at the root'),
         ERR('new objects must be created at the root')),
    case(None, ('JSON.SET', '$..a', '5'),
         ERR('new objects must be created at the root'),
         ERR('new objects must be created at the root')),

    # ---------------- NX / XX ----------------
    # XX means "only update what exists", so it must never create
    case('{}', ('JSON.SET', '$.a.b.c', '5', 'XX'), NIL, NIL, '[{}]', '[{}]'),
    case(None, ('JSON.SET', '$.a.b', '5', 'XX'), NIL, NIL),
    # NX creates
    case('{}', ('JSON.SET', '$.a.b.c', '5', 'NX'),
         OK, NIL, '[{"a":{"b":{"c":5}}}]', '[{}]'),
    # ... but never overwrites
    case('{"a":{"b":{"c":1}}}', ('JSON.SET', '$.a.b.c', '5', 'NX'),
         NIL, NIL, '[{"a":{"b":{"c":1}}}]', '[{"a":{"b":{"c":1}}}]'),

    # ---------------- multi-target paths ----------------
    # a union creates the missing target and updates the existing one
    case('{"a":{},"b":{"c":1}}', ('JSON.SET', "$['a','b'].c", '9'),
         OK, OK, '[{"a":{"c":9},"b":{"c":9}}]', '[{"a":{},"b":{"c":9}}]'),
    # non-object matches are skipped rather than failing the command
    case('{"p":{},"q":{},"s":"str"}', ('JSON.SET', '$.*.n', '7'),
         OK, ERR('static path'),
         '[{"p":{"n":7},"q":{"n":7},"s":"str"}]', '[{"p":{},"q":{},"s":"str"}]'),
    # a descendant matches the root as well as every node below it
    case('{"a":{}}', ('JSON.SET', '$..n', '7'),
         OK, ERR('static path'), '[{"a":{"n":7},"n":7}]', '[{"a":{}}]'),
    case('{"a":[{},{},{}]}', ('JSON.SET', '$.a[0:2].b', '7'),
         OK, ERR('static path'),
         '[{"a":[{"b":7},{"b":7},{}]}]', '[{"a":[{},{},{}]}]'),

    # ------ a path that matches but creates nothing still updates ------
    # creation must not turn a working multi-target update into an error
    case('{"a":{"a":1}}', ('JSON.SET', '$..a', '5'),
         OK, OK, '[{"a":5}]', '[{"a":5}]'),
    case('{"p":{"n":1},"q":{"n":2}}', ('JSON.SET', '$.*.n', '9'),
         OK, OK, '[{"p":{"n":9},"q":{"n":9}}]', '[{"p":{"n":9},"q":{"n":9}}]'),
    case('{"arr":[1,2]}', ('JSON.SET', '$..[0]', '9'),
         OK, OK, '[{"arr":[9,2]}]', '[{"arr":[9,2]}]'),
    case('{"a":{"a":1}}', ('JSON.MERGE', '$..a', '{"a":"b"}'),
         OK, OK, '[{"a":{"a":{"a":"b"}}}]', '[{"a":{"a":{"a":"b"}}}]'),

    # ---------------- array indexes are never created ----------------
    case('{"arr":[]}', ('JSON.SET', '$.arr[0]', '7'),
         ERR('array index out of range'), ERR('array index out of range'),
         '[{"arr":[]}]', '[{"arr":[]}]'),
    case('{"arr":[]}', ('JSON.SET', '$.a.b[0]', '7'),
         ERR('array index out of range'), ERR('array index out of range')),
    case('{"arr":[]}', ('JSON.SET', '$..[9]', '7'),
         ERR('static path'), ERR('static path')),

    # ---------------- JSON.MERGE ----------------
    case('{"a":{}}', ('JSON.MERGE', '$.a.b.c', '5'),
         OK, NIL, '[{"a":{"b":{"c":5}}}]', '[{"a":{}}]'),
    case(None, ('JSON.MERGE', '$.a.b.c', '5'),
         OK, ERR('new objects must be created at the root'),
         '[{"a":{"b":{"c":5}}}]'),
    case('{"a":{}}', ('JSON.MERGE', '$.a.b', '5'),
         OK, OK, '[{"a":{"b":5}}]', '[{"a":{"b":5}}]'),
    case('{"a":3}', ('JSON.MERGE', '$.a.b', '5'), NIL, NIL),
    case(None, ('JSON.MERGE', '$.a[0].b', '5'),
         ERR('new objects must be created at the root'),
         ERR('new objects must be created at the root')),
    case('{"a":{},"b":{"c":{"x":1}}}', ('JSON.MERGE', "$['a','b'].c", '{"y":2}'),
         OK, OK,
         '[{"a":{"c":{"y":2}},"b":{"c":{"x":1,"y":2}}}]',
         '[{"a":{},"b":{"c":{"x":1,"y":2}}}]'),
    # A top-level null patch replaces; it only deletes as an object member. So a
    # created path holding null matches merging null into one that already
    # exists -- neither deletes anything.
    case('{"a":{"b":1}}', ('JSON.MERGE', '$.a', 'null'),
         OK, OK, '[{"a":null}]', '[{"a":null}]'),
    case('{}', ('JSON.MERGE', '$.a.b', 'null'),
         OK, NIL, '[{"a":{"b":null}}]', '[{}]'),
    case('{"a":{"b":1,"c":2}}', ('JSON.MERGE', '$.a', '{"b":null}'),
         OK, OK, '[{"a":{"c":2}}]', '[{"a":{"c":2}}]'),

    # ---------------- legacy and projection paths ----------------
    # legacy (dot) paths create too
    case('{"a":{}}', ('JSON.SET', '.a.b.c', '5'),
         OK, NIL, '[{"a":{"b":{"c":5}}}]', '[{"a":{}}]'),
    # a computed expression is read-only in both modes
    case('{"a":1}', ('JSON.SET', '$.a + 1', '5'),
         ERR('computed/projection expressions'),
         ERR('computed/projection expressions')),

    # ------ value formats survive creation (the leaf is never re-serialized) --
    case('{}', ('JSON.SET', '$.a.b', '{"x":[1,2],"y":null}'),
         OK, NIL, '[{"a":{"b":{"x":[1,2],"y":null}}}]', '[{}]'),
    case('{}', ('JSON.SET', '$.a.b', '[1.5,2.5,3.5]', 'FPHA', 'FP32'),
         OK, NIL, '[{"a":{"b":[1.5,2.5,3.5]}}]', '[{}]'),

    # ---------------- MAX_DEPTH, nothing half-created ----------------
    case('{"keep":1}', ('JSON.SET', DEEP, '5'),
         ERR('recursion limit exceeded'), NIL, '[{"keep":1}]', '[{"keep":1}]'),
    case('{"keep":1}', ('JSON.MERGE', DEEP, '5'),
         ERR('recursion limit exceeded'), NIL, '[{"keep":1}]', '[{"keep":1}]'),
    case(None, ('JSON.SET', DEEP, '5'),
         ERR('recursion limit exceeded'),
         ERR('new objects must be created at the root')),
]


def test_all_cases_with_auto_create_enabled():
    env = _env(True)
    env.expect('CONFIG', 'GET', 'json-auto-create-deep-paths').equal(
        ['json-auto-create-deep-paths', 'yes'])
    # load-time only
    env.expect('CONFIG', 'SET', 'json-auto-create-deep-paths', 'no').raiseError()
    for c in CASES:
        _check(env, c, c.on, c.doc_on, 'ON')


def test_all_cases_with_auto_create_disabled():
    """The disabled column is the regression guard for today's behavior."""
    env = _env(False)
    env.expect('CONFIG', 'GET', 'json-auto-create-deep-paths').equal(
        ['json-auto-create-deep-paths', 'no'])
    for c in CASES:
        _check(env, c, c.off, c.doc_off, 'OFF')


def test_created_document_survives_rdb_reload():
    env = _env(True)
    env.skipOnCluster()
    if env.useAof:
        env.skip()
    env.expect('JSON.SET', 'k', '$.a.b.c', '5').ok()
    for _ in env.retry_with_rdb_reload():
        env.assertExists('k')
        env.expect('JSON.GET', 'k', '$').equal('[{"a":{"b":{"c":5}}}]')
