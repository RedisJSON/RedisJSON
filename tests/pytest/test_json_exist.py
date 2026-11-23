import json
from RLTest import Env


def test_json_exist_matches():
    for protocol in (2, 3):
        env = Env(protocol=protocol, decodeResponses=True)
        env.expect('JSON.SET', 'doc', '$', json.dumps(
            {'items': [None, False, 0, '', [], {}]})).ok()
        for path in ['$', '$.items[*]', '$.items[0,0]', '.items']:
            env.expect('JSON.EXIST', 'doc', path).equal(1)
        for index in range(6):
            env.expect('JSON.EXIST', 'doc', '$.items[%d]' % index).equal(1)
        for path in ['$.absent', '$.items[99]', '$.items[?(@ == 42)]']:
            env.expect('JSON.EXIST', 'doc', path).equal(0)
        env.expect('JSON.EXIST', 'missing', '$').equal(0)
        env.expect('EXISTS', 'missing').equal(0)


def test_json_exist_errors():
    env = Env(decodeResponses=True)
    env.expect('JSON.SET', 'doc', '$', '{}').ok()
    for key in ['doc', 'missing']:
        env.expect('JSON.EXIST', key, '$[').raiseError()
        env.expect('JSON.EXIST', key, '$.a + 1').raiseError().contains('projection')
    env.cmd('SET', 'string', 'value')
    env.expect('JSON.EXIST', 'string', '$').raiseError()
    env.expect('JSON.EXIST', 'doc').raiseError()
    env.expect('JSON.EXIST', 'doc', '$', '$').raiseError()
    env.expect('COMMAND GETKEYS', 'JSON.EXIST', 'doc', '$').equal(['doc'])
