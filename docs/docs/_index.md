---
title: JSON
description: JSON support for Redis
linkTitle: JSON
type: docs
---

[![Discord](https://img.shields.io/discord/697882427875393627?style=flat-square)](https://discord.gg/QUkjSsk)
[![Github](https://img.shields.io/static/v1?label=&message=repository&color=5961FF&logo=github)](https://github.com/RedisJSON/RedisJSON/)

The Redis JSON module provides JavaScript Object Notation support for Redis. Redis JSON lets you store, update, and retrieve JSON values in a Redis database, similar to any other Redis data type. Redis JSON also works seamlessly with [Search and Query](https://redis.io/docs/stack/search/) to let you [index and query JSON documents](https://redis.io/docs/stack/search/indexing_json).

## Primary features

* Full support for the JSON standard
* A [JSONPath](http://goessner.net/articles/JsonPath/) syntax for selecting/updating elements inside documents (see [JSONPath syntax](/redisjson/path#jsonpath-syntax))
* Documents stored as binary data in a tree structure, allowing fast access to sub-elements
* Typed atomic operations for all JSON value types

## Use Redis JSON

To learn how to use JSON, it's best to start with the Redis CLI. The following examples assume that you're connected to a Redis server with JSON enabled.

### `redis-cli` examples

First, start [`redis-cli`](http://redis.io/topics/rediscli) in interactive mode.

The first JSON command to try is `JSON.SET`, which sets a Redis key with a JSON value. `JSON.SET` accepts all JSON value types. This example creates a JSON string:

```sh
127.0.0.1:6379> JSON.SET animal $ '"dog"'
"OK"
127.0.0.1:6379> JSON.GET animal $
"[\"dog\"]"
127.0.0.1:6379> JSON.TYPE animal $
1) "string"
```

Note how the commands include the dollar sign character `$`. This is the [path](/redisjson/path) to the value in the JSON document (in this case it just means the root).

Here are a few more string operations. `JSON.STRLEN` tells you the length of the string, and you can append another string to it with `JSON.STRAPPEND`.

```sh
127.0.0.1:6379> JSON.STRLEN animal $
1) "3"
127.0.0.1:6379> JSON.STRAPPEND animal $ '" (Canis familiaris)"'
1) "22"
127.0.0.1:6379> JSON.GET animal $
"[\"dog (Canis familiaris)\"]"
``` 

Numbers can be [incremented](/commands/json.numincrby) and [multiplied](/commands/json.nummultby):

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

Here's a more interesting example that includes JSON arrays and objects:

```
127.0.0.1:6379> JSON.SET example $ '[ true, { "answer": 42 }, null ]'
OK
127.0.0.1:6379> JSON.GET example $
"[[true,{\"answer\":42},null]]"
127.0.0.1:6379> JSON.GET example $[1].answer
"[42]"
127.0.0.1:6379> JSON.DEL example $[-1]
(integer) 1
127.0.0.1:6379> JSON.GET example $
"[[true,{\"answer\":42}]]"
```

The `JSON.DEL` command deletes any JSON value you specify with the `path` parameter.

You can manipulate arrays with a dedicated subset of JSON commands:

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

JSON objects also have their own commands:

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

To return a JSON response in a more human-readable format, run `redis-cli` in raw output mode and include formatting keywords such as `INDENT`, `NEWLINE`, and `SPACE` with the `JSON.GET` command:

```sh
$ redis-cli --raw
127.0.0.1:6379> JSON.GET obj INDENT "\t" NEWLINE "\n" SPACE " " $
[
	{
		"name": "Leonard Cohen",
		"lastSeen": 1478476800,
		"loggedOut": true
	}
]
```

### Python example

This code snippet shows how to use JSON with raw Redis commands from Python with [redis-py](https://github.com/redis/redis-py):

```Python
import redis

data = {
    'dog': {
        'scientific-name' : 'Canis familiaris'
    }
}

r = redis.Redis()
r.json().set('doc', '$', data)
doc = r.json().get('doc', '$')
dog = r.json().get('doc', '$.dog')
scientific_name = r.json().get('doc', '$..scientific-name')
```

### Run with Docker

To run JSON with Docker, use the `redis-stack-server` Docker image:

```sh
$ docker run -d --name redis-stack-server -p 6379:6379 redis/redis-stack-server:latest
```

For more information about running Redis Stack in a Docker container, see [Run Redis Stack on Docker](/docs/stack/get-started/install/docker/).

### Download binaries

To download and run Redis JSON from a precompiled binary:

1. Download a precompiled version of Search and Query from the [Redis download center](https://redis.com/download-center/modules/).

1. Run Redis with JSON:

    ```sh
    $ redis-server --loadmodule /path/to/module/src/rejson.so
    ```

### Build from source

To build JSON from the source code:

1. Clone the [JSON repository](https://github.com/RedisJSON/RedisJSON) (make sure you include the `--recursive` option to properly clone submodules):

    ```sh
    $ git clone --recursive https://github.com/RedisJSON/RedisJSON.git
    $ cd RedisJSON
    ```

1. Install dependencies:

    ```sh
    $ ./sbin/setup
    ```

1. Build:
    ```sh
    $ make build
    ```

### Load the module to Redis

Requirements:

Generally, it is best to run the latest Redis version.

If your OS has a [Redis 6.x package or later](http://redis.io/download), you can install it using the OS package manager.

Otherwise, you can invoke 

```sh
$ ./deps/readies/bin/getredis
```

To load the JSON module, use one of the following methods:

* [Makefile recipe](#makefile-recipe)
* [Configuration file](#configuration-file)
* [Command-line option](#command-line-option)
* [MODULE LOAD command](/commands/module-load/)

#### Makefile recipe

Run Redis with JSON:

```sh
$ make run
```

#### Configuration file

Or you can have Redis load the module during startup by adding the following to your `redis.conf` file:

```
loadmodule /path/to/module/target/release/librejson.so
```

On Mac OS, if this module was built as a dynamic library, run:

```
loadmodule /path/to/module/target/release/librejson.dylib
```

In the above lines replace `/path/to/module/` with the actual path to the module.

Alternatively, you can download and run the JSON module from a precompiled binary:

1. Download a precompiled version of the JSON module from the [Redis download center](https://redis.com/download-center/modules/).

#### Command-line option

Alternatively, you can have Redis load the module using the following command-line argument syntax:

 ```bash
 $ redis-server --loadmodule /path/to/module/librejson.so
 ```

In the above lines replace `/path/to/module/` with the actual path to the module's library.

#### `MODULE LOAD` command

You can also use the `MODULE LOAD` command to load JSON. Note that `MODULE LOAD` is a **dangerous command** and may be blocked/deprecated in the future due to security considerations.

After the module has been loaded successfully, the Redis log should have lines similar to:

```
...
9:M 11 Aug 2022 16:24:06.701 * <ReJSON> version: 20009 git sha: d8d4b19 branch: HEAD
9:M 11 Aug 2022 16:24:06.701 * <ReJSON> Exported RedisJSON_V1 API
9:M 11 Aug 2022 16:24:06.701 * <ReJSON> Enabled diskless replication
9:M 11 Aug 2022 16:24:06.701 * <ReJSON> Created new data type 'ReJSON-RL'
9:M 11 Aug 2022 16:24:06.701 * Module 'ReJSON' loaded from /opt/redis-stack/lib/rejson.so
...
```

### Limitation

A JSON value passed to a command can have a depth of up to 128. If you pass to a command a JSON value that contains an object or an array with a nesting level of more than 128, the command returns an error.
