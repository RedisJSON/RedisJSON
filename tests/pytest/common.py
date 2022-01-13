import signal
from contextlib import contextmanager
from functools import wraps
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

def skipOnExistingEnv(env):
    if 'existing' in env.env:
        env.skip()

def skipOnCrdtEnv(env):
    if len([a for a in env.cmd('module', 'list') if a[1] == 'crdt']) > 0:
        env.skip()

def skip(f, on_cluster=False):
    @wraps(f)
    def wrapper(env, *args, **kwargs):
        if not on_cluster or env.isCluster():
            env.skip()
            return
        return f(env, *args, **kwargs)
    return wrapper

def no_san(f):
    @wraps(f)
    def wrapper(env, *args, **kwargs):
        if SANITIZER != '':
            fname = f.__name__
            env.debugPrint("skipping {} due to memory sanitizer".format(fname), force=True)
            env.skip()
            return
        return f(env, *args, **kwargs)
    return wrapper
