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

#: The hash tag keeps both keys in one slot, so a multi-key JSON.MSET is not a
#: CROSSSLOT error under the oss-cluster topology.
KEY = 'k{s}'
KEY2 = 'k2{s}'

#: Expected outcomes. `OK`/`NIL` are literal replies; `ERR(fragment)` requires an
#: error whose message contains `fragment`.
OK = 'OK'
NIL = None

Err = namedtuple('Err', 'fragment')


def ERR(fragment):
    return Err(fragment)


#: doc      -- JSON to seed KEY with, or None to leave the key absent
#: cmd      -- the command to run, keys included, exactly as it goes on the wire
#: on/off   -- expected reply with the config enabled / disabled
#: doc_on   -- expected `JSON.GET KEY $` afterwards with the config enabled
#: doc_off  -- ditto, disabled. None means "don't check".
#: doc2*    -- the same four for KEY2, for the multi-key JSON.MSET cases. A case
#:             that wants KEY2 has to name it in `cmd` itself.
Case = namedtuple('Case', 'doc cmd on off doc_on doc_off doc2 doc2_on doc2_off')


def case(doc, cmd, on, off, doc_on=None, doc_off=None,
         doc2=None, doc2_on=None, doc2_off=None):
    return Case(doc, cmd, on, off, doc_on, doc_off, doc2, doc2_on, doc2_off)


def _env(enabled):
    # noDefaultModuleArgs: don't merge the global --module-args on top of ours.
    if enabled:
        return Env(moduleArgs=AUTO_CREATE_ARGS, noDefaultModuleArgs=True)
    return Env(noDefaultModuleArgs=True)


def _check(env, c, expected, expected_doc, expected_doc2, mode):
    """Run one case against one env and assert that mode's expectation."""
    env.cmd('DEL', KEY, KEY2)
    for key, doc in ((KEY, c.doc), (KEY2, c.doc2)):
        if doc is not None:
            env.expect('JSON.SET', key, '$', doc).ok()

    label = '[%s] %s %r on %r' % (mode, c.cmd[0], c.cmd[1:], c.doc)
    attempt = env.expect(*c.cmd)
    if isinstance(expected, Err):
        attempt.raiseError().contains(expected.fragment)
    elif expected is NIL:
        attempt.equal(None)
    else:
        attempt.equal(expected)

    for key, doc in ((KEY, expected_doc), (KEY2, expected_doc2)):
        if doc is not None:
            env.assertEqual(env.cmd('JSON.GET', key, '$'), doc, message=label)


DEEP = '$.' + '.'.join('a%d' % i for i in range(200))

