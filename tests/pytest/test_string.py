# -*- coding: utf-8 -*-

from functools import reduce
import random
import sys
import os
import redis
import json
from RLTest import Env
from includes import *

from RLTest import Defaults

Defaults.decode_responses = True


def testGetScalarString(env):
    r = env

    r.assertTrue(r.execute_command('SET', 'x', '"thevalue"'))
    r.assertEqual(r.execute_command('JSON.GET', 'x', '$'), '["thevalue"]')

    r.assertTrue(r.execute_command('SET', 'x', '123'))
    r.assertEqual(r.execute_command('JSON.GET', 'x', '$'), '[123]')

    r.assertTrue(r.execute_command('SET', 'x', '[123,"a"]'))
    r.assertEqual(r.execute_command('JSON.GET', 'x', '$[0]'), '[123]')

def testGetFromString(env):
    r = env

    r.assertTrue(r.execute_command('SET', 'x', '{"a": 1, "b": [1, 2, 3]}'))
    r.assertEqual(r.execute_command('JSON.GET', 'x', '$.a'), '[1]')
    r.assertEqual(r.execute_command('JSON.GET', 'x', '$.b[0:2]'), '[1,2]')

def testSetToString(env):
    r = env

    r.assertTrue(r.execute_command('SET', 'x', '{"a": 1}'))
    
    r.assertOk(r.execute_command('JSON.SET', 'x', '$.a', '3'))
    r.assertEqual(r.execute_command('GET', 'x'), '{"a":3}')

    r.assertOk(r.execute_command('JSON.SET', 'x', '$.b', '[1,2]'))
    r.assertEqual(r.execute_command('GET', 'x'), '{"a":3,"b":[1,2]}')


def testParsingError(env):
    r = env

    r.assertTrue(r.execute_command('SET', 'x', 'thevalue'))
    r.expect('JSON.GET', 'x', '$').raiseError()

    r.assertTrue(r.execute_command('SET', 'x', '{"a": 1'))
    r.expect('JSON.GET', 'x', '$.a').raiseError()
