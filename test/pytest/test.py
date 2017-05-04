from rmtest import ModuleTestCase
import redis
import unittest
import json
import os

# Path to module
module_path = os.environ['REDIS_MODULE_PATH']
# Path to redis-server executable
redis_path = os.environ['REDIS_SERVER_PATH']
# Path to JSON test case files
json_path = os.path.abspath(os.path.join(os.getcwd(), os.environ['JSON_PATH']))

# TODO: these are currently not supported so ignore them
json_ignore = [
    'pass-json-parser-0002.json',   # UTF-8 to Unicode
    'pass-json-parser-0005.json',   # big numbers
    'pass-json-parser-0006.json',   # UTF-8 to Unicode
    'pass-json-parser-0007.json',   # UTF-8 to Unicode
    'pass-json-parser-0012.json',   # UTF-8 to Unicode
    'pass-jsonsl-1.json',           # big numbers
    'pass-jsonsl-yelp.json',        # float percision
]

# Some basic documents to use in the tests
docs = {
    'simple': {
        'foo': 'bar',
    },
    'basic': {
        'string': 'string value',
        'none': None,
        'bool': True,
        'int': 42,
        'num': 4.2,
        'arr': [42, None, -1.2, False, ['sub', 'array'], {'subdict': True}],
        'dict': {
            'a': 1,
            'b': '2',
            'c': None,
        }
    },
    'scalars': {
        'unicode': 'string value',
        'NoneType': None,
        'bool': True,
        'int': 42,
        'float': -1.2,
    },
    'values': {
        'unicode': 'string value',
        'NoneType': None,
        'bool': True,
        'int': 42,
        'float': -1.2,
        'dict': {},
        'list': []
    },
    'types': {
        'null':     None,
        'boolean':  False,
        'integer':  42,
        'number':   1.2,
        'string':   'str',
        'object':   {},
        'array':    [],
    },
}


