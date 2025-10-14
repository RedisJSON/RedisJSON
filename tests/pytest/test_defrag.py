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
    json_str = json.dumps(obj)
    env.expect('JSON.SET', 'test', '$', json_str).ok()
    for i in range(10000):
        env.expect('JSON.SET', 'test%d' % i, '$', json_str).ok()
    i += 1
    env.expect('JSON.SET', 'test%d' % i, '$', json_str).ok()
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

def testDefragBigJsons(env):
    enableDefrag(env)

    # Disable defrag so we can actually create fragmentation
    env.cmd('CONFIG', 'SET', 'activedefrag', 'no')
    
    env.expect('JSON.SET', 'key1', '$', "[]").ok()
    env.expect('JSON.SET', 'key2', '$', "[]").ok()

    for i in range(100000):
        env.cmd('JSON.ARRAPPEND', 'key1', '$', "[1.11111111111]")
        env.cmd('JSON.ARRAPPEND', 'key2', '$', "[1.11111111111]")

    # Now we delete key2 which should cause fragmenation
    env.expect('DEL', 'key2').equal(1)

    # wait for fragmentation for up to 30 seconds
    frag = env.cmd('info', 'memory')['allocator_frag_ratio']
    startTime = time.time()
    while frag < 1.4:
        time.sleep(0.1)
        frag = env.cmd('info', 'memory')['allocator_frag_ratio']
        if time.time() - startTime > 30:
            # We will wait for up to 30 seconds and then we consider it a failure
            env.assertTrue(False, message='Failed waiting for fragmentation, current value %s which is expected to be above 1.4.' % frag)
            return

    #enable active defrag
    env.cmd('CONFIG', 'SET', 'activedefrag', 'yes')

    # wait for fragmentation for go down for up to 30 seconds
    frag = env.cmd('info', 'memory')['allocator_frag_ratio']
    startTime = time.time()
    while frag > 1.125:
        time.sleep(0.1)
        frag = env.cmd('info', 'memory')['allocator_frag_ratio']
        if time.time() - startTime > 30:
            # We will wait for up to 30 seconds and then we consider it a failure
            env.assertTrue(False, message='Failed waiting for fragmentation to go down, current value %s which is expected to be bellow 1.125.' % frag)
            return

def testDefragWithSharedStrings(env):
    """
    Test for RED-171586: Defrag crash with shared strings.
    
    This test reproduces the scenario where:
    1. Many JSON documents share common strings
    2. Defrag starts and tries to clear the shared string cache
    3. Documents still reference the cached strings
    4. Concurrent operations trigger use-after-free
    
    Without the fix, this would crash Redis with SIGABRT.
    With the fix, Redis continues operating normally.
    """
    enableDefrag(env)
    
    # Create many documents with shared strings (simulates production workload)
    shared_strings = ['active', 'pending', 'completed', 'failed']
    
    for i in range(5000):
        doc = {
            'id': f'doc-{i}',
            'status': shared_strings[i % len(shared_strings)],  # Shared strings
            'type': 'document',  # Another shared string
            'priority': 'high' if i % 2 == 0 else 'low',  # More shared strings
        }
        env.expect('JSON.SET', f'shared:key:{i}', '$', json.dumps(doc)).ok()
    
    # Get initial defrag stats
    _, _, _, _, _, keysDefragStart = env.cmd('JSON.DEBUG', 'DEFRAG_INFO')
    
    # Wait for defrag to start (this is when the bug would trigger)
    startTime = time.time()
    defragStarted = False
    while time.time() - startTime < 30:
        _, _, _, _, _, keysDefrag = env.cmd('JSON.DEBUG', 'DEFRAG_INFO')
        if keysDefrag > keysDefragStart:
            defragStarted = True
            break
        time.sleep(0.1)
    
    env.assertTrue(defragStarted, message='Defrag did not start within 30 seconds')
    
    # Continue JSON operations during defrag (this would crash with the bug)
    for i in range(100):
        doc = {
            'test': f'data-{i}',
            'status': 'active',  # Using shared string
        }
        env.expect('JSON.SET', f'test:key:{i}', '$', json.dumps(doc)).ok()
    
    # Verify data integrity - read some documents with shared strings
    res = json.loads(env.cmd('JSON.GET', 'shared:key:0', '$'))[0]
    env.assertEqual(res['status'], 'active')
    env.assertEqual(res['type'], 'document')
    
    res = json.loads(env.cmd('JSON.GET', 'shared:key:100', '$'))[0]
    env.assertEqual(res['status'], 'active')
    
    # If we got here without crashing, the fix is working!
    env.assertGreater(env.cmd('info', 'Stats')['active_defrag_key_hits'], 0)

def testAggressiveDefragSharedStrings(env):
    """RED-171586: Aggressive test to maximize crash likelihood with shared strings"""
    enableDefrag(env)
    
    shared_strings = ['active', 'pending', 'completed', 'failed', 'processing']
    
    # Create many documents with shared strings
    for i in range(10000):
        doc = {
            'id': f'doc-{i}',
            'status': shared_strings[i % len(shared_strings)],
            'type': 'document',
            'priority': 'high' if i % 2 == 0 else 'low',
        }
        env.expect('JSON.SET', f'key:{i}', '$', json.dumps(doc)).ok()
    
    # Delete many keys to fragment memory and trigger defrag
    for i in range(5000):
        env.expect('DEL', f'key:{i}').equal(1)
    
    # Aggressively trigger defrag while doing JSON.SET
    for iteration in range(50):
        for j in range(100):
            key_id = 10000 + (iteration * 100) + j
            doc = {
                'iteration': iteration,
                'id': f'new-doc-{key_id}',
                'status': shared_strings[j % len(shared_strings)],
                'type': 'document',
            }
            env.expect('JSON.SET', f'newkey:{key_id}', '$', json.dumps(doc)).ok()
        
        time.sleep(0.01)
    
    # Verify data integrity
    res = json.loads(env.cmd('JSON.GET', 'newkey:10000', '$'))[0]
    env.assertEqual(res['status'], shared_strings[0])
