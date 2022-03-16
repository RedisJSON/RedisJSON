---
title: RedisJSON - a JSON data type for Redis
linkTitle: RedisJSON
type: docs
---

<img src="images/logo.svg" alt="logo" width="200"/>

[![Forum](https://img.shields.io/badge/Forum-RedisJSON-blue)](https://forum.redis.com/c/modules/redisjson)
[![Discord](https://img.shields.io/discord/697882427875393627?style=flat-square)](https://discord.gg/QUkjSsk)

RedisJSON is a [Redis](https://redis.io/) module that implements [ECMA-404 The JSON Data Interchange Standard](http://json.org/) as a native data type. It allows storing, updating and fetching JSON values from Redis keys (documents).

Primary features:

* Full support of the JSON standard
* [JSONPath](http://goessner.net/articles/JsonPath/)-like syntax for selecting elements inside documents
* Documents are stored as binary data in a tree structure, allowing fast access to sub-elements
* Typed atomic operations for all JSON values types

RedisJSON is developed with <3 at [Redis](https://redis.com). The source code is available
at: [https://github.com/RedisJSON/RedisJSON](https://github.com/RedisJSON/RedisJSON)

## Quickstart

1.  [Create a free database in Redis Cloud](#redis-cloud)
1.  [Launch RedisJSON with Docker](#launch-redisjson-with-docker)
1.  [Use it from **any** Redis client](#using-redisjson), e.g.:

![RedisJSON with `redis-cli`](images/demo.gif)

Alternatively, you can also build and load the module yourself. [Build and Load the RedisJSON module library](#building-and-loading-the-module)

## Redis Cloud

RedisJSON is available on all Redis Cloud managed services, including a completely free tier up to 30MB!

[Get started here](https://redis.com/redis-enterprise-cloud/pricing/)

## Launch RedisJSON with Docker
Run the following on Windows, MacOS or Linux with Docker.
```
docker run -p 6379:6379 --name redis-redisjson redislabs/rejson:latest
```

## Using RedisJSON

Before using RedisJSON, you should familiarize yourself with its commands and syntax as detailed in the [commands reference](/redisjson/commands) document. However, to quickly get started just review this section and get:

1.  A Redis server running the module (see [building](#building-on-ubuntu-2004) and [loading](#loading-the-module-to-redis) for instructions)
1.  Any [Redis](http://redis.io/clients) or [RedisJSON client](#client-libraries)

### With `redis-cli`

This example will use [`redis-cli`](http://redis.io/topics/rediscli) as the Redis client. The first RedisJSON command to try out is `JSON.SET`, which sets a Redis key with a JSON value. All JSON values can be used, for example a string:

```
127.0.0.1:6379> JSON.SET foo $ '"bar"'
OK
127.0.0.1:6379> JSON.GET foo $
"[\"bar\"]"
127.0.0.1:6379> JSON.TYPE foo $
1) string
```

`JSON.GET` and `JSON.TYPE` do literally that regardless of the value's type, but you should really check out `JSON.GET` prettifying powers. Note how the commands are given the period character, i.e. `.`. This is the [path](/redisjson/path) to the value in the RedisJSON data type (in this case it just means the root). A couple more string operations:

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

### With any other client

Unless your [Redis client](http://redis.io/clients) already supports Redis modules (unlikely) or RedisJSON specifically (even more unlikely), you should be okay using its ability to send raw Redis commands. Depending on your client of choice, the exact method for doing that may vary.

#### Python example

This code snippet shows how to use RedisJSON with raw Redis commands from Python with [redis-py](https://github.com/redis/redis-py):

```Python
import redis
import json

data = {
    'foo': 'bar'
}

r = redis.StrictRedis()
r.json().set('doc', '$', json.dumps(data))
reply = json.loads(r.json().get('doc', '$'))
```

## Download and running binaries

First download the pre-compiled version from [Redis download center](https://redis.com/download-center/modules/).

Next, run Redis with RedisJSON:

```
$ redis-server --loadmodule /path/to/module/rejson.so
```

## Building and Loading the Module

```
cargo build --release
```

### Building on Ubuntu 20.04

The following packages are required to successfully build on Ubuntu 20.04:

```
sudo apt install build-essential llvm cmake libclang1 libclang-dev cargo
```
Then, run `make` or `cargo build --release` in the repository directory

### Loading the module to Redis

Requirements:

* [Redis v6.0 or above](http://redis.io/download)

We recommend you have Redis load the module during startup by adding the following to your `redis.conf` file:

```
loadmodule /path/to/module/target/release/librejson.so
```

On Mac OS, if this module has been built as a dynamic library use:

```
loadmodule /path/to/module/target/release/librejson.dylib
```

In the above lines replace `/path/to/module/` with the actual path to the module's library.

Alternatively, you can have Redis load the module using the following command line argument syntax:

```bash
~/$ redis-server --loadmodule ./target/release/librejson.so
```

Lastly, you can also use the [`MODULE LOAD`](/commands/module-load) command. Note, however, that `MODULE LOAD` is a **dangerous command** and may be blocked/deprecated in the future due to security considerations.

Once the module has been loaded successfully, the Redis log should have lines similar to:

```
...

1877:M 23 Dec 02:02:59.725 # <RedisJSON> JSON data type for Redis - v1.0.0 [encver 0]
1877:M 23 Dec 02:02:59.725 * Module 'RedisJSON' loaded from <redacted>/src/rejson.so
...
```

