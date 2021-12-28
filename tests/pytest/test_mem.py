
from common import *
import time
import datetime


CHECK_MEMRECLAIM = os.getenv('MEMRECLAIM', '0') == '1'
SERDE_JSON = os.getenv('SERDE_JSON', '0') == '1'

JSON_FILES = [
    {'file': 'https://raw.githubusercontent.com/mloskot/json_benchmark/master/data/canada.json',
     'vsz': 2880000 if SERDE_JSON else 598000},
    {'file': 'https://raw.githubusercontent.com/RichardHightower/json-parsers-benchmark/master/data/citm_catalog.json',
     'vsz': 847000 if SERDE_JSON else 200000},
]


class TestMem:
    def __init__(self):
        self.env = Env()
        for jfile in JSON_FILES:
            path = paella.wget(jfile['file'], tempdir=True)
            jfile['path'] = path
            with open(path) as jsonfile:
                jfile['doc'] = jsonfile.read()

    def __del__(self):
        for jfile in JSON_FILES:
            os.unlink(jfile['path'])

    def testKeys(self):
        env = self.env
        env.skipOnCluster()
        if env.useAof or env.useSlaves:
            env.skip()

        def add_and_check(title):
            t0 = time.monotonic()
            for i in range(0, 100):
                env.execute_command('json.set', f'json{i}', '$', jfile['doc'])
            title += f" t={datetime.timedelta(seconds=time.monotonic() - t0)}"
            checkEnvMem(env, expected_vsz=jfile['vsz'], vsz0=vsz0, title=title)
        def delete():
            t0 = time.monotonic()
            for i in range(0, 100):
                env.execute_command('json.del', f'json{i}')
            print(f"--- del: t={datetime.timedelta(seconds=time.monotonic() - t0)}")

        fi = 0
        for jfile in JSON_FILES:
            fi += 1
            vsz0 = checkEnvMem(env, title=f"before (keys {fi})")
            add_and_check(f"add (keys {fi})")
            if CHECK_MEMRECLAIM:
                delete()
                add_and_check(f"add after del (keys {fi})")
            env.execute_command('flushall') # so no .rdb file is created
            env.stop()
            env.start()

    def testFields(self):
        env = self.env
        env.skipOnCluster()
        if env.useAof or env.useSlaves:
            env.skip()

        def add_and_check(title):
            t0 = time.monotonic()
            env.execute_command('json.set', 'json', '.', '{}')
            for i in range(0, 100):
                env.execute_command('json.set', 'json', f'$.json{i}', jfile['doc'])
            title += f" t={datetime.timedelta(seconds=time.monotonic() - t0)}"
            checkEnvMem(env, expected_vsz=jfile['vsz'], vsz0=vsz0, title=title)
        def delete():
            t0 = time.monotonic()
            for i in range(0, 100):
                env.execute_command('json.del', 'json', f'json{i}')
            env.debugPrint(f"--- del: t={datetime.timedelta(seconds=time.monotonic() - t0)}")

        fi = 0
        for jfile in JSON_FILES:
            fi += 1
            vsz0 = checkEnvMem(env, title=f"before (fields {fi})")
            add_and_check(f"add (fields {fi})")
            if CHECK_MEMRECLAIM:
                delete()
                add_and_check(f"add after del (fields {fi})")
            env.execute_command('flushall') # so no .rdb file is created
            env.stop()
            env.start()
