# RED-213286: BLESS (Redis 8.12) marks a key NO-EVICT so maxmemory eviction never
# picks it. The flag lives outside the module's value, so every module path that
# writes, copies, saves or reloads a document has to keep it. RED-220725 was such a
# path in RedisBloom: replacing a value through RM_ModuleTypeSetValue dropped the
# flag. RedisJSON writes in place; this guards that every write keeps it.

import time
from common import *
from redis import ResponseError
from test_replication import ROWS

DOC = '{"a":1,"arr":[1,2,3]}'


def _skip_without_bless(env):
    env.skipOnCluster()  # BLESS state and INFO blessed_keys are per shard
    try:
        env.cmd('BLESS', 'GET', 'missing')
    except ResponseError as e:
        if 'unknown command' in str(e):
            env.skip()


def _assert_blessed(env, keys, when):
    for key in keys:
        env.assertEqual(env.cmd('BLESS', 'GET', key), ['NO-EVICT'], message=f'{when}: {key}')
    env.assertEqual(env.cmd('BLESS', 'GET', 'plain'), [], message=f'{when}: plain')
    env.assertEqual(env.cmd('INFO', 'stats')['blessed_keys'], len(keys), message=when)


def _wait_aof_rewrite(env, timeout=10):
    deadline = time.time() + timeout
    while time.time() < deadline:
        info = env.cmd('INFO', 'persistence')
        if not info['aof_rewrite_in_progress'] and not info['aof_rewrite_scheduled']:
            env.assertEqual(info['aof_last_bgrewrite_status'], 'ok')
            return
        time.sleep(0.1)
    env.assertTrue(False, message='AOF rewrite did not finish')


def testWriteCommandsKeepNoEvict(env):
    """Bless every key a write command touches, then run the command on them again."""
    _skip_without_bless(env)
    con = env.getConnection()
    for cmd, setup, write, _ in ROWS:
        con.execute_command('FLUSHALL')
        for spec in (setup or []):
            con.execute_command(*spec)
        con.execute_command(*write)  # creates the keys when the setup didn't
        keys = con.execute_command('KEYS', '*')
        for key in keys:
            con.execute_command('BLESS', 'SET', key, 'NO-EVICT')
        con.execute_command(*write)
        for key in keys:
            env.assertEqual(con.execute_command('BLESS', 'GET', key), ['NO-EVICT'], message=f'{cmd}: {key}')
        env.assertEqual(con.execute_command('INFO', 'stats')['blessed_keys'], len(keys), message=cmd)


def testNoEvictSurvivesCopyRdbAndAof(env):
    _skip_without_bless(env)
    env.cmd('JSON.SET', 'doc', '$', DOC)
    env.cmd('BLESS', 'SET', 'doc', 'NO-EVICT')
    env.cmd('JSON.SET', 'plain', '$', DOC)  # never blessed

    # COPY gives the destination the source's flag, the same way it gives it the TTL.
    env.cmd('COPY', 'doc', 'copy')
    _assert_blessed(env, ['doc', 'copy'], 'COPY')

    env.cmd('DEBUG', 'RELOAD')
    _assert_blessed(env, ['doc', 'copy'], 'DEBUG RELOAD')

    # RedisJSON has no aof_rewrite callback, so the rewrite keeps its RDB preamble. A key
    # blessed after the rewrite is replayed from the incremental file instead.
    env.cmd('CONFIG', 'SET', 'appendonly', 'yes')
    _wait_aof_rewrite(env)
    env.cmd('JSON.SET', 'late', '$', DOC)
    env.cmd('BLESS', 'SET', 'late', 'NO-EVICT')
    env.cmd('JSON.ARRAPPEND', 'late', '$.arr', '4')
    env.cmd('DEBUG', 'LOADAOF')
    _assert_blessed(env, ['doc', 'copy', 'late'], 'AOF')
    env.assertEqual(env.cmd('JSON.GET', 'late', '$.arr'), '[[1,2,3,4]]')


def testBlessedKeySurvivesEviction(env):
    _skip_without_bless(env)
    env.cmd('CONFIG', 'SET', 'maxmemory-policy', 'allkeys-lru')
    env.cmd('JSON.SET', 'doc', '$', DOC)
    env.cmd('BLESS', 'SET', 'doc', 'NO-EVICT')

    # Under --use-slaves the 1MB replication backlog counts toward maxmemory, so the
    # headroom has to exceed it or eviction runs out of unblessed keys and fails with OOM.
    env.cmd('CONFIG', 'SET', 'maxmemory', env.cmd('INFO', 'memory')['used_memory'] + 4000000)
    with env.getConnection().pipeline(transaction=False) as pipe:
        for i in range(4000):
            pipe.set(f'flood:{i}', 'x' * 2000)
        pipe.execute()
    env.cmd('CONFIG', 'SET', 'maxmemory', 0)

    env.assertGreater(env.cmd('INFO', 'stats')['evicted_keys'], 0)
    env.assertEqual(env.cmd('EXISTS', 'doc'), 1)
