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
    # Test index of int scalar in multi values using filter expression
    r.assertOk(r.execute_command('JSON.SET',
                                 'store',
                                 '$',
                                 '{"store":{"book":[{"category":"reference","author":"Nigel Rees","title":"Sayings of the Century","price":8.95,"size":[10,20,30,40]},{"category":"fiction","author":"Evelyn Waugh","title":"Sword of Honour","price":12.99,"size":[50,60,70,80]},{"category":"fiction","author":"Herman Melville","title":"Moby Dick","isbn":"0-553-21311-3","price":8.99,"size":[5,10,20,30]},{"category":"fiction","author":"J. R. R. Tolkien","title":"The Lord of the Rings","isbn":"0-395-19395-8","price":22.99,"size":[5,6,7,8]}],"bicycle":{"color":"red","price":19.95}}}'))

    res = r.execute_command('JSON.GET',
                            'store',
                            '$.store.book[?(@.price<10)].size')
    r.assertEqual(res, '[[10,20,30,40],[5,10,20,30]]')
    res = r.execute_command('JSON.ARRINDEX',
                            'store',
                            '$.store.book[?(@.price<10)].size',
                            '20')
    r.assertEqual(res, [1, 2])

    # Test index of int scalar in multi values
    r.assertOk(r.execute_command('JSON.SET', 'test_num',
                                 '.',
                                 '[{"arr":[0,1,3.0,3,2,1,0,3]},{"nested1_found":{"arr":[5,4,3,2,1,0,1,2,3.0,2,4,5]}},{"nested2_not_found":{"arr":[2,4,6]}},{"nested3_scalar":{"arr":"3"}},[{"nested41_not_arr":{"arr_renamed":[1,2,3]}},{"nested42_empty_arr":{"arr":[]}}]]'))

    res = r.execute_command('JSON.GET', 'test_num', '$..arr')
    r.assertEqual(res, '[[0,1,3.0,3,2,1,0,3],[5,4,3,2,1,0,1,2,3.0,2,4,5],[2,4,6],"3",[]]')

    res = r.execute_command('JSON.ARRINDEX', 'test_num', '$..arr', 3)
    r.assertEqual(res, [3, 2, -1, None, -1])

    # Test index of double scalar in multi values
    res = r.execute_command('JSON.ARRINDEX', 'test_num', '$..arr', 3.0)
    r.assertEqual(res, [2, 8, -1, None, -1])

    # Test index of string scalar in multi values
    r.assertOk(r.execute_command('JSON.SET', 'test_string',
                                 '.',
                                 '[{"arr":["bazzz","bar",2,"baz",2,"ba","baz",3]},{"nested1_found":{"arr":[null,"baz2","buzz",2,1,0,1,"2","baz",2,4,5]}},{"nested2_not_found":{"arr":["baz2",4,6]}},{"nested3_scalar":{"arr":"3"}},[{"nested41_arr":{"arr_renamed":[1,"baz",3]}},{"nested42_empty_arr":{"arr":[]}}]]'))
    res = r.execute_command('JSON.GET', 'test_string', '$..arr')
    r.assertEqual(res, '[["bazzz","bar",2,"baz",2,"ba","baz",3],[null,"baz2","buzz",2,1,0,1,"2","baz",2,4,5],["baz2",4,6],"3",[]]')

    res = r.execute_command('JSON.ARRINDEX', 'test_string', '$..arr', '"baz"')
    r.assertEqual(res, [3, 8, -1, None, -1])

    res = r.execute_command('JSON.ARRINDEX', 'test_string', '$..arr', '"baz"', 2)
    r.assertEqual(res, [3, 8, -1, None, -1])
    res = r.execute_command('JSON.ARRINDEX', 'test_string', '$..arr', '"baz"', 4)
    r.assertEqual(res, [6, 8, -1, None, -1])
    res = r.execute_command('JSON.ARRINDEX', 'test_string', '$..arr', '"baz"', -5)
    r.assertEqual(res, [3, 8, -1, None, -1])
    res = r.execute_command('JSON.ARRINDEX', 'test_string', '$..arr', '"baz"', 4, 7)
    r.assertEqual(res, [6, -1, -1, None, -1])
    res = r.execute_command('JSON.ARRINDEX', 'test_string', '$..arr', '"baz"', 4, -1)
    r.assertEqual(res, [6, 8, -1, None, -1])
    res = r.execute_command('JSON.ARRINDEX', 'test_string', '$..arr', '"baz"', 4, 0)
    r.assertEqual(res, [6, 8, -1, None, -1])
    res = r.execute_command('JSON.ARRINDEX', 'test_string', '$..arr', '5', 7, -1)
    r.assertEqual(res, [-1, -1, -1, None, -1])
    res = r.execute_command('JSON.ARRINDEX', 'test_string', '$..arr', '5', 7, 0)
    r.assertEqual(res, [-1, 11, -1, None, -1])

    # Test index of null scalar in multi values
    r.assertOk(r.execute_command('JSON.SET', 'test_null',
                                 '.',
                                 '[{"arr":["bazzz","null",2,null,2,"ba","baz",3]},{"nested1_found":{"arr":["zaz","baz2","buzz",2,1,0,1,"2",null,2,4,5]}},{"nested2_not_found":{"arr":["null",4,6]}},{"nested3_scalar":{"arr":null}},[{"nested41_arr":{"arr_renamed":[1,null,3]}},{"nested42_empty_arr":{"arr":[]}}]]'))
    res = r.execute_command('JSON.GET', 'test_null', '$..arr')
    r.assertEqual(res, '[["bazzz","null",2,null,2,"ba","baz",3],["zaz","baz2","buzz",2,1,0,1,"2",null,2,4,5],["null",4,6],null,[]]')

    res = r.execute_command('JSON.ARRINDEX', 'test_null', '$..arr', 'null')
    r.assertEqual(res, [3, 8, -1, None, -1])

    # Fail with none-scalar value
    r.expect('JSON.ARRINDEX', 'test_null', '$..nested42_empty_arr.arr', '{"arr":[]}').raiseError()

    # Do not fail with none-scalar value in legacy mode
    res = r.execute_command('JSON.ARRINDEX', 'test_null', '.[4][1].nested42_empty_arr.arr', '{"arr":[]}')
    r.assertEqual(res, -1)

    # Test legacy (path begins with dot)
    # Test index of int scalar in single value
    r.assertEqual(r.execute_command('JSON.ARRINDEX', 'test_num', '.[0].arr', 3), 3)
    r.assertEqual(r.execute_command('JSON.ARRINDEX', 'test_num', '.[0].arr', 9), -1)
    r.expect('JSON.ARRINDEX', 'test_num', '.[0].arr_not', 3).raiseError()
    # Test index of string scalar in single value
    r.assertEqual(r.execute_command('JSON.ARRINDEX', 'test_string', '.[0].arr', '"baz"'), 3)
    r.assertEqual(r.execute_command('JSON.ARRINDEX', 'test_string', '.[0].arr', '"faz"'), -1)
    # Test index of null scalar in single value
    r.assertEqual(r.execute_command('JSON.ARRINDEX', 'test_null', '.[0].arr', 'null'), 3)
    r.assertEqual(r.execute_command('JSON.ARRINDEX', 'test_null', '..nested2_not_found.arr', 'null'), -1)

