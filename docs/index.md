# ReJSON - a JSON data type for Redis

ReJSON is a [Redis](https://redis.io/) module that implements
[ECMA-404 The JSON Data Interchange Standard](http://json.org/) as a native data type. It allows storing, updating
and fetching JSON values from Redis keys (documents). The JSON values are managed as binary objects,
thus allowing Redis-blazing performance. 

ReJSON is developed with <3 at [Redis Labs](https://redislabs.com). The source code is available
from: https://github.com/RedisLabsModules/ReJSON

## Quickstart

1.  [Build the ReJSON module library](#building-the-module-library)
1.  [Load ReJSON to Redis](#loading-the-module-to-redis)
1.  [Use it from **any** Redis client](#using-rejson), e.g.:

![ReJSON with `redis-cli`](images/demo.gif)

## Building the module

### Linux Ubuntu 16.04

Prerequirements:

* The ReJSON repository: `git clone https://github.com/RedisLabsModules/rejson.git`
* The `build-essential` package: `apt-get install build-essential`
* CMake (v3.0.0 or greater): `apt-get install cmake`

This module employs standard CMake tooling. Assuming that the repository's directory is at
`~/rejson`, navigate to it and run the script [`/bootstrap.sh`](/bootstrap.sh):

```bash
~/rejson$ ./bootstrap.sh
-- The C compiler identification is GNU 5.4.0
...
!! 'REDIS_SERVER_PATH' variable not defined (use cmake -D) - module unit test will not be run
-- Configuring done
-- Generating done
-- Build files have been written to: rejson/build
```

The bootstrapping will initialize the build directory `build`. Optionally, you may [configure the
path to a Redis server executable](developer.md#testing) to enable unit testing.

Now you can build the module's library itself:

```bash
~/rejson$ cmake --build build --target rejson
Scanning dependencies of target rmobject
...
[100%] Linking C shared library rejson/lib/rejson.so
[100%] Built target rejson
```

Congratulations! You can find the compiled module library at `lib/rejson.so`.

### MacOSX

TBD

### Windows

Yeah, right :)

## Loading the module to Redis

Prerequirements:

* [Redis v4.0 or above](http://redis.io/download)

The recommended way have Redis load the module is during startup by by adding the following to the
`redis.conf` file:

```
loadmodule /path/to/module/rejson.so
```

In the line above replace `/path/to/module/rejson.so` with the actual path to the module's library.
Alternatively you, you can have Redis load the module using the following command line argument
syntax:

```bash
~/$ redis-server --loadmodule /path/to/module/rejson.so
```

Lastly, you can also use the [`MODULE LOAD`](http://redis.io/commands/module-load) command. Note,
however, that `MODULE LOAD` is a dangerous command and may be blocked/deprecated in the future due
to security considerations.

Once the module has been loaded successfully, the Redis log should have lines similar to:

```
...
1877:M 23 Dec 02:02:59.725 # <ReJSON> JSON data type for Redis - v1.0.0 [encver 0]
1877:M 23 Dec 02:02:59.725 * Module 'ReJSON' loaded from /foo/bar/rejson/lib/rejson.so
...
```

## Using ReJSON

Before using ReJSON you should familiarize yourself with its commands and syntax as detailed in the
[commands refernce](commands.md) document. However, to quickly get started just review this
section and get these two things:

1.  A Redis server running the the module (see [building](#building-the-module-library) and
    [loading](#loading-the-module-to-Redis) for instructions)
1.  Any [Redis client](http://redis.io/clients)

### With `redis-cli`

This example will use [`redis-cli`](http://redis.io/topics/rediscli) as the Redis client. The first
ReJSON command to try out is [`JSON.SET`](commands.md#jsonset), which sets a Redis key with a JSON
value. All JSON values can be used, for example a string:

```
127.0.0.1:6379> JSON.SET foo . '"bar"'
OK
127.0.0.1:6379> JSON.GET foo
"\"bar\""
127.0.0.1:6379> JSON.TYPE foo .
string
```

[`JSON.GET`](commands.md#jsonget) and [`JSON.TYPE`](commands.md#jsontype) do literally that
regardless of the value's type, but you should really check out `JSON.GET` prettifying powers. Note
how the commands are given the period character, i.e. `.`. This is the
[path](path.md) to the value in the ReJSON data type and in this case it just means
the root. A couple more of string operations:

```
127.0.0.1:6379> JSON.STRLEN foo .
3
127.0.0.1:6379> JSON.STRAPPEND foo . '"baz"'
6
127.0.0.1:6379> JSON.GET foo
"\"barbaz\""

``` 

[`JSON.STRLEN`](commands.md#jsonstrlen) tells you the length of the string, and you can append
another string to it with [`JSON.STRAPPEND`](commands.md#jsonstrappend). Numbers can be
[incremented](commands.md#jsonnumincrby) and [multiplied](commands.md#jsonnummultby):

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

The handy [`JSON.DEL`](commands.md#jsondel) command deletes anything you tell it to. Arrays can be
manipulated with a dedicated subset of ReJSON commands:

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

And objects have their own commands too:

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

### With any other client

Unless your [Redis client](http://redis.io/clients) already supports Redis modules (unlikely) or
ReJSON specifically (even unlikelier), you should be ok using its ability to send raw Redis
commands. Depending on your client of choice the exact method for doing that may vary.

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
[/examples/python](/examples/python).
