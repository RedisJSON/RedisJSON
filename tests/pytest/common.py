
from contextlib import contextmanager
from includes import *


@contextmanager
def TimeLimit(timeout):
    def handler(signum, frame):
        raise Exception('TimeLimit timeout')

    signal.signal(signal.SIGALRM, handler)
    signal.setitimer(signal.ITIMER_REAL, timeout, 0)
    try:
        yield
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, signal.SIG_DFL)

def envMem(env):
    pid = env.envRunner.masterProcess.pid
    ps = paella.sh(f"ps u -p {pid}", join=False, lines=True)
    f = re.split(r"\s*", ps[1].strip())
    return {'vsz': int(f[4]), 'rss': int(f[5]) }

def checkEnvMem(env, expected_vsz=None, vsz0=0, threshold=0.1):
    if os.getenv('MEMINFO', '0') == '1':
        pid = env.envRunner.masterProcess.pid
        print(paella.sh(f'cat /proc/{pid}/status | grep ^Vm', join=False))
    mem = envMem(env)
    vsz = mem['vsz'] - vsz0
    if expected_vsz is not None:
        env.assertGreater(vsz, expected_vsz * (1 - threshold))
        env.assertLess(vsz, expected_vsz * (1 + threshold))
    return vsz
