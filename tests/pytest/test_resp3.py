# -*- coding: utf-8 -*-
import sys
import os
import redis
import json
from RLTest import Env

from common import *
from includes import *

from RLTest import Defaults

from functools import reduce

Defaults.decode_responses = True


def is_redis_version_smaller_than(con, _version, is_cluster=False):

    res = con.execute_command('INFO')
    ver = ""
    if is_cluster:
        try:
            ver = ((list(res.values()))[0])['redis_version']
        except:
            ver = res['redis_version']
    else:
        ver = res['redis_version']
    return (version.parse(ver) < version.parse(_version))


class testResp3():
    def __init__(self):
        self.env = Env(protocol=3)

    def test_resp3_set_get(self):
        r = self.env

        r.assertTrue(r.execute_command('SET', 'test_not_JSON', 'test_not_JSON'))

        # Test JSON.SET RESP3
        r.assertOk(r.execute_command('JSON.SET', 'test_resp3', '$', '{"a1":{"b":{"c":1}},"a2":{"b":{"c":2}}}'))

        # Test JSON.GET RESP3
        r.assertEqual(r.execute_command('JSON.GET', 'test_resp3', '$'), [['{"a1":{"b":{"c":1}},"a2":{"b":{"c":2}}}']])
        r.assertEqual(r.execute_command('JSON.GET', 'test_resp3', '$..b'), [['{"c":1}', '{"c":2}']])
        r.assertEqual(r.execute_command('JSON.GET', 'test_resp3', '$.a1', '$.a2'),  [['{"b":{"c":1}}'], ['{"b":{"c":2}}']])
        r.assertEqual(r.execute_command('JSON.GET', 'test_resp3', '$.a1', '$.a3', '$.a2'),  [['{"b":{"c":1}}'], [], ['{"b":{"c":2}}']])
        r.assertEqual(r.execute_command('JSON.GET', 'test_resp3', '$.a3'), [[]])

        # TEST JSON.GET with none existent key
        r.assertEqual(r.execute_command('JSON.GET', 'test_no_such_key', '$.a3'), None)

        # TEST JSON.GET with not a JSON key
        r.expect('JSON.GET', 'test_not_JSON', '$.a3').raiseError()

    # Test JSON.DEL RESP3
    def test_resp_json_del(self):
        r = self.env

        r.assertTrue(r.execute_command('SET', 'test_not_JSON', 'test_not_JSON'))

        r.assertOk(r.execute_command('JSON.SET', 'test_resp3', '$', '{"a1":{"b":{"c":1}},"a2":{"b":{"c":2}}}'))
        
        r.assertEqual(r.execute_command('JSON.DEL', 'test_resp3', '$..b'), 2)
        
        # Test none existing path
        r.assertEqual(r.execute_command('JSON.DEL', 'test_resp3', '$.a1.b'), 0)

        # Test none existing key
        r.assertEqual(r.execute_command('JSON.DEL', 'test_no_such_key', '$.a1.b'), 0)

        # Test not a JSON key
        r.expect('JSON.DEL', 'test_not_JSON', '$.a1.b').raiseError()

    # Test JSON.NUMINCRBY RESP3
    def test_resp_json_num_ops(self):
        r = self.env

        r.assertTrue(r.execute_command('SET', 'test_not_JSON', 'test_not_JSON'))

        r.assertOk(r.execute_command('JSON.SET', 'test_resp3', '$', '{"a1":{"b":{"c":1}},"a2":{"b":{"c":2.2}},"a3":{"b":{"c":"val"}}}'))

        # Test NUMINCRBY
        r.assertEqual(r.execute_command('JSON.NUMINCRBY', 'test_resp3', '$.a1.b.c', 1), [2])
        r.assertEqual(r.execute_command('JSON.NUMINCRBY', 'test_resp3', '$..c', 2), [4, 4.2, None])

        # Test NUMMULTBY
        r.assertEqual(r.execute_command('JSON.NUMMULTBY', 'test_resp3', '$.a2.b.c', 2.3), [9.66])
        r.assertEqual(r.execute_command('JSON.NUMMULTBY', 'test_resp3', '$..c', 2), [8, 19.32, None])

        # Test none existing key
        r.expect('JSON.NUMINCRBY', 'test_no_such_key', '$.a1.b', '1').raiseError()

        # Test not a JSON key
        r.expect('JSON.NUMMULTBY', 'test_not_JSON', '$.a1.b', '1').raiseError()


    # Test JSON.MSET RESP3
    def test_resp_json_mset(self):
        r = self.env

        r.assertTrue(r.execute_command('SET', 'test_not_JSON', 'test_not_JSON'))

        r.assertOk(r.execute_command('JSON.MSET', 'test_resp3_1{s}', '$', '{"a1":{"b":{"c":1}},"a2":{"b":{"c":2}}}', 'test_resp3_2{s}', '$', '{"a1":{"b":{"c":1}},"a2":{"b":{"c":2}}}'))

        # Test none existing key
        r.expect('JSON.MSET', 'test_no_such_key', '$.a1.b', '1').raiseError()

        # Test not a JSON key
        r.expect('JSON.MSET', 'test_not_JSON', '$.a1.b', '1').raiseError()

    # Test JSON.MERGE RESP3
    def test_resp_json_merge(self):
        r = self.env

        r.assertTrue(r.execute_command('SET', 'test_not_JSON', 'test_not_JSON'))

        r.assertOk(r.execute_command('JSON.SET', 'test_resp3', '$', '{"a1":{"b":{"c":1}},"a2":{"b":{"c":2}}}'))

        r.assertOk(r.execute_command('JSON.MERGE', 'test_resp3', '$', '{"a3":4}'))

        # Test none existing key
        r.expect('JSON.MERGE', 'test_no_such_key', 'test_resp3_1', '$', '{"a3":4}').raiseError()

        # Test not a JSON key
        r.expect('JSON.MERGE', 'test_not_JSON', '$', '{"a3":4}').raiseError()