CASES = [
    # ---------------- JSON.SET, existing key ----------------
    # a whole missing chain
    case('{"a":{}}', ('JSON.SET', KEY, '$.a.b.c', '5'),
         OK, NIL, '[{"a":{"b":{"c":5}}}]', '[{"a":{}}]'),
    case('{}', ('JSON.SET', KEY, '$.a.b.c.d', '5'),
         OK, NIL, '[{"a":{"b":{"c":{"d":5}}}}]', '[{}]'),
    # only the final key missing: has always worked
    case('{"a":{}}', ('JSON.SET', KEY, '$.a.b', '5'),
         OK, OK, '[{"a":{"b":5}}]', '[{"a":{"b":5}}]'),
    # siblings survive
    case('{"a":{"keep":1}}', ('JSON.SET', KEY, '$.a.b.c', '5'),
         OK, NIL, '[{"a":{"keep":1,"b":{"c":5}}}]', '[{"a":{"keep":1}}]'),
    # a non-object in the way is a nil, never an error
    case('{"a":3}', ('JSON.SET', KEY, '$.a.b.c', '5'),
         NIL, NIL, '[{"a":3}]', '[{"a":3}]'),
    case('{"a":[1,2]}', ('JSON.SET', KEY, '$.a.b', '5'),
         NIL, NIL, '[{"a":[1,2]}]', '[{"a":[1,2]}]'),
    # descent through an existing array element is fine
    case('{"a":[{},{}]}', ('JSON.SET', KEY, '$.a[1].b.c', '5'),
         OK, NIL, '[{"a":[{},{"b":{"c":5}}]}]', '[{"a":[{},{}]}]'),

    # ---------------- JSON.SET, absent key ----------------
    case(None, ('JSON.SET', KEY, '$.a.b.c', '5'),
         OK, ERR('new objects must be created at the root'),
         '[{"a":{"b":{"c":5}}}]'),
    case(None, ('JSON.SET', KEY, '$.a', '5'),
         OK, ERR('new objects must be created at the root'), '[{"a":5}]'),
    case(None, ('JSON.SET', KEY, '$.a.b', '{"d":{}}'),
         OK, ERR('new objects must be created at the root'),
         '[{"a":{"b":{"d":{}}}}]'),
    # a segment we cannot invent keeps the error in both modes
    case(None, ('JSON.SET', KEY, '$.a[0].b', '5'),
         ERR('new objects must be created at the root'),
         ERR('new objects must be created at the root')),
    case(None, ('JSON.SET', KEY, '$..a', '5'),
         ERR('new objects must be created at the root'),
         ERR('new objects must be created at the root')),

    # ---------------- NX / XX ----------------
    # XX means "only update what exists", so it must never create
    case('{}', ('JSON.SET', KEY, '$.a.b.c', '5', 'XX'), NIL, NIL, '[{}]', '[{}]'),
    case(None, ('JSON.SET', KEY, '$.a.b', '5', 'XX'), NIL, NIL),
    # NX creates
    case('{}', ('JSON.SET', KEY, '$.a.b.c', '5', 'NX'),
         OK, NIL, '[{"a":{"b":{"c":5}}}]', '[{}]'),
    # ... but never overwrites
    case('{"a":{"b":{"c":1}}}', ('JSON.SET', KEY, '$.a.b.c', '5', 'NX'),
         NIL, NIL, '[{"a":{"b":{"c":1}}}]', '[{"a":{"b":{"c":1}}}]'),

    # ---------------- multi-target paths ----------------
    # a union creates the missing target and updates the existing one
    case('{"a":{},"b":{"c":1}}', ('JSON.SET', KEY, "$['a','b'].c", '9'),
         OK, OK, '[{"a":{"c":9},"b":{"c":9}}]', '[{"a":{},"b":{"c":9}}]'),
    # non-object matches are skipped rather than failing the command
    case('{"p":{},"q":{},"s":"str"}', ('JSON.SET', KEY, '$.*.n', '7'),
         OK, ERR('static path'),
         '[{"p":{"n":7},"q":{"n":7},"s":"str"}]', '[{"p":{},"q":{},"s":"str"}]'),
    # a descendant matches the root as well as every node below it
    case('{"a":{}}', ('JSON.SET', KEY, '$..n', '7'),
         OK, ERR('static path'), '[{"a":{"n":7},"n":7}]', '[{"a":{}}]'),
    case('{"a":[{},{},{}]}', ('JSON.SET', KEY, '$.a[0:2].b', '7'),
         OK, ERR('static path'),
         '[{"a":[{"b":7},{"b":7},{}]}]', '[{"a":[{},{},{}]}]'),

    # ------ a path that matches but creates nothing still updates ------
    # creation must not turn a working multi-target update into an error
    case('{"a":{"a":1}}', ('JSON.SET', KEY, '$..a', '5'),
         OK, OK, '[{"a":5}]', '[{"a":5}]'),
    case('{"p":{"n":1},"q":{"n":2}}', ('JSON.SET', KEY, '$.*.n', '9'),
         OK, OK, '[{"p":{"n":9},"q":{"n":9}}]', '[{"p":{"n":9},"q":{"n":9}}]'),
    case('{"arr":[1,2]}', ('JSON.SET', KEY, '$..[0]', '9'),
         OK, OK, '[{"arr":[9,2]}]', '[{"arr":[9,2]}]'),
    case('{"a":{"a":1}}', ('JSON.MERGE', KEY, '$..a', '{"a":"b"}'),
         OK, OK, '[{"a":{"a":{"a":"b"}}}]', '[{"a":{"a":{"a":"b"}}}]'),

    # ---------------- array indexes are never created ----------------
    case('{"arr":[]}', ('JSON.SET', KEY, '$.arr[0]', '7'),
         ERR('array index out of range'), ERR('array index out of range'),
         '[{"arr":[]}]', '[{"arr":[]}]'),
    case('{"arr":[]}', ('JSON.SET', KEY, '$.a.b[0]', '7'),
         ERR('array index out of range'), ERR('array index out of range')),
    case('{"arr":[]}', ('JSON.SET', KEY, '$..[9]', '7'),
         ERR('static path'), ERR('static path')),

    # ---------------- JSON.MERGE ----------------
    case('{"a":{}}', ('JSON.MERGE', KEY, '$.a.b.c', '5'),
         OK, NIL, '[{"a":{"b":{"c":5}}}]', '[{"a":{}}]'),
    case(None, ('JSON.MERGE', KEY, '$.a.b.c', '5'),
         OK, ERR('new objects must be created at the root'),
         '[{"a":{"b":{"c":5}}}]'),
    case('{"a":{}}', ('JSON.MERGE', KEY, '$.a.b', '5'),
         OK, OK, '[{"a":{"b":5}}]', '[{"a":{"b":5}}]'),
    case('{"a":3}', ('JSON.MERGE', KEY, '$.a.b', '5'), NIL, NIL),
    case(None, ('JSON.MERGE', KEY, '$.a[0].b', '5'),
         ERR('new objects must be created at the root'),
         ERR('new objects must be created at the root')),
    case('{"a":{},"b":{"c":{"x":1}}}', ('JSON.MERGE', KEY, "$['a','b'].c", '{"y":2}'),
         OK, OK,
         '[{"a":{"c":{"y":2}},"b":{"c":{"x":1,"y":2}}}]',
         '[{"a":{},"b":{"c":{"x":1,"y":2}}}]'),
    # A top-level null patch replaces; it only deletes as an object member. So a
    # created path holding null matches merging null into one that already
    # exists -- neither deletes anything.
    case('{"a":{"b":1}}', ('JSON.MERGE', KEY, '$.a', 'null'),
         OK, OK, '[{"a":null}]', '[{"a":null}]'),
    case('{}', ('JSON.MERGE', KEY, '$.a.b', 'null'),
         OK, NIL, '[{"a":{"b":null}}]', '[{}]'),
    case('{"a":{"b":1,"c":2}}', ('JSON.MERGE', KEY, '$.a', '{"b":null}'),
         OK, OK, '[{"a":{"c":2}}]', '[{"a":{"c":2}}]'),

    # ---------------- JSON.MSET ----------------
    case('{"a":{}}', ('JSON.MSET', KEY, '$.a.b.c', '5'),
         OK, NIL, '[{"a":{"b":{"c":5}}}]', '[{"a":{}}]'),
    case('{"a":{}}', ('JSON.MSET', KEY, '$.a.b', '5'),
         OK, OK, '[{"a":{"b":5}}]', '[{"a":{"b":5}}]'),
    case(None, ('JSON.MSET', KEY, '$.a.b.c', '5'),
         OK, ERR('new objects must be created at the root'),
         '[{"a":{"b":{"c":5}}}]'),
    case('{"a":{},"b":{"c":1}}', ('JSON.MSET', KEY, "$['a','b'].c", '9'),
         OK, OK, '[{"a":{"c":9},"b":{"c":9}}]', '[{"a":{},"b":{"c":9}}]'),
    case('{"a":{}}', ('JSON.MSET', KEY, '$..n', '7'),
         OK, ERR('static path'), '[{"a":{"n":7},"n":7}]', '[{"a":{}}]'),
    # A later triplet must see what an earlier one wrote, not the document the
    # command started with -- so the plan is made in the second pass.
    case(None, ('JSON.MSET', KEY, '$.a.b.c', '5', KEY, '$.d.e.f', '6'),
         OK, ERR('new objects must be created at the root'),
         '[{"a":{"b":{"c":5}},"d":{"e":{"f":6}}}]'),
    case('{}', ('JSON.MSET', KEY, '$.a.b', '1', KEY, '$.a.c', '2'),
         OK, NIL, '[{"a":{"b":1,"c":2}}]', '[{}]'),
    case('{}', ('JSON.MSET', KEY, '$.a.b', '1', KEY, '$.a.b', '2'),
         OK, NIL, '[{"a":{"b":2}}]', '[{}]'),
    # Two keys at once, each creating its own chain.
    case('{}', ('JSON.MSET', KEY, '$.a.b', '1', KEY2, '$.c.d', '2'),
         OK, NIL, '[{"a":{"b":1}}]', '[{}]',
         doc2='{}', doc2_on='[{"c":{"d":2}}]', doc2_off='[{}]'),
    # ... and the second key does not have to exist either
    case('{"keep":1}', ('JSON.MSET', KEY, '$.x', '1', KEY2, '$.a.b', '2'),
         OK, ERR('new objects must be created at the root'),
         '[{"keep":1,"x":1}]', '[{"keep":1}]',
         doc2_on='[{"a":{"b":2}}]'),
    case('{"keep":1}', ('JSON.MSET', KEY, '$', '99', KEY2, '$.a[0].b', '5'),
         ERR('new objects must be created at the root'),
         ERR('new objects must be created at the root'),
         '[{"keep":1}]', '[{"keep":1}]'),

    # ---------------- JSON.ARRAPPEND ----------------
    # the created leaf is seeded with `[]`, then the append runs unchanged
    case('{"a":{}}', ('JSON.ARRAPPEND', KEY, '$.a.b', '1'),
         [1], [], '[{"a":{"b":[1]}}]', '[{"a":{}}]'),
    case('{}', ('JSON.ARRAPPEND', KEY, '$.a.b.c', '1', '2'),
         [2], [], '[{"a":{"b":{"c":[1,2]}}}]', '[{}]'),
    # legacy paths reply with the new length, and error on no match
    case('{"a":{}}', ('JSON.ARRAPPEND', KEY, '.a.b', '1'),
         1, ERR('Path does not exist or not an array'),
         '[{"a":{"b":[1]}}]', '[{"a":{}}]'),
    # an existing array is appended to, never reseeded
    case('{"a":{"b":[1]}}', ('JSON.ARRAPPEND', KEY, '$.a.b', '2'),
         [2], [2], '[{"a":{"b":[1,2]}}]', '[{"a":{"b":[1,2]}}]'),
    # a match of the wrong type keeps its own per-match null -- the seed only
    # ever lands where the path resolved to nothing
    case('{"a":"str"}', ('JSON.ARRAPPEND', KEY, '$.a', '1'),
         [None], [None], '[{"a":"str"}]', '[{"a":"str"}]'),
    # an absent key stays an error: only SET/MSET/MERGE create a document
    case(None, ('JSON.ARRAPPEND', KEY, '$.a.b', '1'),
         ERR("key that doesn't exist"), ERR("key that doesn't exist")),
    case('{"p":{},"q":{}}', ('JSON.ARRAPPEND', KEY, '$.*.n', '9'),
         [1, 1], [], '[{"p":{"n":[9]},"q":{"n":[9]}}]', '[{"p":{},"q":{}}]'),

    # ---------------- JSON.ARRINSERT ----------------
    case('{}', ('JSON.ARRINSERT', KEY, '$.a.b', '0', '1'),
         [1], [], '[{"a":{"b":[1]}}]', '[{}]'),
    # 0 is the only index an empty array can satisfy, so any other one creates
    # nothing at all rather than failing with a stray `[]` left behind
    case('{}', ('JSON.ARRINSERT', KEY, '$.a.b', '1', '1'),
         [], [], '[{}]', '[{}]'),
    # an existing array keeps every index it already supports
    case('{"a":{"b":[1,2]}}', ('JSON.ARRINSERT', KEY, '$.a.b', '1', '9'),
         [3], [3], '[{"a":{"b":[1,9,2]}}]', '[{"a":{"b":[1,9,2]}}]'),
    case('{"a":{}}', ('JSON.ARRINSERT', KEY, '.a.b', '0', '1'),
         1, ERR('Path does not exist or not an array'),
         '[{"a":{"b":[1]}}]', '[{"a":{}}]'),
    case(None, ('JSON.ARRINSERT', KEY, '$.a.b', '0', '1'),
         ERR("key that doesn't exist"), ERR("key that doesn't exist")),
    # a projection stays read-only for these too
    case('{"a":1}', ('JSON.ARRAPPEND', KEY, '$.a + 1', '1'),
         ERR('computed/projection expressions'),
         ERR('computed/projection expressions')),

    # ---------------- JSON.NUMINCRBY ----------------
    # `0` is the seed, so a created leaf answers with the increment itself.
    # Note RESP2 replies to a JSONPath here with a JSON *string*, not an array.
    case('{"a":{}}', ('JSON.NUMINCRBY', KEY, '$.a.b', '5'),
         '[5]', '[]', '[{"a":{"b":5}}]', '[{"a":{}}]'),
    case('{}', ('JSON.NUMINCRBY', KEY, '$.a.b.c', '2.5'),
         '[2.5]', '[]', '[{"a":{"b":{"c":2.5}}}]', '[{}]'),
    # legacy NUMINCRBY replies with the number as a string
    case('{"a":{}}', ('JSON.NUMINCRBY', KEY, '.a.b', '5'),
         '5', ERR('does not contains a number'),
         '[{"a":{"b":5}}]', '[{"a":{}}]'),
    case('{"a":"str"}', ('JSON.NUMINCRBY', KEY, '$.a', '5'),
         '[null]', '[null]', '[{"a":"str"}]', '[{"a":"str"}]'),
    case(None, ('JSON.NUMINCRBY', KEY, '$.a.b', '5'),
         ERR("key that doesn't exist"), ERR("key that doesn't exist")),
    # MULTBY and POWBY are excluded: `0 * n` and `0 ^ n` would fabricate an
    # answer, so they have no seed and create nothing in either mode.
    case('{}', ('JSON.NUMMULTBY', KEY, '$.a.b', '5'),
         '[]', '[]', '[{}]', '[{}]'),
    case('{}', ('JSON.NUMPOWBY', KEY, '$.a.b', '5'),
         '[]', '[]', '[{}]', '[{}]'),

    # ---------------- JSON.STRAPPEND ----------------
    case('{"a":{}}', ('JSON.STRAPPEND', KEY, '$.a.b', '"hi"'),
         [2], [], '[{"a":{"b":"hi"}}]', '[{"a":{}}]'),
    # an existing string is appended to, not reseeded
    case('{"s":"x"}', ('JSON.STRAPPEND', KEY, '$.s', '"y"'),
         [2], [2], '[{"s":"xy"}]', '[{"s":"xy"}]'),
    case('{"a":{}}', ('JSON.STRAPPEND', KEY, '.a.b', '"hi"'),
         2, ERR('not a string'), '[{"a":{"b":"hi"}}]', '[{"a":{}}]'),
    case(None, ('JSON.STRAPPEND', KEY, '$.a.b', '"hi"'),
         ERR("key that doesn't exist"), ERR("key that doesn't exist")),

    # ---------------- legacy and projection paths ----------------
    # legacy (dot) paths create too
    case('{"a":{}}', ('JSON.SET', KEY, '.a.b.c', '5'),
         OK, NIL, '[{"a":{"b":{"c":5}}}]', '[{"a":{}}]'),
    # a computed expression is read-only in both modes
    case('{"a":1}', ('JSON.SET', KEY, '$.a + 1', '5'),
         ERR('computed/projection expressions'),
         ERR('computed/projection expressions')),

    # ------ value formats survive creation (the leaf is never re-serialized) --
    case('{}', ('JSON.SET', KEY, '$.a.b', '{"x":[1,2],"y":null}'),
         OK, NIL, '[{"a":{"b":{"x":[1,2],"y":null}}}]', '[{}]'),
    case('{}', ('JSON.SET', KEY, '$.a.b', '[1.5,2.5,3.5]', 'FPHA', 'FP32'),
         OK, NIL, '[{"a":{"b":[1.5,2.5,3.5]}}]', '[{}]'),

    # ---------------- MAX_DEPTH, nothing half-created ----------------
    case('{"keep":1}', ('JSON.SET', KEY, DEEP, '5'),
         ERR('recursion limit exceeded'), NIL, '[{"keep":1}]', '[{"keep":1}]'),
    case('{"keep":1}', ('JSON.MERGE', KEY, DEEP, '5'),
         ERR('recursion limit exceeded'), NIL, '[{"keep":1}]', '[{"keep":1}]'),
    case(None, ('JSON.SET', KEY, DEEP, '5'),
         ERR('recursion limit exceeded'),
         ERR('new objects must be created at the root')),
    case(None, ('JSON.MSET', KEY, DEEP, '5'),
         ERR('recursion limit exceeded'),
         ERR('new objects must be created at the root')),
    # JSON.MSET plans in its second pass, once earlier triplets have been
    # written, so it cannot raise this as an error without applying some of
    # them. It reports the same way it reports any path it could not write:
    # nil, exactly as `apply_updates` already does for an over-deep write.
    case('{"keep":1}', ('JSON.MSET', KEY, DEEP, '5'),
         NIL, NIL, '[{"keep":1}]', '[{"keep":1}]'),
]


