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
        #print(((list(res.values()))[0]))
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
        r.assertEqual(r.execute_command('JSON.GET', 'test_resp3', '$.a3'), [[]])
        r.assertEqual(r.execute_command('JSON.GET', 'test_no_such_key', '$.a3'), None)
        r.expect('JSON.GET', 'test_not_JSON', '$.a3').raiseError()



        
