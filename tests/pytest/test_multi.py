# -*- coding: utf-8 -*-

import sys
import os
import redis
import json
from RLTest import Env
from includes import *

from RLTest import Defaults

Defaults.decode_responses = True

# ----------------------------------------------------------------------------------------------

# Path to JSON test case files
HERE = os.path.abspath(os.path.dirname(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "../.."))
TESTS_ROOT = os.path.abspath(os.path.join(HERE, ".."))
JSON_PATH = os.path.join(TESTS_ROOT, 'files')


def testDelCommand(env):
    """Test REJSON.DEL command"""
    r = env

    r.assertOk(r.execute_command('JSON.SET', 'doc1', '$', '{"a": 1, "nested": {"a": 2, "b": 3}}'))
    res = r.execute_command('JSON.DEL', 'doc1', '$..a')
    r.assertEqual(res, 2)
    res = r.execute_command('JSON.GET', 'doc1', '$')
    r.assertEqual(res, '[{"nested":{"b":3}}]')

    # Test deletion of nested hierarchy - only higher hierarchy is deleted
    r.assertOk(r.execute_command('JSON.SET', 'doc2', '$', '{"a": {"a": 2, "b": 3}, "b": ["a", "b"], "nested": {"b":[true, "a","b"]}}'))
    res = r.execute_command('JSON.DEL', 'doc2', '$..a')
    r.assertEqual(res, 1)
    res = r.execute_command('JSON.GET', 'doc2', '$')
    r.assertEqual(res, '[{"nested":{"b":[true,"a","b"]},"b":["a","b"]}]')

    r.assertOk(r.execute_command('JSON.SET', 'doc3', '$', '[{"ciao":["non ancora"],"nested":[{"ciao":[1,"a"]}, {"ciao":[2,"a"]}, {"ciaoc":[3,"non","ciao"]}, {"ciao":[4,"a"]}, {"e":[5,"non","ciao"]}]}]'))
    res = r.execute_command('JSON.DEL', 'doc3', '$.[0]["nested"]..ciao')
    r.assertEqual(res, 3)
    res = r.execute_command('JSON.GET', 'doc3', '$')
    r.assertEqual(res, '[[{"ciao":["non ancora"],"nested":[{},{},{"ciaoc":[3,"non","ciao"]},{},{"e":[5,"non","ciao"]}]}]]')
