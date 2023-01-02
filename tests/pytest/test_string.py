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

def testGetFromString(env):
    r = env

    r.assertTrue(r.execute_command('SET', 'x', '{"a": 1}'))
    r.assertEqual(r.execute_command('JSON.GET', 'x', '$.a'), '[1]')


def testSetToString(env):
    r = env

    r.assertTrue(r.execute_command('SET', 'x', '{"a": 1}'))
    r.assertOk(r.execute_command('JSON.SET', 'x', '$.a', '3'))
    r.assertEqual(r.execute_command('GET', 'x'), '{"a":3}')
