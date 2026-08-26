# -*- coding: utf-8 -*-

import sys
import os
import redis
import json
from RLTest import Env

from common import *
from includes import *

from RLTest import Defaults

Defaults.decode_responses = True

# ----------------------------------------------------------------------------------------------

def testRecursiveDescentWithFilter(env):
    with open('issue674.json') as file:
        json_doc = file.read()
        env.expect('JSON.SET', 'test', '$', json_doc).ok()
        #res1 = json.loads(env.execute_command('JSON.GET', 'test', '$'))
        #res2 = json.loads(json_doc)
        env.expect('JSON.GET', 'test', '$..[?(@.uid==1198)].MD5ModelUID').equal('[\"92b3a00b2583b3ac680212feac4e1bf1\"]')