class ReJSONTestCase(ModuleTestCase(module_path=module_path, redis_path=redis_path)):
    """Tests ReJSON Redis module in vitro"""

    def assertNotExists(self, r, key, msg=None):
        self.assertFalse(r.exists(key), msg)  

    def assertOk(self, x, msg=None):
        self.assertEquals("OK", x, msg)
    
    def assertExists(self, r, key, msg=None):
        self.assertTrue(r.exists(key), msg)

    def testSetRootWithInvalidJSONValuesShouldFail(self):
        """Test that setting the root of a ReJSON key with invalid JSON values fails"""
        with self.redis() as r:
            r.delete('test')
            invalid = ['{', '}', '[', ']', '{]', '[}', '\\', '\\\\', '',
                       ' ', '\\"', '\'', '\[', '\x00', '\x0a', '\x0c', '\xff']
            for i in invalid:
                with self.assertRaises(redis.exceptions.ResponseError) as cm:
                    r.execute_command('JSON.SET', 'test', '.', i)
                self.assertNotExists(r, 'test', i)

    def testSetInvalidPathShouldFail(self):
        """Test that invalid paths fail"""
        with self.redis() as r:
            r.delete('test')
            invalid = ['', ' ', '\x00', '\x0a', '\x0c', '\xff',
                       '."', '.\x00', '.\x0a\x0c', '.-foo', '.43',
                       '.foo\n.bar']
            for i in invalid:
                with self.assertRaises(redis.exceptions.ResponseError) as cm:
                    r.execute_command('JSON.SET', 'test', i, 'null')
                self.assertNotExists(r, 'test', i)

    def testSetRootWithJSONValuesShouldSucceed(self):
        """Test that the root of a JSON key can be set with any valid JSON"""
        with self.redis() as r:
            for v in ['string', 1, -2, 3.14, None, True, False, [], {}]:
                r.delete('test')
                j = json.dumps(v)
                self.assertOk(r.execute_command('JSON.SET', 'test', '.', j), v)
                self.assertExists(r, 'test')
                s = json.loads(r.execute_command('JSON.GET', 'test'))
                if type(v) is dict:
                    self.assertDictEqual(v, s, v)
                elif type(v) is list:
                    self.assertListEqual(v, s, v)
                else:
                    self.assertEqual(v, s, v)

    def testSetReplaceRootShouldSucceed(self):
        """Test replacing the root of an existing key with a valid object succeeds"""

        with self.redis() as r:
            r.delete('test')
            self.assertOk(r.execute_command('JSON.SET', 'test', '.', json.dumps(docs['basic'])))
            self.assertOk(r.execute_command('JSON.SET', 'test', '.', json.dumps(docs['simple'])))
            raw = r.execute_command('JSON.GET', 'test', '.')
            self.assertDictEqual(json.loads(raw), docs['simple'])
            for k, v in docs['values'].iteritems():
                self.assertOk(r.execute_command('JSON.SET', 'test', '.', json.dumps(v)), k)
                data = json.loads(r.execute_command('JSON.GET', 'test', '.'))
                self.assertEqual(str(type(data)), '<type \'{}\'>'.format(k), k)
                self.assertEqual(data, v)

    def testSetGetWholeBasicDocumentShouldBeEqual(self):
        """Test basic JSON.GET/JSON.SET"""

        with self.redis() as r:
            r.delete('test')
            data = json.dumps(docs['basic'])
            self.assertOk(r.execute_command('JSON.SET', 'test', '.', data))
            self.assertExists(r, 'test')
            self.assertEqual(json.dumps(json.loads(
                r.execute_command('JSON.GET', 'test'))), data)

    def testSetBehaviorModifyingSubcommands(self):
        """Test JSON.SET's NX and XX subcommands"""

        with self.redis() as r:
            r.delete('test')

            # test against the root
            self.assertIsNone(r.execute_command('JSON.SET', 'test', '.', '{}', 'XX'))
            self.assertOk(r.execute_command('JSON.SET', 'test', '.', '{}', 'NX'))
            self.assertIsNone(r.execute_command('JSON.SET', 'test', '.', '{}', 'NX'))
            self.assertOk(r.execute_command('JSON.SET', 'test', '.', '{}', 'XX'))

            # test an object key
            self.assertIsNone(r.execute_command('JSON.SET', 'test', '.foo', '[]', 'XX'))
            self.assertOk(r.execute_command('JSON.SET', 'test', '.foo', '[]', 'NX'))
            self.assertIsNone(r.execute_command('JSON.SET', 'test', '.foo', '[]', 'NX'))
            self.assertOk(r.execute_command('JSON.SET', 'test', '.foo', '[1]', 'XX'))

            # verify failure for arrays
            with self.assertRaises(redis.exceptions.ResponseError) as cm:
                r.execute_command('JSON.SET', 'test', '.foo[1]', 'null', 'NX')
            with self.assertRaises(redis.exceptions.ResponseError) as cm:
                r.execute_command('JSON.SET', 'test', '.foo[1]', 'null', 'XX')

    def testGetNonExistantPathsFromBasicDocumentShouldFail(self):
        """Test failure of getting non-existing values"""

        with self.redis() as r:
            r.delete('test')
            self.assertOk(r.execute_command('JSON.SET', 'test',
                                            '.', json.dumps(docs['scalars'])))

            # Paths that do not exist
            paths = ['.foo', 'boo', '.key1[0]', '.key2.bar', '.key5[99]', '.key5["moo"]']
            for p in paths:
                with self.assertRaises(redis.exceptions.ResponseError) as cm:
                    r.execute_command('JSON.GET', 'test', p)

            # Test failure in multi-path get
            with self.assertRaises(redis.exceptions.ResponseError) as cm:
                r.execute_command('JSON.GET', 'test', '.bool', paths[0])

    def testGetPartsOfValuesDocumentOneByOne(self):
        """Test type and value returned by JSON.GET"""

        with self.redis() as r:
            r.delete('test')
            self.assertOk(r.execute_command('JSON.SET', 'test',
                                            '.', json.dumps(docs['values'])))
            for k, v in docs['values'].iteritems():
                data = json.loads(r.execute_command('JSON.GET', 'test', '.{}'.format(k)))
                self.assertEqual(str(type(data)), '<type \'{}\'>'.format(k), k)
                self.assertEqual(data, v, k)

    def testGetPartsOfValuesDocumentMultiple(self):
        """Test correctnes of an object returned by JSON.GET"""

        with self.redis() as r:
            r.delete('test')
            self.assertOk(r.execute_command('JSON.SET', 'test',
                                            '.', json.dumps(docs['values'])))
            data = json.loads(r.execute_command('JSON.GET', 'test', *docs['values'].keys()))
            self.assertDictEqual(data, docs['values'])

    def testMgetCommand(self):
        """Test REJSON.MGET command"""

        with self.redis() as r:
            # Set up a few keys
            for d in range(0, 5):
                key = 'doc:{}'.format(d)
                r.delete(key)
                self.assertOk(r.execute_command('JSON.SET', key, '.', json.dumps(docs['basic'])), d)

            # Test an MGET that succeeds on all keys
            raw = r.execute_command('JSON.MGET', *['doc:{}'.format(d) for d in range(0, 5)] + ['.'])
            self.assertEqual(len(raw), 5)
            for d in range(0, 5):
                key = 'doc:{}'.format(d)
                self.assertDictEqual(json.loads(raw[d]), docs['basic'], d)

            # Test an MGET that fails for one key
            r.delete('test')
            self.assertOk(r.execute_command('JSON.SET', 'test', '.', '{"bool":false}'))
            raw = r.execute_command('JSON.MGET', 'test', 'doc:0', 'foo', '.bool')
            self.assertEqual(len(raw), 3)
            self.assertFalse(json.loads(raw[0]))
            self.assertTrue(json.loads(raw[1]))
            self.assertEqual(raw[2], None)

    def testDelCommand(self):
        """Test REJSON.DEL command"""

        with self.redis() as r:
            r.delete('test')

            # Test deleting an empty object
            self.assertOk(r.execute_command('JSON.SET', 'test', '.', '{}'))
            self.assertEqual(r.execute_command('JSON.DEL', 'test', '.'), 1)
            self.assertNotExists(r, 'test')

            # Test deleting some keys from an object
            self.assertOk(r.execute_command('JSON.SET', 'test', '.', '{}'))
            self.assertOk(r.execute_command('JSON.SET', 'test', '.foo', '"bar"'))
            self.assertOk(r.execute_command('JSON.SET', 'test', '.baz', '"qux"'))
            self.assertEqual(r.execute_command('JSON.DEL', 'test', '.baz'), 1)
            self.assertEqual(r.execute_command('JSON.OBJLEN', 'test', '.'), 1)
            self.assertIsNone(r.execute_command('JSON.TYPE', 'test', '.baz'))
            self.assertEqual(r.execute_command('JSON.DEL', 'test', '.foo'), 1)
            self.assertEqual(r.execute_command('JSON.OBJLEN', 'test', '.'), 0)
            self.assertIsNone(r.execute_command('JSON.TYPE', 'test', '.foo'))
            self.assertEqual(r.execute_command('JSON.TYPE', 'test', '.'), 'object')

            # Test with an array
            self.assertOk(r.execute_command('JSON.SET', 'test', '.foo', '"bar"'))
            self.assertOk(r.execute_command('JSON.SET', 'test', '.baz', '"qux"'))
            self.assertOk(r.execute_command('JSON.SET', 'test', '.arr', '[1.2,1,2]'))
            self.assertEqual(r.execute_command('JSON.DEL', 'test', '.arr[1]'), 1)
            self.assertEqual(r.execute_command('JSON.OBJLEN', 'test', '.'), 3)
            self.assertEqual(r.execute_command('JSON.ARRLEN', 'test', '.arr'), 2)
            self.assertEqual(r.execute_command('JSON.TYPE', 'test', '.arr'), 'array')
            self.assertEqual(r.execute_command('JSON.DEL', 'test', '.arr'), 1)
            self.assertEqual(r.execute_command('JSON.OBJLEN', 'test', '.'), 2)
            self.assertEqual(r.execute_command('JSON.DEL', 'test', '.'), 1)
            self.assertIsNone(r.execute_command('JSON.GET', 'test'))

    def testObjectCRUD(self):
        """Test JSON Object CRUDness"""
        with self.redis() as r:
            r.delete('test')

            # Create an object
            self.assertOk(r.execute_command('JSON.SET', 'test', '.', '{ }'))
            self.assertEqual('object', r.execute_command('JSON.TYPE', 'test', '.'))
            self.assertEqual(0, r.execute_command('JSON.OBJLEN', 'test', '.'))
            raw = r.execute_command('JSON.GET', 'test')
            data = json.loads(raw)
            self.assertDictEqual(data, {})

            # Test failure to access a non-existing element
            with self.assertRaises(redis.exceptions.ResponseError) as cm:
                r.execute_command('JSON.GET', 'test', '.foo')

            # Test setting a key in the oject
            self.assertOk(r.execute_command('JSON.SET', 'test', '.foo', '"bar"'))
            self.assertEqual(1, r.execute_command('JSON.OBJLEN', 'test', '.'))
            raw = r.execute_command('JSON.GET', 'test', '.')
            data = json.loads(raw)
            self.assertDictEqual(data, {u'foo': u'bar'})

            # Test replacing a key's value in the object
            self.assertOk(r.execute_command('JSON.SET', 'test', '.foo', '"baz"'))
            raw = r.execute_command('JSON.GET', 'test', '.')
            data = json.loads(raw)
            self.assertDictEqual(data, {u'foo': u'baz'})

            # Test adding another key to the object
            self.assertOk(r.execute_command('JSON.SET', 'test', '.boo', '"far"'))
            self.assertEqual(2, r.execute_command('JSON.OBJLEN', 'test', '.'))
            raw = r.execute_command('JSON.GET', 'test', '.')
            data = json.loads(raw)
            self.assertDictEqual(data, {u'foo': u'baz', u'boo': u'far'})

            # Test deleting a key from the object
            self.assertEqual(1, r.execute_command('JSON.DEL', 'test', '.foo'))
            raw = r.execute_command('JSON.GET', 'test', '.')
            data = json.loads(raw)
            self.assertDictEqual(data, {u'boo': u'far'})

            # Test replacing the object
            self.assertOk(r.execute_command('JSON.SET', 'test', '.', '{"foo": "bar"}'))
            raw = r.execute_command('JSON.GET', 'test', '.')
            data = json.loads(raw)
            self.assertDictEqual(data, {u'foo': u'bar'})

            # Test deleting the object
            self.assertEqual(1, r.execute_command('JSON.DEL', 'test', '.'))
            self.assertIsNone(r.execute_command('JSON.GET', 'test', '.'))

    def testArrayCRUD(self):
        """Test JSON Array CRUDness"""

        with self.redis() as r:
            r.delete('test')

            # Test creation of an empty array
            self.assertOk(r.execute_command('JSON.SET', 'test', '.', '[]'))
            self.assertEqual('array', r.execute_command('JSON.TYPE', 'test', '.'))
            self.assertEqual(0, r.execute_command('JSON.ARRLEN', 'test', '.'))

            # Test failure of setting an element at different positons in an empty array
            with self.assertRaises(redis.exceptions.ResponseError) as cm:
                r.execute_command('JSON.SET', 'test', '[0]', 0)
            with self.assertRaises(redis.exceptions.ResponseError) as cm:
                r.execute_command('JSON.SET', 'test', '[19]', 0)
            with self.assertRaises(redis.exceptions.ResponseError) as cm:
                r.execute_command('JSON.SET', 'test', '[-1]', 0)

            # Test appending and inserting elements to the array
            self.assertEqual(1, r.execute_command('JSON.ARRAPPEND', 'test', '.', 1))
            self.assertEqual(1, r.execute_command('JSON.ARRLEN', 'test', '.'))
            self.assertEqual(2, r.execute_command('JSON.ARRINSERT', 'test', '.', 0, -1))
            self.assertEqual(2, r.execute_command('JSON.ARRLEN', 'test', '.'))
            data = json.loads(r.execute_command('JSON.GET', 'test', '.'))
            self.assertListEqual([-1, 1, ], data)
            self.assertEqual(3, r.execute_command('JSON.ARRINSERT', 'test', '.', -1, 0))
            data = json.loads(r.execute_command('JSON.GET', 'test', '.'))
            self.assertListEqual([-1, 0, 1, ], data)
            self.assertEqual(5, r.execute_command('JSON.ARRINSERT', 'test', '.', -3, -3, -2))
            data = json.loads(r.execute_command('JSON.GET', 'test', '.'))
            self.assertListEqual([-3, -2, -1, 0, 1, ], data)
            self.assertEqual(7, r.execute_command('JSON.ARRAPPEND', 'test', '.', 2, 3))
            data = json.loads(r.execute_command('JSON.GET', 'test', '.'))
            self.assertListEqual([-3, -2, -1, 0, 1, 2, 3], data)

            # Test replacing elements in the array
            self.assertOk(r.execute_command('JSON.SET', 'test', '[0]', '"-inf"'))
            self.assertOk(r.execute_command('JSON.SET', 'test', '[-1]', '"+inf"'))
            self.assertOk(r.execute_command('JSON.SET', 'test', '[3]', 'null'))
            data = json.loads(r.execute_command('JSON.GET', 'test', '.'))
            self.assertListEqual([u'-inf', -2, -1, None, 1, 2, u'+inf'], data)

            # Test deleting from the array
            self.assertEqual(1, r.execute_command('JSON.DEL', 'test', '[1]'))
            self.assertEqual(1, r.execute_command('JSON.DEL', 'test', '[-2]'))
            data = json.loads(r.execute_command('JSON.GET', 'test', '.'))
            self.assertListEqual([u'-inf', -1, None, 1, u'+inf'], data)

            # Test trimming the array
            self.assertEqual(4, r.execute_command('JSON.ARRTRIM', 'test', '.', 1, -1))
            data = json.loads(r.execute_command('JSON.GET', 'test', '.'))
            self.assertListEqual([-1, None, 1, u'+inf'], data)
            self.assertEqual(3, r.execute_command('JSON.ARRTRIM', 'test', '.', 0, -2))
            data = json.loads(r.execute_command('JSON.GET', 'test', '.'))
            self.assertListEqual([-1, None, 1], data)
            self.assertEqual(1, r.execute_command('JSON.ARRTRIM', 'test', '.', 1, 1))
            data = json.loads(r.execute_command('JSON.GET', 'test', '.'))
            self.assertListEqual([None], data)

            # Test replacing the array
            self.assertOk(r.execute_command('JSON.SET', 'test', '.', '[true]'))
            self.assertEqual('array', r.execute_command('JSON.TYPE', 'test', '.'))
            self.assertEqual(1, r.execute_command('JSON.ARRLEN', 'test', '.'))
            self.assertEqual('true', r.execute_command('JSON.GET', 'test', '[0]'))

    def testArrIndexCommand(self):
        """Test JSON.ARRINDEX command"""
        with self.redis() as r:
            r.delete('test')
            self.assertOk(r.execute_command('JSON.SET', 'test',
                                            '.', '{ "arr": [0, 1, 2, 3, 2, 1, 0] }'))
            self.assertEqual(r.execute_command('JSON.ARRINDEX', 'test', '.arr', 0), 0)
            self.assertEqual(r.execute_command('JSON.ARRINDEX', 'test', '.arr', 3), 3)
            self.assertEqual(r.execute_command('JSON.ARRINDEX', 'test', '.arr', 4), -1)
            self.assertEqual(r.execute_command('JSON.ARRINDEX', 'test', '.arr', 0, 1), 6)
            self.assertEqual(r.execute_command('JSON.ARRINDEX', 'test', '.arr', 0, -1), 6)
            self.assertEqual(r.execute_command('JSON.ARRINDEX', 'test', '.arr', 0, 6), 6)
            self.assertEqual(r.execute_command('JSON.ARRINDEX', 'test', '.arr', 0, 4, -0), 6)
            self.assertEqual(r.execute_command('JSON.ARRINDEX', 'test', '.arr', 0, 5, -1), -1)
            self.assertEqual(r.execute_command('JSON.ARRINDEX', 'test', '.arr', 2, -2, 6), -1)
            self.assertEqual(r.execute_command('JSON.ARRINDEX', 'test', '.arr', '"foo"'), -1)

            self.assertEqual(r.execute_command('JSON.ARRINSERT', 'test', '.arr', 4, '[4]'), 8)
            self.assertEqual(r.execute_command('JSON.ARRINDEX', 'test', '.arr', 3), 3)
            self.assertEqual(r.execute_command('JSON.ARRINDEX', 'test', '.arr', 2, 3), 5)
            self.assertEqual(r.execute_command('JSON.ARRINDEX', 'test', '.arr', '[4]'), -1)

    def testArrTrimCommand(self):
        """Test JSON.ARRTRIM command"""

        with self.redis() as r:
            r.delete('test')
            self.assertOk(r.execute_command('JSON.SET', 'test',
                                            '.', '{ "arr": [0, 1, 2, 3, 2, 1, 0] }'))
            self.assertEqual(r.execute_command('JSON.ARRTRIM', 'test', '.arr', 1, -2), 5)
            self.assertListEqual(json.loads(r.execute_command(
                'JSON.GET', 'test', '.arr')), [1, 2, 3, 2, 1])
            self.assertEqual(r.execute_command('JSON.ARRTRIM', 'test', '.arr', 0, 99), 5)
            self.assertListEqual(json.loads(r.execute_command(
                'JSON.GET', 'test', '.arr')), [1, 2, 3, 2, 1])
            self.assertEqual(r.execute_command('JSON.ARRTRIM', 'test', '.arr', 0, 2), 3)
            self.assertListEqual(json.loads(r.execute_command(
                'JSON.GET', 'test', '.arr')), [1, 2, 3])
            self.assertEqual(r.execute_command('JSON.ARRTRIM', 'test', '.arr', 99, 2), 0)
            self.assertListEqual(json.loads(r.execute_command('JSON.GET', 'test', '.arr')), [])

    def testArrPopCommand(self):
        """Test JSON.ARRPOP command"""

        with self.redis() as r:
            r.delete('test')
            self.assertOk(r.execute_command('JSON.SET', 'test',
                                            '.', '[1,2,3,4,5,6,7,8,9]'))
            self.assertEqual('9', r.execute_command('JSON.ARRPOP', 'test'))
            self.assertEqual('8', r.execute_command('JSON.ARRPOP', 'test', '.'))
            self.assertEqual('7', r.execute_command('JSON.ARRPOP', 'test', '.', -1))
            self.assertEqual('5', r.execute_command('JSON.ARRPOP', 'test', '.', -2))
            self.assertEqual('1', r.execute_command('JSON.ARRPOP', 'test', '.', 0))
            self.assertEqual('4', r.execute_command('JSON.ARRPOP', 'test', '.', 2))
            self.assertEqual('6', r.execute_command('JSON.ARRPOP', 'test', '.', 99))
            self.assertEqual('2', r.execute_command('JSON.ARRPOP', 'test', '.', -99))
            self.assertEqual('3', r.execute_command('JSON.ARRPOP', 'test'))
            self.assertIsNone(r.execute_command('JSON.ARRPOP', 'test'))

    def testTypeCommand(self):
        """Test JSON.TYPE command"""

        with self.redis() as r:
            for k, v in docs['types'].iteritems():
                r.delete('test')
                self.assertOk(r.execute_command('JSON.SET', 'test', '.', json.dumps(v)))
                reply = r.execute_command('JSON.TYPE', 'test', '.')
                self.assertEqual(reply, k)

    def testLenCommands(self):
        """Test the JSON.ARRLEN, JSON.OBJLEN and JSON.STRLEN commands"""

        with self.redis() as r:
            r.delete('test')

            # test that nothing is returned for empty keys
            self.assertEqual(r.execute_command('JSON.ARRLEN', 'foo', '.bar'), None)

            # test elements with valid lengths
            self.assertOk(r.execute_command('JSON.SET', 'test', '.', json.dumps(docs['basic'])))
            self.assertEqual(r.execute_command('JSON.STRLEN', 'test', '.string'), 12)
            self.assertEqual(r.execute_command('JSON.OBJLEN', 'test', '.dict'), 3)
            self.assertEqual(r.execute_command('JSON.ARRLEN', 'test', '.arr'), 6)

            # test elements with undefined lengths
            with self.assertRaises(redis.exceptions.ResponseError) as cm:
                r.execute_command('JSON.ARRLEN', 'test', '.bool')
            with self.assertRaises(redis.exceptions.ResponseError) as cm:
                r.execute_command('JSON.STRLEN', 'test', '.none')
            with self.assertRaises(redis.exceptions.ResponseError) as cm:
                r.execute_command('JSON.OBJLEN', 'test', '.int')
            with self.assertRaises(redis.exceptions.ResponseError) as cm:
                r.execute_command('JSON.STRLEN', 'test', '.num')

            # test a non existing key
            with self.assertRaises(redis.exceptions.ResponseError) as cm:
                r.execute_command('JSON.LEN', 'test', '.foo')

            # test an out of bounds index
            with self.assertRaises(redis.exceptions.ResponseError) as cm:
                r.execute_command('JSON.LEN', 'test', '.arr[999]'), -1

            # test an infinite index
            with self.assertRaises(redis.exceptions.ResponseError) as cm:
                r.execute_command('JSON.LEN', 'test', '.arr[-inf]')

    def testObjKeysCommand(self):
        """Test JSON.OBJKEYS command"""

        with self.redis() as r:
            r.delete('test')
            self.assertOk(r.execute_command('JSON.SET', 'test', '.', json.dumps(docs['types'])))
            data = r.execute_command('JSON.OBJKEYS', 'test', '.')
            self.assertEqual(len(data), len(docs['types']))
            for k in data:
                self.assertTrue(k in docs['types'], k)

            # test a wrong type
            with self.assertRaises(redis.exceptions.ResponseError) as cm:
                r.execute_command('JSON.OBJKEYS', 'test', '.null')

    def testNumIncrCommand(self):
        """Test JSON.NUMINCRBY command"""

        with self.redis() as r:
            r.delete('test')
            self.assertOk(r.execute_command('JSON.SET', 'test', '.', '{ "foo": 0, "bar": "baz" }'))
            self.assertEqual('1', r.execute_command('JSON.NUMINCRBY', 'test', '.foo', 1))
            self.assertEqual('1', r.execute_command('JSON.GET', 'test', '.foo'))
            self.assertEqual('3', r.execute_command('JSON.NUMINCRBY', 'test', '.foo', 2))
            self.assertEqual('3.5', r.execute_command('JSON.NUMINCRBY', 'test', '.foo', .5))

            # test a wrong type
            with self.assertRaises(redis.exceptions.ResponseError) as cm:
                r.execute_command('JSON.NUMINCRBY', 'test', '.bar', 1)

            # test a missing path
            with self.assertRaises(redis.exceptions.ResponseError) as cm:
                r.execute_command('JSON.NUMINCRBY', 'test', '.fuzz', 1)

            # test issue #9
            self.assertOk(r.execute_command('JSON.SET', 'num', '.', '0'))
            self.assertEqual('1', r.execute_command('JSON.NUMINCRBY', 'num', '.', 1))
            self.assertEqual('2.5', r.execute_command('JSON.NUMINCRBY', 'num', '.', 1.5))            

    def testStrCommands(self):
        """Test JSON.STRAPPEND and JSON.STRLEN commands"""

        with self.redis() as r:
            r.delete('test')
            self.assertOk(r.execute_command('JSON.SET', 'test', '.', '"foo"'))
            self.assertEqual('string', r.execute_command('JSON.TYPE', 'test', '.'))
            self.assertEqual(3, r.execute_command('JSON.STRLEN', 'test', '.'))
            self.assertEqual(6, r.execute_command('JSON.STRAPPEND', 'test', '.', '"bar"'))
            self.assertEqual('"foobar"', r.execute_command('JSON.GET', 'test', '.'))

    def testRespCommand(self):
        """Test JSON.RESP command"""

        with self.redis() as r:
            r.delete('test')
            self.assertOk(r.execute_command('JSON.SET', 'test', '.', 'null'))
            self.assertIsNone(r.execute_command('JSON.RESP', 'test'))
            self.assertOk(r.execute_command('JSON.SET', 'test', '.', 'true'))
            self.assertEquals('true', r.execute_command('JSON.RESP', 'test'))
            self.assertOk(r.execute_command('JSON.SET', 'test', '.', 42))
            self.assertEquals(42, r.execute_command('JSON.RESP', 'test'))
            self.assertOk(r.execute_command('JSON.SET', 'test', '.', 2.5))
            self.assertEquals('2.5', r.execute_command('JSON.RESP', 'test'))
            self.assertOk(r.execute_command('JSON.SET', 'test', '.', '"foo"'))
            self.assertEquals('foo', r.execute_command('JSON.RESP', 'test'))
            self.assertOk(r.execute_command('JSON.SET', 'test', '.', '{"foo":"bar"}'))
            resp = r.execute_command('JSON.RESP', 'test')
            self.assertEqual(2, len(resp))
            self.assertEqual('{', resp[0])
            self.assertEqual(2, len(resp[1]))
            self.assertEqual('foo', resp[1][0])
            self.assertEqual('bar', resp[1][1])
            self.assertOk(r.execute_command('JSON.SET', 'test', '.', '[1,2]'))
            resp = r.execute_command('JSON.RESP', 'test')
            self.assertEqual(3, len(resp))
            self.assertEqual('[', resp[0])
            self.assertEqual(1, resp[1])
            self.assertEqual(2, resp[2])

    def testAllJSONCaseFiles(self):
        """Test using all JSON test case files"""
        self.maxDiff = None
        with self.redis() as r:
            for file in os.listdir(json_path):
                if file.endswith('.json'):
                    path = '{}/{}'.format(json_path, file)
                    r.delete('test')
                    with open(path) as f:
                        value = f.read()
                        if file.startswith('pass-'):
                            self.assertOk(r.execute_command('JSON.SET', 'test', '.', value), path)
                        elif file.startswith('fail-'):
                            with self.assertRaises(redis.exceptions.ResponseError) as cm:
                                r.execute_command('JSON.SET', 'test', '.', value)
                            self.assertNotExists(r, 'test', path)

    def testSetGetComparePassJSONCaseFiles(self):
        """Test setting, getting, saving and loading passable JSON test case files"""

        with self.redis() as r:
            for jsonfile in os.listdir(json_path):
                self.maxDiff = None
                if jsonfile.startswith('pass-') and jsonfile.endswith('.json') and jsonfile not in json_ignore:
                    path = '{}/{}'.format(json_path, jsonfile)
                    r.flushdb()
                    with open(path) as f:
                        value = f.read()
                        self.assertOk(r.execute_command('JSON.SET', jsonfile, '.', value), path)
                        d1 = json.loads(value)
                        for _ in r.retry_with_rdb_reload():
                            self.assertExists(r, jsonfile)
                            raw = r.execute_command('JSON.GET', jsonfile)
                            d2 = json.loads(raw)
                            if type(d1) is dict:
                                self.assertDictEqual(d1, d2, path)
                            elif type(d1) is list:
                                self.assertListEqual(d1, d2, path)
                            else:
                                self.assertEqual(d1, d2, path)

    def testIssue_13(self):
        """https://github.com/RedisLabsModules/rejson/issues/13"""

        with self.redis() as r:
            r.delete('test')
            self.assertOk(r.execute_command('JSON.SET', 'test', '.', json.dumps(docs['simple'])))
            # This shouldn't crash Redis
            r.execute_command('JSON.GET', 'test', 'foo', 'foo')


if __name__ == '__main__':
    unittest.main()
