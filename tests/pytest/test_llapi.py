# Flow tests for the RedisJSON shared C API (LLAPI).
#
# The LLAPI is a pointer-based shared API (RedisModule_GetSharedAPI) and is not
# reachable from a Redis client. These tests drive it through a tiny C consumer
# module (tests/pytest/llapi_test_module) that exposes each RedisJSONAPI function
# as an `LLAPI.*` command.
#
import os
import json
import unittest
from RLTest import Env, Defaults
from includes import *

Defaults.decode_responses = True

HERE = os.path.abspath(os.path.dirname(__file__))
DOC_FILE = os.path.join(HERE, '..', 'files', 'llapi_doc.json')

with open(DOC_FILE) as _f:
    DOC = _f.read()

LLAPI_MODULE = os.environ.get('LLAPI_TEST_MODULE')


def _module_under_test():
    m = Defaults.module
    if not m:
        return []
    return list(m) if isinstance(m, (list, tuple)) else [m]


def _new_env():
    """A fresh Env loading the JSON module under test plus the LLAPI consumer."""
    if not LLAPI_MODULE or not os.path.exists(LLAPI_MODULE):
        # Only tests.sh builds and exports the consumer .so (and it hard-fails the
        # run if that build breaks). A direct `pytest`/IDE run leaves the env var
        # unset, so skip cleanly there instead of erroring the whole file.
        raise unittest.SkipTest(
            f'LLAPI test module not built/available (LLAPI_TEST_MODULE={LLAPI_MODULE!r}); '
            'it is built by tests/pytest/tests.sh')
    modules = _module_under_test() + [LLAPI_MODULE]
    # noDefaultModuleArgs: the global --module-args default is for a single module;
    # skip merging it so our two-module list isn't rejected as a count mismatch.
    return Env(module=modules, noDefaultModuleArgs=True)


def _env_with_doc():
    env = _new_env()
    env.expect('JSON.SET', 'doc', '$', DOC).ok()
    return env


# --------------------------------------------------------------------------- #
# Happy path
# --------------------------------------------------------------------------- #

def testLLAPIVersion():
    env = _new_env()
    ver = env.cmd('LLAPI.VERSION')
    # RedisJSON exports up to V7; other modules may export a lower version.
    env.assertTrue(isinstance(ver, int) and ver >= 1)


def testLLAPIType():
    env = _env_with_doc()
    cases = {
        '$.string': 'string',
        '$.int': 'int',
        '$.double': 'double',
        '$.bool_true': 'bool',
        '$.bool_false': 'bool',
        '$.null_val': 'null',
        '$.object': 'object',
        '$.het_array': 'array',
        '$': 'object',
    }
    for path, expected in cases.items():
        env.assertEqual(env.cmd('LLAPI.TYPE', 'doc', path), expected, message=path)


def testLLAPIScalar():
    env = _env_with_doc()
    env.assertEqual(env.cmd('LLAPI.SCALAR', 'doc', '$.int'), 42)
    env.assertEqual(float(env.cmd('LLAPI.SCALAR', 'doc', '$.double')), 4.2)
    env.assertEqual(env.cmd('LLAPI.SCALAR', 'doc', '$.bool_true'), 1)
    env.assertEqual(env.cmd('LLAPI.SCALAR', 'doc', '$.bool_false'), 0)
    env.assertEqual(env.cmd('LLAPI.SCALAR', 'doc', '$.string'), 'hello world')
    env.assertEqual(env.cmd('LLAPI.SCALAR', 'doc', '$.null_val'), 'null')


def testLLAPIGetLen():
    env = _env_with_doc()
    env.assertEqual(env.cmd('LLAPI.GETLEN', 'doc', '$.string'), len('hello world'))
    env.assertEqual(env.cmd('LLAPI.GETLEN', 'doc', '$.het_array'), 7)
    env.assertEqual(env.cmd('LLAPI.GETLEN', 'doc', '$.num_array'), 4)
    env.assertEqual(env.cmd('LLAPI.GETLEN', 'doc', '$.object'), 3)
    env.assertEqual(env.cmd('LLAPI.GETLEN', 'doc', '$.empty_array'), 0)


def testLLAPIGetJson():
    env = _env_with_doc()
    res = env.cmd('LLAPI.GETJSON', 'doc', '$.object')
    env.assertEqual(json.loads(res), {'a': 1, 'b': 'two', 'c': None})
    env.assertEqual(json.loads(env.cmd('LLAPI.GETJSON', 'doc', '$.int')), 42)


def testLLAPIOpenGetAndIter():
    env = _env_with_doc()
    # OPEN_GET returns the JSON of every matched node.
    res = env.cmd('LLAPI.OPEN_GET', 'doc', '$.num_array[*]')
    env.assertEqual([json.loads(x) for x in res], [10, 20, 30, 40])
    # ITER_LEN reports the match count, ITER_JSON serializes the whole result set.
    env.assertEqual(env.cmd('LLAPI.ITER_LEN', 'doc', '$.num_array[*]'), 4)
    env.assertEqual(json.loads(env.cmd('LLAPI.ITER_JSON', 'doc', '$.num_array[*]')),
                    [10, 20, 30, 40])
    # A non-matching (but valid) path yields zero results, not an error.
    env.assertEqual(env.cmd('LLAPI.ITER_LEN', 'doc', '$.does_not_exist'), 0)
    # ITER_JSON deliberately differs: whole-set serialization (getJSONFromIter)
    # returns Status::Err for an empty result set, unlike len()/next().
    env.expect('LLAPI.ITER_JSON', 'doc', '$.does_not_exist').raiseError()


