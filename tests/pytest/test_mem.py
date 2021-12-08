
from common import *


JSON_FILES = [
    {'file': 'https://raw.githubusercontent.com/mloskot/json_benchmark/master/data/canada.json',
     'vsz': 2880000},
    {'file': 'https://raw.githubusercontent.com/RichardHightower/json-parsers-benchmark/master/data/citm_catalog.json',
     'vsz': 847000},
]


class TestMem:
    def __init__(self):
        for jfile in JSON_FILES:
            path = paella.wget(jfile['file'], tempdir=True)
            jfile['path'] = path
            with open(path) as jsonfile:
                jfile['doc'] = jsonfile.read()

    def __del__(self):
        for jfile in JSON_FILES:
            os.unlink(jfile['path'])

    def testKeys(self):
        for jfile in JSON_FILES:
            env = Env()
            vsz0 = checkEnvMem(env)
            for i in range(0, 100):
                env.execute_command('json.set', f'json{i}', '.', jfile['doc'])
            checkEnvMem(env, jfile['vsz'], vsz0)
            env.execute_command('flushall')
            env.stop()

    def testFields(self):
        for jfile in JSON_FILES:
            env = Env()
            vsz0 = checkEnvMem(env)
            env.execute_command('json.set', 'json', '.', '{}')
            for i in range(0, 100):
                env.execute_command('json.set', 'json', f'.json{i}', jfile['doc'])
            checkEnvMem(env, jfile['vsz'], vsz0)
            env.execute_command('flushall')
            env.stop()