def test_all_cases_with_auto_create_enabled():
    env = _env(True)
    env.expect('CONFIG', 'GET', 'json-auto-create-deep-paths').equal(
        ['json-auto-create-deep-paths', 'yes'])
    # load-time only
    env.expect('CONFIG', 'SET', 'json-auto-create-deep-paths', 'no').raiseError()
    for c in CASES:
        _check(env, c, c.on, c.doc_on, c.doc2_on, 'ON')


def test_all_cases_with_auto_create_disabled():
    """The disabled column is the regression guard for today's behavior."""
    env = _env(False)
    env.expect('CONFIG', 'GET', 'json-auto-create-deep-paths').equal(
        ['json-auto-create-deep-paths', 'no'])
    for c in CASES:
        _check(env, c, c.off, c.doc_off, c.doc2_off, 'OFF')


def test_numincrby_creates_under_resp3():
    """The seed lands above the RESP2/RESP3 split in `json_num_op`, so the two
    protocols differ only in the shape of the reply, never in what was created.
    """
    env = Env(protocol=3, moduleArgs=AUTO_CREATE_ARGS, noDefaultModuleArgs=True)
    env.expect('JSON.SET', KEY, '$', '{"a":{}}').ok()
    env.expect('JSON.NUMINCRBY', KEY, '$.a.b', '5').equal([5])
    env.expect('JSON.GET', KEY, '$').equal('[{"a":{"b":5}}]')


