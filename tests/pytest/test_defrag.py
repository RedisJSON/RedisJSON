import time
import json
from RLTest import Defaults

Defaults.decode_responses = True

def enableDefrag(env):
    # make defrag as aggressive as possible
    env.cmd('CONFIG', 'SET', 'hz', '100')
    env.cmd('CONFIG', 'SET', 'active-defrag-ignore-bytes', '1')
    env.cmd('CONFIG', 'SET', 'active-defrag-threshold-lower', '0')
    env.cmd('CONFIG', 'SET', 'active-defrag-cycle-min', '99')

    try:
        env.cmd('CONFIG', 'SET', 'activedefrag', 'yes')
    except Exception:
        # If active defrag is not supported by the current Redis, simply skip the test.
        env.skip()

def defragOnObj(env, obj):
    enableDefrag(env)
    env.expect('JSON.SET', 'test', '$', json.dumps(obj)).ok()
    for i in range(10000):
        env.expect('JSON.SET', 'test%d' % i, '$', json.dumps(obj)).ok()
    i += 1
    env.expect('JSON.SET', 'test%d' % i, '$', json.dumps(obj)).ok()
    for i in range(10000):
        env.expect('DEL', 'test%d' % i).equal(1)
    i += 1
    _, _, _, _, _, keysDefrag = env.cmd('JSON.DEBUG', 'DEFRAG_INFO')
    startTime = time.time()
    # Wait for at least 2 defrag full cycles
    # We verify only the 'keysDefrag' value because the other values
    # are not promised to be updated. It depends if Redis support
    # the start/end defrag callbacks.
    while keysDefrag < 2:
        time.sleep(0.1)
        _, _, _, _, _, keysDefrag = env.cmd('JSON.DEBUG', 'DEFRAG_INFO')
        if time.time() - startTime > 30:
            # We will wait for up to 30 seconds and then we consider it a failure
            env.assertTrue(False, message='Failed waiting for defrag to run')
            return
    # make sure json is still valid.
    res = json.loads(env.cmd('JSON.GET', 'test%d' % i, '$'))[0]
    env.assertEqual(res, obj)
    env.assertGreater(env.cmd('info', 'Stats')['active_defrag_key_hits'], 0)

def testDefragNumber(env):
    defragOnObj(env, 1)

def testDefragBigNumber(env):
    defragOnObj(env, 100000000000000000000)
    
def testDefragDouble(env):
    defragOnObj(env, 1.111111111111)

def testDefragNegativeNumber(env):
    defragOnObj(env, -100000000000000000000)

def testDefragNegativeDouble(env):
    defragOnObj(env, -1.111111111111)

def testDefragTrue(env):
    defragOnObj(env, True)

def testDefragFalse(env):
    defragOnObj(env, True)

def testDefragNone(env):
    defragOnObj(env, None)

def testDefragEmptyString(env):
    defragOnObj(env, "")

def testDefragString(env):
    defragOnObj(env, "foo")

def testDefragEmptyArray(env):
    defragOnObj(env, [])

def testDefragArray(env):
    defragOnObj(env, [1, 2, 3])

def testDefragEmptyObject(env):
    defragOnObj(env, {})

def testDefragObject(env):
    defragOnObj(env, {"foo": "bar"})

def testDefragComplex(env):
    defragOnObj(env, {"foo": ["foo", 1, None, True, False, {}, {"foo": [], "bar": 1}]})
