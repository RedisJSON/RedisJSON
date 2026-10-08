# MOD-18948: per-write-command primary -> replica data-integrity verification.
#
# For every write command the module registers, run one representative
# invocation on the primary and assert the replica ends up holding the same
# document. Catches commands that are implemented correctly on the primary
# while their propagation to the replica is wrong or missing -- a divergence
# that stays silent (link up, offsets matching) until a failover promotes the
# replica. MOD-16050 is the canonical case, in RedisBloom.

import time
from common import *
from redis import ResponseError

MODULE_NAME = 'ReJSON'
WAIT_TIMEOUT_MS = 1000

# Every row that mutates an existing document starts from this one.
BASE = [['JSON.SET', 'k', '$', '{"a":2,"arr":[1,2,3],"s":"x","b":true,"o":{"n":1}}']]

# The document read back in full is the fingerprint: unlike a probabilistic
# sketch, JSON has no internal state that JSON.GET '$' does not show. (DUMP is
# not usable here -- this suite sets Defaults.decode_responses, so a binary RDB
# payload cannot come back over these connections.)
READ_K = [['JSON.GET', 'k', '$'], ['JSON.TYPE', 'k', '$']]

# (command, setup, write-under-test, reads compared on both sides)
#
# One invocation per command, no argument permutations -- but the invocation is
# the form most likely to diverge: a mid-array index over a tail append, a
# null-valued merge (which deletes) over a plain overwrite, a create-on-write
# over an overwrite of an existing key.
ROWS = [
    ('json.set', None, ['JSON.SET', 'k', '$', '{"a":1,"arr":[1,2,3]}'], READ_K),
    # RFC 7386: a null value deletes the member, a new one is added.
    ('json.merge', BASE, ['JSON.MERGE', 'k', '$', '{"a":null,"new":5}'], READ_K),
    ('json.mset', None,
     ['JSON.MSET', 'k1', '$', '{"v":1}', 'k2', '$', '{"v":2}'],
     [['JSON.GET', 'k1', '$'], ['JSON.GET', 'k2', '$']]),
    # Deleting a middle element shifts the ones after it.
    ('json.del', BASE, ['JSON.DEL', 'k', '$.arr[1]'], READ_K),
    ('json.forget', BASE, ['JSON.FORGET', 'k', '$.s'], READ_K),
    # A float delta on an integer promotes the stored number's type.
    ('json.numincrby', BASE, ['JSON.NUMINCRBY', 'k', '$.a', '1.5'], READ_K),
    ('json.nummultby', BASE, ['JSON.NUMMULTBY', 'k', '$.a', '3'], READ_K),
    ('json.numpowby', BASE, ['JSON.NUMPOWBY', 'k', '$.a', '2'], READ_K),
    ('json.toggle', BASE, ['JSON.TOGGLE', 'k', '$.b'], READ_K),
    ('json.strappend', BASE, ['JSON.STRAPPEND', 'k', '$.s', '"yz"'], READ_K),
    ('json.arrappend', BASE, ['JSON.ARRAPPEND', 'k', '$.arr', '4', '5'], READ_K),
    ('json.arrinsert', BASE, ['JSON.ARRINSERT', 'k', '$.arr', '1', '9'], READ_K),
    ('json.arrpop', BASE, ['JSON.ARRPOP', 'k', '$.arr', '1'], READ_K),
    ('json.arrtrim', BASE, ['JSON.ARRTRIM', 'k', '$.arr', '1', '2'], READ_K),
    ('json.clear', BASE, ['JSON.CLEAR', 'k', '$.arr'], READ_K),
]

# Write commands deliberately not in ROWS, with the reason. Keep this empty.
KNOWN_EXCLUSIONS = set()


def _wait_link_up(env, timeout=10):
    """RLTest starts the replica with --slaveof but never waits for the sync."""
    slave = env.getSlaveConnection()
    deadline = time.time() + timeout
    while time.time() < deadline:
        if slave.execute_command('INFO', 'replication')['master_link_status'] == 'up':
            return
        time.sleep(0.1)
    env.assertTrue(False, message='replica link never came up')


def _is_trivial(val):
    """A read that returns the type default proves nothing -- both sides match."""
    if val is None or val == 0 or val in ('', '[]', b'', b'[]'):
        return True
    if isinstance(val, (list, tuple)):
        return len(val) == 0 or all(_is_trivial(v) for v in val)
    return False


def _read_slave(con, spec):
    """A divergence must be reported as a diff, not raised -- keep rows independent."""
    try:
        return con.execute_command(*spec)
    except ResponseError as e:
        return f'error: {e}'


def _keys(con):
    return sorted(con.execute_command('KEYS', '*'))


def _verify_row(env, master, slave, cmd, setup, write, reads):
    master.execute_command('FLUSHALL')
    for spec in (setup or []):
        master.execute_command(*spec)
    master.execute_command(*write)

    acked = master.execute_command('WAIT', 1, WAIT_TIMEOUT_MS)
    env.assertEqual(acked, 1, message=f'{cmd}: replica did not ack the write')
    env.assertEqual(_keys(master), _keys(slave), message=f'{cmd}: key set diverged')

    for spec in reads:
        got = master.execute_command(*spec)
        env.assertFalse(_is_trivial(got), message=f'{cmd}: master {spec[0]} returned a default value')
        env.assertEqual(got, _read_slave(slave, spec), message=f'{cmd}: {spec[0]} diverged')


def testWriteCommandsReplicate(env):
    env.skipOnCluster()  # a cluster env has no replica connection to read
    # Under --use-aof the replica below would run with AOF, and RLTest intermittently hangs
    # stopping such a replica (it ignores SIGTERM; RLTest then waits on it with no timeout).
    # Replication is covered by the other env groups, so skip rather than add it here.
    env.skipOnAOF()
    env = Env(useSlaves=True, protocol=2)
    master, slave = env.getConnection(), env.getSlaveConnection()
    _wait_link_up(env)
    for cmd, setup, write, reads in ROWS:
        _verify_row(env, master, slave, cmd, setup, write, reads)


def testEveryWriteCommandIsCovered(env):
    """The command table is only as good as its coverage of the real command set."""
    env = Env(protocol=2)  # no replica needed: this only reads the command table
    if server_version_is_less_than('7.0'):
        env.skip()
    con = env.getConnection()
    # redis-py pipes every COMMAND * reply through its COMMAND INFO parser,
    # which cannot read a COMMAND LIST reply. Take the raw replies instead.
    con.set_response_callback('COMMAND', lambda r, **_: r)
    names = con.execute_command('COMMAND', 'LIST', 'FILTERBY', 'MODULE', MODULE_NAME)
    info = con.execute_command('COMMAND', 'INFO', *names)
    write_cmds = {c[0].lower() for c in info if c and 'write' in c[2]}
    missing = write_cmds - {cmd for cmd, _, _, _ in ROWS} - KNOWN_EXCLUSIONS
    env.assertEqual(missing, set(), message=f'write commands with no replication test: {missing}')