def testLLAPIReset():
    env = _env_with_doc()
    # Iterate fully, reset, iterate again: both passes see all matches.
    env.assertEqual(env.cmd('LLAPI.RESET', 'doc', '$.num_array[*]'), [4, 4])


def testLLAPIOpenFromStrAndFlags():
    env = _env_with_doc()
    env.assertEqual(env.cmd('LLAPI.OPENFROMSTR', 'doc', '$.int'), 1)
    env.assertEqual(env.cmd('LLAPI.OPENFLAGS', 'doc', '$.int'), 1)


def testLLAPIGetAt():
    env = _env_with_doc()
    env.assertEqual(json.loads(env.cmd('LLAPI.GETAT', 'doc', '$.het_array', '0')), 1)
    env.assertEqual(json.loads(env.cmd('LLAPI.GETAT', 'doc', '$.het_array', '1')), 'two')
    env.assertEqual(json.loads(env.cmd('LLAPI.GETAT', 'doc', '$.num_array', '2')), 30)


def testLLAPIGetArray():
    env = _env_with_doc()
    # Homogeneous numeric array: RedisJSON packs it into a typed buffer.
    het_type, n = env.cmd('LLAPI.GETARRAY', 'doc', '$.num_array')
    env.assertEqual(n, 4)
    env.assertNotEqual(het_type, 'heterogeneous')
    # Mixed array stays heterogeneous.
    env.assertEqual(env.cmd('LLAPI.GETARRAY', 'doc', '$.het_array'), ['heterogeneous', 7])
    env.assertEqual(env.cmd('LLAPI.GETARRAY', 'doc', '$.empty_array'), ['heterogeneous', 0])


def testLLAPIKeyValues():
    env = _env_with_doc()
    flat = env.cmd('LLAPI.KEYVALUES', 'doc', '$.object')
    pairs = {flat[i]: json.loads(flat[i + 1]) for i in range(0, len(flat), 2)}
    env.assertEqual(pairs, {'a': 1, 'b': 'two', 'c': None})


def testLLAPIPathParse():
    env = _env_with_doc()
    # Static single path: single + defined order.
    env.assertEqual(env.cmd('LLAPI.PATHPARSE', '$.int'), [1, 1])
    # Wildcard path: not single.
    single, _order = env.cmd('LLAPI.PATHPARSE', '$.num_array[*]')
    env.assertEqual(single, 0)


def testLLAPIIsJson():
    env = _env_with_doc()
    env.cmd('SET', 'plain', 'notjson')
    env.assertEqual(env.cmd('LLAPI.ISJSON', 'doc'), 1)
    env.assertEqual(env.cmd('LLAPI.ISJSON', 'plain'), 0)


# --------------------------------------------------------------------------- #
# Negative / error paths
# --------------------------------------------------------------------------- #

def testLLAPIErrorsMissingOrWrongKey():
    env = _env_with_doc()
    env.cmd('SET', 'plain', 'notjson')
    # Missing key / non-JSON key cannot be opened.
    env.expect('LLAPI.TYPE', 'missing', '$').raiseError()
    env.expect('LLAPI.TYPE', 'plain', '$').raiseError()
    env.expect('LLAPI.OPENFROMSTR', 'missing', '$').raiseError()


def testLLAPIErrorsTypeMismatch():
    env = _env_with_doc()
    # getLen only works on string/array/object.
    env.expect('LLAPI.GETLEN', 'doc', '$.int').raiseError()
    # getArray only works on arrays.
    env.expect('LLAPI.GETARRAY', 'doc', '$.int').raiseError()
    # getKeyValues only works on objects.
    env.expect('LLAPI.KEYVALUES', 'doc', '$.num_array').raiseError()


def testLLAPIErrorsGetAt():
    env = _env_with_doc()
    # Index out of range.
    env.expect('LLAPI.GETAT', 'doc', '$.num_array', '99').raiseError()
    # Not an array.
    env.expect('LLAPI.GETAT', 'doc', '$.int', '0').raiseError()


def testLLAPIErrorsBadPath():
    env = _env_with_doc()
    # Malformed path fails to compile.
    env.expect('LLAPI.ITER_LEN', 'doc', '$[').raiseError()
    env.expect('LLAPI.PATHPARSE', '$[').raiseError()
    # Projection / computed paths are not exposed by the LLAPI.
    env.expect('LLAPI.OPEN_GET', 'doc', '$.int + 1').raiseError()
    env.expect('LLAPI.PATHPARSE', '$.int + 1').raiseError()
