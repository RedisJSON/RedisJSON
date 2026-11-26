import json
from RLTest import Env


def check_paths(protocol):
    env = Env(protocol=protocol, decodeResponses=True)
    key = 'paths{test}'
    doc = {'items': [{'price': 10}, {'name': 'missing'}, {'price': None}, {'price': 30}],
           'a.b': {'0': 'numeric member'}, 'quote"slash\\': 7, '$a': 8, 'שלום': 9}
    env.expect('JSON.SET', key, '$', json.dumps(doc)).ok()
    def paths(*args):
        return json.loads(env.cmd('JSON.PATH', key, *args))
    env.assertEqual(paths(), ['$'])
    env.assertEqual(paths('$'), ['$'])
    expected = ['$["items"][0]["price"]', '$["items"][2]["price"]', '$["items"][3]["price"]']
    env.assertEqual(paths('$.items[*].price'), expected)
    for legacy in ['.', '.items[*].price', 'items', '$a']:
        for target in [key, 'missing{test}']:
            env.expect('JSON.PATH', target, legacy).raiseError().contains('legacy')
        env.expect('JSON.PATH', key, '$', legacy).raiseError().contains('legacy')
    env.assertEqual(paths('$["$a"]'), ['$["$a"]'])
    env.expect('JSON.GET', key, '.items[*].price').equal('10')
    env.expect('JSON.GET', key, '$a').equal('8')
    env.assertEqual(paths('$.items[3,0,3].price'), [expected[2], expected[0], expected[2]])
    env.assertEqual(paths('$.items[?(@.price > 15)].price'), [expected[2]])
    env.assertEqual(paths('$.absent'), [])
    env.expect('JSON.PATH', 'missing{test}', '$').equal(None)
    env.cmd('SET', 'string{test}', 'x')
    env.expect('JSON.PATH', 'string{test}', '$').raiseError().contains('wrong Redis type')
    env.expect('JSON.PATH').raiseError().contains('wrong number')
    env.expect('JSON.PATH', key, '$[').raiseError()
    env.assertEqual(paths('$.items[0].price + 1'), [None])
    env.assertEqual(paths('$.items[0].price', '$.absent'), {'$.items[0].price': [expected[0]], '$.absent': []})
    env.assertEqual(paths('$.items[*].price', '$.items[*].price'), {'$.items[*].price': expected})
    env.assertEqual(paths('INDENT', '  ', 'NEWLINE', '\n', 'SPACE', ' ', '$.items[*].price'), expected)
    for command in ['JSON.GET', 'JSON.PATH']:
        env.expect(command, key, 'FORMAT', 'STRINGS', '$').raiseError()
    for query in ['$.items[*].price', '$.items[3,0,3].price', '$["a.b"]["0"]', '$.*', '$..price']:
        locations = paths(query)
        values = json.loads(env.cmd('JSON.GET', key, query))
        env.assertEqual(len(locations), len(values))
        for path, value in zip(locations, values):
            env.assertEqual(json.loads(env.cmd('JSON.GET', key, path)), [value])
    if protocol == 3:
        env.expect('JSON.PATH', key, 'FORMAT', 'EXPAND', '.items[*].price').raiseError().contains('legacy')
        env.expect('JSON.PATH', key, 'FORMAT', 'EXPAND').equal([['$']])
        env.expect('JSON.PATH', key, 'FORMAT', 'EXPAND', '$.items[*].price', '$.absent').equal([expected, []])
        env.expect('JSON.PATH', key, 'FORMAT', 'EXPAND1', '$.items[*].price').equal([[json.dumps(p) for p in expected]])
        env.expect('JSON.PATH', key, 'FORMAT', 'EXPAND', '$.items[0].price + 1').equal([[None]])
    else:
        env.expect('JSON.PATH', key, 'FORMAT', 'EXPAND', '$').raiseError()
    pipe = env.getConnection().pipeline(transaction=True)
    pipe.execute_command('JSON.PATH', key, '$.items[*].price')
    pipe.execute_command('JSON.GET', key, '$.items[*].price')
    locations, values = pipe.execute()
    env.assertEqual(list(zip(json.loads(locations), json.loads(values))), list(zip(expected, [10, None, 30])))
    env.assertEqual(env.cmd('COMMAND GETKEYS', 'JSON.PATH', key, '$'), [key])


def test_json_path_resp2():
    check_paths(2)


def test_json_path_resp3():
    check_paths(3)
