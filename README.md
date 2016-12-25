# ReJSON - a JSON data type for Redis

ReJSON is a [Redis](1) module that implements
[ECMA-404 The JSON Data Interchange Standard](2) as a native data type. It allows storing, updating
and fetching JSON values from Redis keys (documents). The JSON values are managed as binary objects,
thus allowing Redis-blazing performance. 

## Quickstart

1.  [Build the ReJSON module library](#building-the-module-library)
1.  [Load ReJSON to Redis](#loading-the-module-to-redis)
1.  [Use it from **any** Redis client](#using-rejson), e.g.:

![ReJSON with `redis-cli`](demo.gif)

## What is ReJSON

TODO

## Limitations and known issues

* Alpha quality
* AOF rewrite will fail for documents with serialization over 0.5GB?
* Searching for object keys is O(N)
* Unicode is assumed to be totally unsupported and is definitely untested
* Containers are not scaled down after deleting items

## Building the module library

### Linux

Prerequirements:

* The ReJSON repository: `git clone https://github.com/RedisLabsModules/rejson.git`
* The `build-essential` and `cmake` packages: `apt-get install build-essential cmake`

This module employs standard CMake tooling. Assuming that the repository's directory is at
`~/rejson`, navigate to it and run the script [`bootstrap.sh`](bootstrap.sh) followed by `cmake`.
The output should look something like:

```
~/rejson$ ./bootstrap.sh
-- The C compiler identification is GNU 5.4.0
...
!! 'REDIS_SERVER_PATH' variable not defined (use cmake -D) - module unit test will not be run
-- Configuring done
-- Generating done
-- Build files have been written to: rejson/build
~/rejson$ cmake --build build --target rejson
Scanning dependencies of target rmobject
...
[100%] Linking C shared library rejson/lib/rejson.so
[100%] Built target rejson
rejson$ 
```

Congratulations! You can find the compiled module library at `lib/rejson.so`.

#### MacOSX

TBD

#### Windows

Yeah, right :)

## Loading the module to Redis

Prerequirements:

* [Redis v4.0 or above](3)

The recommended way have Redis load the module is during startup by by adding the following to the
`redis.conf` file:

```
loadmodule /path/to/module/rejson.so
```

In the line above replace `/path/to/module/rejson.so` with the actual path to the module's library.
Alternatively you, you can have Redis load the module using the following command line argument
syntax:

```
~/$ redis-server --loadmodule /path/to/module/rejson.so
```

Lastly, you can also use the [`MODULE LOAD`](4) command. Note, however, that `MODULE LOAD` is a
dangerous command and may be blocked/deprecated in the future due to security considerations.

Once the module has been loaded successfully, the Redis log should have lines similar to:

```
...
1877:M 23 Dec 02:02:59.725 # <ReJSON> JSON data type for Redis - v1.0.0 [encver 0]
1877:M 23 Dec 02:02:59.725 * Module 'ReJSON' loaded from /foo/bar/rejson/lib/rejson.so
...
```

## Using ReJSON

Before using ReJSON you should familiarize yourself with its commands and syntax as detailed in the
[commands refernce](docs/commands) document. However, to quickly get started just review this
section and get these two things:

1.  A Redis server running the the module (see [building](#building-the-module-library) and
    [loading](#loading-the-module-to-Redis) for instructions)
1.  Any [Redis client](5)

### Using `redis-cli`

This example will use [`redis-cli`](6) as a the Redis client. The first ReJSON command to try out is
[`JSON.SET`](docs/commands.md#set), which sets a Redis key with a JSON value. All JSON values can be
used, for example a [string](docs/commands.md#string-operations):

```
127.0.0.1:6379> JSON.SET foo . '"bar"'
OK
127.0.0.1:6379> JSON.GET foo
"\"bar\""
127.0.0.1:6379> JSON.TYPE foo .
string
```

[`JSON.GET`](docs/commands.md#get) and [`JSON.TYPE`](#docs/commands.md#type) do literally that
regardless of the value's type, but you should really check out `JSON.GET` prettifying powers. Note
how the commands are given the period character, i.e. `.`. This is the
[path](docs/commmands.md#path) to the value in the ReJSON data type and in this case it just means
the root. A couple more of [string operations](docs/commands.md#string-operations):

```
127.0.0.1:6379> JSON.STRLEN foo .
3
127.0.0.1:6379> JSON.STRAPPEND foo . '"baz"'
6
127.0.0.1:6379> JSON.GET foo
"\"barbaz\""

``` 

[`JSON.STRLEN`](docs/commands.md#strlen) tells you the length of the string, and you can append
another string to it with [`JSON.STRAPPEND`](docs/commands.md#strappend). Numbers can be
[incremented](docs/commands.md#numincrby) and [multiplied](docs/commands.md#nummultby):

```
127.0.0.1:6379> JSON.SET num . 0
OK
127.0.0.1:6379> JSON.NUMINCRBY num . 1
"1"
127.0.0.1:6379> JSON.NUMINCRBY num . 1.5
"2.5"
127.0.0.1:6379> JSON.NUMINCRBY num . -0.75
"1.75"
127.0.0.1:6379> JSON.NUMMULTBY num . 24
"42"
```

Of course, a more interesting example would involve an array or maybe an object. Because or isn't
xor here goes:

```
127.0.0.1:6379> JSON.SET amoreinterestingexample . '[ true, { "answer": 42 }, null ]'
OK
127.0.0.1:6379> JSON.GET amoreinterestingexample
"[true,{\"answer\":42},null]"
127.0.0.1:6379> JSON.GET amoreinterestingexample [1].answer
"42"
127.0.0.1:6379> JSON.DEL amoreinterestingexample [-1]
1
127.0.0.1:6379> JSON.GET amoreinterestingexample
"[true,{\"answer\":42}]"
```

The handy [`JSON.DEL`](docs/commands.md#del) command deletes anything you tell it to. Arrays can be
manipulated with a [dedicated subset](docs/commands.md#array-operations) of ReJSON commands:

```
127.0.0.1:6379> JSON.SET arr . []
OK
127.0.0.1:6379> JSON.ARRAPPEND arr . 0
(integer) 1
127.0.0.1:6379> JSON.GET arr
"[0]"
127.0.0.1:6379> JSON.ARRINSERT arr . 0 -2 -1
(integer) 3
127.0.0.1:6379> JSON.GET arr
"[-2,-1,0]"
127.0.0.1:6379> JSON.ARRTRIM arr . 1 1
1
127.0.0.1:6379> JSON.GET arr
"[-1]"
127.0.0.1:6379> JSON.ARRPOP arr
"-1"
127.0.0.1:6379> JSON.ARRPOP arr
(nil)
```

And objects have their [own commands](docs/commands.md#object-operations) too:

```
127.0.0.1:6379> JSON.SET obj . '{"name":"Leonard Cohen","lastSeen":1478476800,"loggedOut": true}'
OK
127.0.0.1:6379> JSON.OBJLEN obj .
(integer) 3
127.0.0.1:6379> JSON.OBJKEYS obj .
1) "name"
2) "lastSeen"
3) "loggedOut"
```

### Using any other client

Unless your [Redis client](5) already supports Redis modules (unlikely) or ReJSON specifically (even
unlikelier), you should be ok using its ability to send raw Redis commands. Depending on your client
of choice the exact method for doing that may vary.

#### Python example

This code snippet shows how to use ReJSON from Python with
[redis-py](https://github.com/andymccurdy/redis-py):

```Python
import redis
import json

data = {
    'foo': 'bar'
}

r = redis.StrictRedis()
r.execute_command('JSON.SET', 'doc', '.', json.dumps(data))
reply = json.loads(r.execute_command('JSON.GET', 'doc'))
```

For a more comprehensive example, including a simple Python wrapper for ReJSON, see
[examples/python](examples/python).

#### Node.js example

TODO

## RAM usage

Every key in Redis takes memory and requires at least the amount of RAM to store the key name, as
well as some per-key overhead that Redis uses. On top of that, the value in the key also requires
RAM.

ReJSON stores JSON values as binary data after deserializing them. This representation is often more
expensive, size-wize, than the serialized form. The ReJSON data type uses at least 24 bytes (on
64-bit architectures) for every value, as can be seen by sampling an empty string with the
[`JSON.MEMORY`](docs/commands.md#memory) command:

```
127.0.0.1:6379> JSON.SET emptystring . '""'
OK
127.0.0.1:6379> JSON.MEMORY emptystring
(integer) 24
```

This RAM requirement is the same for all scalar values, but strings require additional space
depending on their actual length. For example, a 3-character string will use 3 additional bytes:

```
127.0.0.1:6379> JSON.SET foo . '"bar"'
OK
127.0.0.1:6379> JSON.MEMORY foo
(integer) 27
```

Empty containers take up 32 bytes to set up:

```
127.0.0.1:6379> JSON.SET arr . '[]'
OK
127.0.0.1:6379> JSON.MEMORY arr
(integer) 32
127.0.0.1:6379> JSON.SET obj . '{}'
OK
127.0.0.1:6379> JSON.MEMORY obj
(integer) 32
```

The actual size of a the container is the sum of sizes of all items in it on top of its own
overhead. To avoid expensive memory reallocations, containers' capacity is scaled by multiples of 2
until they a treshold size is reached, from which they grow by fixed chunks.

A container with a single scalar is made up of 32 and 24 bytes, respectively:
```
127.0.0.1:6379> JSON.SET arr . '[""]'
OK
127.0.0.1:6379> JSON.MEMORY arr
(integer) 56
```

A container with two scalars requires 40 bytes for the container (each pointer to an entry in the
container is 8 bytes), and 2 * 24 bytes for the values themselves:
```
127.0.0.1:6379> JSON.SET arr . '["", ""]'
OK
127.0.0.1:6379> JSON.MEMORY arr
(integer) 88
```

A 3-item (each 24 bytes) container will be allocated with capacity for 4 items, i.e. 56 bytes:

```
127.0.0.1:6379> JSON.SET arr . '["", "", ""]'
OK
127.0.0.1:6379> JSON.MEMORY arr
(integer) 128
```

The next item will not require an allocation in the container so usage will increase only by that
scalar's requirement, but another value will scale the container again:

```
127.0.0.1:6379> JSON.SET arr . '["", "", "", ""]'
OK
127.0.0.1:6379> JSON.MEMORY arr
(integer) 152
127.0.0.1:6379> JSON.SET arr . '["", "", "", "", ""]'
OK
127.0.0.1:6379> JSON.MEMORY arr
(integer) 208
```

Note: in the current version, deleting values from containers **does not** free the container's
allocated memory.

## Design

You can find some information abouth ReJSON's design at [docs/design.md](docs/design.md).

## Testing

Python is required for ReJSON's module test. Install it with `apt-get install python`.

Also, the module's test requires a path to the `redis-server` executable. The path is stored in the
`REDIS_SERVER_PATH` variable and can be set CMake's `-D` switch as follows:

```
~/rejson$ cmake -D REDIS_SERVER_PATH=/path/to/redis-server --build build
```

And then run the tests:

```
~/rejson$ cd build
~/rejson/build$ ctest
...
```

## Contributing

## License
AGPLv3 - see [LICENSE](LICENSE)

  [1]:  http://redis.io/
  [2]:  http://json.org/
  [3]:  http://redis.io/download
  [4]:  http://redis.io/commands/module-load
  [5]:  http://redis.io/clients
  [6]:  http://redis.io/topics/rediscli