def test_created_document_survives_rdb_reload():
    env = _env(True)
    env.skipOnCluster()
    if env.useAof:
        env.skip()
    env.expect('JSON.SET', KEY, '$.a.b.c', '5').ok()
    # A seeded leaf is a normal value once created, so it has to survive too.
    env.expect('JSON.ARRAPPEND', KEY, '$.a.b.arr', '1').equal([1])
    for _ in env.retry_with_rdb_reload():
        env.assertExists(KEY)
        env.expect('JSON.GET', KEY, '$').equal('[{"a":{"b":{"c":5,"arr":[1]}}}]')


def test_created_paths_replicate():
    """`apply_changes` replicates verbatim, so the replica re-runs the command
    and has to auto-create the structure itself. That converges only because
    both nodes were loaded with the config on -- a mismatched pair is
    divergent by design (R5) and is deliberately not asserted.
    """
    env = _env(True)
    if not env.useSlaves:
        env.skip()
    env.skipOnCluster()

    # An absent key, built entirely from the path, plus a seeded leaf under it.
    env.expect('JSON.SET', KEY, '$.a.b.c', '5').ok()
    env.expect('JSON.ARRAPPEND', KEY, '$.a.b.arr', '1').equal([1])

    env.cmd('WAIT', '1', '10000')
    replica = env.getSlaveConnection()
    env.assertEqual(replica.execute_command('JSON.GET', KEY, '$'),
                    '[{"a":{"b":{"c":5,"arr":[1]}}}]')
