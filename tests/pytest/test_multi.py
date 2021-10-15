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


def testArrIndexCommand(env):
    """Test JSON.ARRINDEX command"""
    r = env
    r.assertOk(r.execute_command('JSON.SET',
                                 'store',
                                 '$',
                                 '{"store":{"book":[{"category":"reference","author":"Nigel Rees","title":"Sayings of the Century","price":8.95,"size":[10,20,30,40]},{"category":"fiction","author":"Evelyn Waugh","title":"Sword of Honour","price":12.99,"size":[50,60,70,80]},{"category":"fiction","author":"Herman Melville","title":"Moby Dick","isbn":"0-553-21311-3","price":8.99,"size":[5,10,20,30]},{"category":"fiction","author":"J. R. R. Tolkien","title":"The Lord of the Rings","isbn":"0-395-19395-8","price":22.99,"size":[5,6,7,8]}],"bicycle":{"color":"red","price":19.95}}}'))

    res = r.execute_command('JSON.GET',
                            'store',
                            '$.store.book[?(@.price<10)].size')
    r.assertEqual(res, '[[10,20,30,40],[5,10,20,30]]')

    # Test multi values in result
    res = r.execute_command('JSON.ARRINDEX',
                            'store',
                            '$.store.book[?(@.price<10)].size',
                            '20')
    r.assertEqual(res, [1, 2])

    r.assertOk(r.execute_command('JSON.SET', 'test2',
                                 '.',
                                 '[{"arr":[0,1,2,3,2,1,0,3]},{"nested1_found:":{"arr":[5,4,3,2,1,0,1,2,3,2,4,5]}},{"nested2_not_found:":{"arr":[2,4,6]}},{"nested3_scalar:":{"arr":"3"}},[{"nested41_not_arr:":{"arr_not":[1,2,3]}},{"nested42_empty_arr:":{"arr":[]}}]]'))

    # Test multi values
    res = r.execute_command('JSON.GET', 'test2', '$..arr')
    r.assertEqual(res, '[[0,1,2,3,2,1,0,3],[5,4,3,2,1,0,1,2,3,2,4,5],[2,4,6],"3",[]]')

    res = r.execute_command('JSON.ARRINDEX', 'test2', '$..arr', 3)
    #r.assertEqual(res, [[3, 7], [2, 8], -1, None, -1])
    r.assertEqual(res, [3, 2, -1, None, -1])

    # Test single value (legacy)
    r.assertEqual(r.execute_command('JSON.ARRINDEX', 'test2', '.[0].arr', 3), 3)
    r.assertEqual(r.execute_command('JSON.ARRINDEX', 'test2', '.[0].arr', 9), -1)
    r.expect('JSON.ARRINDEX', 'test2', '.[0].arr_not', 3).raiseError()

    #X res = r.execute_command('JSON.MGET', 'test2', 'test1', '.[0].arr')
    #X r.assertEqual(res, ['[0,1,2,3,2,1,0,3]', None])
