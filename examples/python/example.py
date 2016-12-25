import redis
import rejson

# Here's a document
doc = {
    'foo':  'bar',
    'baz':  42,
    'arr':  [0, 1, 2],
    'sub':  {
        'k1':   'v1'
    }
}

# Open a connection to redis
client = redis.StrictRedis()

# Set up the ReJSON class to use the Redis client
rj = rejson.ReJSON(client)

# Store the document
print rj.set('doc', '.', doc)               # prints OK

# Change some data
print rj.strappend('doc', '.foo', 'duck')   # prints 7 (the length of 'barduck')
print rj.numincrby('doc', '.baz', 6337)     # prints 6379
print rj.delete('doc', '.arr[-1]')          # prints 1
print rj.arrappend('doc', '.arr', 'more')   # prints 3
print rj.set('doc', '.sub', None)           # prints OK

# Retrieve it
doc = rj.get('doc')                         

# {u'arr': [0, 1, u'more'], u'foo': u'barduck', u'baz': 6379, u'sub': None}
print doc
