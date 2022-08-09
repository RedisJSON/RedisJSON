---
title: RedisJSON
description: JSON support for Redis
linkTitle: JSON
type: docs
---

[![Discord](https://img.shields.io/discord/697882427875393627?style=flat-square)](https://discord.gg/QUkjSsk)
[![Github](https://img.shields.io/static/v1?label=&message=repository&color=5961FF&logo=github)](https://github.com/RedisJSON/RedisJSON/)

RedisJSON is a [Redis](https://redis.io/) module that provides JSON support in Redis. RedisJSON lets your store, update, and retrieve JSON values in Redis just as you would with any other Redis data type. RedisJSON also works seamlessly with [RediSearch](https://redis.io/docs/stack/search/) to let you index and query your JSON documents.

## Primary features

* Full support for the JSON standard
* A [JSONPath](http://goessner.net/articles/JsonPath/) syntax for selecting/updating elements inside documents
* Documents stored as binary data in a tree structure, allowing fast access to sub-elements
* Typed atomic operations for all JSON values types

## Using RedisJSON

To learn how to use RedisJSON, it's best to start with the Redis CLI. The following examples assume that you're connected to a Redis server with RedisJSON enabled.

### With `redis-cli`

To following along, start [`redis-cli`](http://redis.io/topics/rediscli).

The first RedisJSON command to try is `JSON.SET`, which sets a Redis key with a JSON value. All JSON values can be used, for example a string:

```
127.0.0.1:6379> JSON.SET foo $ '"bar"'
OK
127.0.0.1:6379> JSON.GET foo $
"[\"bar\"]"
127.0.0.1:6379> JSON.TYPE foo $
1) string
```

`JSON.GET` and `JSON.TYPE` do literally that regardless of the value's type, but you should really check out `JSON.GET` prettifying powers. Note how the commands are given the dollar sign character, i.e. `$`. This is the [path](/redisjson/path) to the value in the RedisJSON data type (in this case it just means the root). A couple more string operations:

```
127.0.0.1:6379> JSON.STRLEN foo $
1) (integer) 3
127.0.0.1:6379> JSON.STRAPPEND foo $ '"baz"'
1) (integer) 6
127.0.0.1:6379> JSON.GET foo $
"[\"barbaz\"]"

``` 

`JSON.STRLEN` tells you the length of the string, and you can append another string to it with `JSON.STRAPPEND`. Numbers can be [incremented](/commands/json.numincrby) and [multiplied](/commands/json.nummultby):

```
127.0.0.1:6379> JSON.SET num $ 0
OK
127.0.0.1:6379> JSON.NUMINCRBY num $ 1
"[1]"
127.0.0.1:6379> JSON.NUMINCRBY num $ 1.5
"[2.5]"
127.0.0.1:6379> JSON.NUMINCRBY num $ -0.75
"[1.75]"
127.0.0.1:6379> JSON.NUMMULTBY num $ 24
"[42]"
```

Of course, a more interesting example would involve an array or maybe an object:

```
127.0.0.1:6379> JSON.SET amoreinterestingexample $ '[ true, { "answer": 42 }, null ]'
OK
127.0.0.1:6379> JSON.GET amoreinterestingexample $
"[[true,{\"answer\":42},null]]"
127.0.0.1:6379> JSON.GET amoreinterestingexample $[1].answer
"[42]"
127.0.0.1:6379> JSON.DEL amoreinterestingexample $[-1]
(integer) 1
127.0.0.1:6379> JSON.GET amoreinterestingexample $
"[[true,{\"answer\":42}]]"
```

The handy `JSON.DEL` command deletes anything you tell it to. Arrays can be manipulated with a dedicated subset of RedisJSON commands:

```
127.0.0.1:6379> JSON.SET arr $ []
OK
127.0.0.1:6379> JSON.ARRAPPEND arr $ 0
1) (integer) 1
127.0.0.1:6379> JSON.GET arr $
"[[0]]"
127.0.0.1:6379> JSON.ARRINSERT arr $ 0 -2 -1
1) (integer) 3
127.0.0.1:6379> JSON.GET arr $
"[[-2,-1,0]]"
127.0.0.1:6379> JSON.ARRTRIM arr $ 1 1
1) (integer) 1
127.0.0.1:6379> JSON.GET arr $
"[[-1]]"
127.0.0.1:6379> JSON.ARRPOP arr $
1) "-1"
127.0.0.1:6379> JSON.ARRPOP arr $
1) (nil)
```

And objects have their own commands too:

```
127.0.0.1:6379> JSON.SET obj $ '{"name":"Leonard Cohen","lastSeen":1478476800,"loggedOut": true}'
OK
127.0.0.1:6379> JSON.OBJLEN obj $
1) (integer) 3
127.0.0.1:6379> JSON.OBJKEYS obj $
1) 1) "name"
   2) "lastSeen"
   3) "loggedOut"
```

#### Python example

This code snippet shows how to use RedisJSON with raw Redis commands from Python with [redis-py](https://github.com/redis/redis-py):

```Python
import redis

data = {
    'foo': {
        'bar' : 'baz'
    }
}

r = redis.Redis()
r.json().set('doc', '$', data)
doc = r.json().get('doc', '$')
foo = r.json().get('doc', '$.foo')
bar = r.json().get('doc', '$..bar')
```

### Build from source

To build RedisJSON from the source code:

1. Clone the [RedisJSON repository](https://github.com/RedisJSON/RedisJSON) (make sure you include the `--recursive` option to properly clone submodules):

    ```sh
    $ git clone --recursive https://github.com/RedisJSON/RedisJSON.git
    $ cd RedisJSON
    ```

1. Install dependencies:

    ```sh
    $ make setup
    ```

1. Build:
    ```sh
    $ make build
    ```

### Loading the module to Redis

Requirements:

Generally, it is best to run the latest Redis version.

If your OS has a [Redis 6.x package or later](http://redis.io/download), you can install it using the OS package manager.

Otherwise, you can invoke ./deps/readies/bin/getredis.

Run Redis with RedisJSON:

```sh
$ redis-server --loadmodule /path/to/module/target/release/librejson.so
```

Or you can have Redis load the module during startup by adding the following to your `redis.conf` file:

```
loadmodule /path/to/module/target/release/librejson.so
```

On Mac OS, if this module has been built as a dynamic library use:

```
loadmodule /path/to/module/target/release/librejson.dylib
```

In the above lines replace `/path/to/module/` with the actual path to the module's library.

Alternatively, you can download and run RedisJSON from a precompiled binary:

1. Download a precompiled version of RedisJSON from the [Redis download center](https://redis.com/download-center/modules/).

1. Load RedisJSON:

    ```sh
    $ redis-server --loadmodule /path/to/library/librejson.so
    ```
```

Lastly, you can also use the [`MODULE LOAD`](/commands/module-load) command. Note, however, that `MODULE LOAD` is a **dangerous command** and may be blocked/deprecated in the future due to security considerations.

Once the module has been loaded successfully, the Redis log should have lines similar to:

```
...
1877:M 23 Dec 02:02:59.725 # <RedisJSON> JSON data type for Redis - v1.0.0 [encver 0]
1877:M 23 Dec 02:02:59.725 * Module 'RedisJSON' loaded from <redacted>/src/rejson.so
...
```
