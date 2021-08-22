import signal
from contextlib import contextmanager


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
