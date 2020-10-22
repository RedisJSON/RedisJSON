[![GitHub issues](https://img.shields.io/github/release/RedisJSON/RedisJSON.svg)](https://github.com/RedisJSON/RedisJSON/releases/latest)
[![CircleCI](https://circleci.com/gh/RedisJSON/RedisJSON/tree/master.svg?style=svg)](https://circleci.com/gh/RedisJSON/RedisJSON/tree/master)
[![macos](https://github.com/RedisJSON/RedisJSON/workflows/macos/badge.svg)](https://github.com/RedisJSON/RedisJSON/actions?query=workflow%3Amacos)
[![Docker Cloud Build Status](https://img.shields.io/docker/cloud/build/redislabs/rejson.svg)](https://hub.docker.com/r/redislabs/rejson/builds/)
[![Forum](https://img.shields.io/badge/Forum-RedisJSON-blue)](https://forum.redislabs.com/c/modules/redisjson)
[![Discord](https://img.shields.io/discord/697882427875393627?style=flat-square)](https://discord.gg/QUkjSsk)

# RedisJSON

RedisJSON is a [Redis](https://redis.io/) module that implements [ECMA-404 The JSON Data Interchange Standard](https://json.org/) as a native data type. It allows storing, updating and fetching JSON values from Redis keys (documents).

## Primary features:

* Full support of the JSON standard
* [JSONPath](https://goessner.net/articles/JsonPath/) syntax for selecting elements inside documents
* Documents are stored as binary data in a tree structure, allowing fast access to sub-elements
* Typed atomic operations for all JSON values types
* Secondary index support based on [RediSearch](https://redisearch.io)

## Quick start

```
docker run -p 6379:6379 --name redis-redisjson redislabs/rejson:latest
```

## Documentation

Read the docs at http://redisjson.io


## New Commands in RedisJSON

    JSON.INDEX ADD <index> <field> <path>
    JSON.INDEX DEL <index>
    JSON.QGET <index> <query> <path>

* `<index>` - user defined index name
* `<path>` - [JSONPath](https://goessner.net/articles/JsonPath/) syntax for selecting elements inside documents
* `<query>` - sytanx is based on [RediSearch query syntax](https://oss.redislabs.com/redisearch/Query_Syntax/)

### Next Milestone
    JSON.QSET <index> <query> <path> <json> [NX | XX]
    JSON.QDEL <index> <query> <path>
    
    JSON.INDEX DEL <index> <field>
    JSON.INDEX INFO <index> <field>

Return value from JSON.QGET is an array of keys and values:

    key
    json
    key
    json

In a language such as Java this could be represented as a `Map<String, Document>`.
    
## Examples

A query combining multiple paths:
    
    JSON.QGET mytype "@path1:hello @path2:world" d.name
    
    
```bash
127.0.0.1:6379> json.set user1 $ '{"last":"Joe", "first":"Mc"}' INDEX person
OK
127.0.0.1:6379> json.set user2 $ '{"last":"Joan", "first":"Mc"}' INDEX person
OK
127.0.0.1:6379> json.index add person last $.last
OK
127.0.0.1:6379> JSON.QGET person Jo*
"{\"user2\":[{\"last\":\"Joan\",\"first\":\"Mc\"}],\"user1\":[{\"last\":\"Joe\",\"first\":\"Mc\"}]}"
127.0.0.1:6379> json.set user3 $ '{"last":"Joel", "first":"Dan"}' INDEX person
OK
127.0.0.1:6379> JSON.QGET person Jo*
"{\"user2\":[{\"last\":\"Joan\",\"first\":\"Mc\"}],\"user1\":[{\"last\":\"Joe\",\"first\":\"Mc\"}],\"user3\":[{\"last\":\"Joel\",\"first\":\"Dan\"}]}"
127.0.0.1:6379> json.index add person first $.first
OK
127.0.0.1:6379> JSON.QGET person Mc
"{\"user2\":[{\"last\":\"Joan\",\"first\":\"Mc\"}],\"user1\":[{\"last\":\"Joe\",\"first\":\"Mc\"}]}"
127.0.0.1:6379> JSON.QGET person Mc $.last
"{\"user2\":[\"Joan\"],\"user1\":[\"Joe\"]}"
127.0.0.1:6379> JSON.QGET person "@last:Jo* @first:Mc" $.last
"{\"user2\":[\"Joan\"],\"user1\":[\"Joe\"]}"
```

## Build

```bash
cargo build --release
```

## Run

### Linux

```
redis-server --loadmodule ./target/release/librejson.so
```

### Mac OS

```
redis-server --loadmodule ./target/release/librejson.dylib
```

## Client libraries

Some languages have client libraries that provide support for RedisJSON's commands:

| Project | Language | License | Author | URL |
| ------- | -------- | ------- | ------ | --- |
| iorejson | Node.js | MIT | [Evan Huang @evanhuang8](https://github.com/evanhuang8) | [git](https://github.com/evanhuang8/iorejson) [npm](https://www.npmjs.com/package/iorejson) |
| node_redis-rejson | Node.js | MIT | [Kyle Davis @stockholmux](https://github.com/stockholmux) | [git](https://github.com/stockholmux/node_redis-rejson) [npm](https://www.npmjs.com/package/redis-rejson) |
| JReJSON | Java | BSD-2-Clause | [Redis Labs](https://redislabs.com) | [git](https://github.com/RedisLabs/JReJSON/) |
| rejson-py | Python | BSD-2-Clause | [Redis Labs](https://redislabs.com) | [git](https://github.com/RedisLabs/rejson-py) [pypi](https://pypi.python.org/pypi/rejson) |
| go-rejson (multiple clients) | Go | MIT | [Nitish Malhotra @nitishm](https://github.com/nitishm) | [git](https://github.com/nitishm/go-rejson/) |
| jonson  (go-redis client)| Go | Apache-2.0 | [Daniel Krom @KromDaniel](https://github.com/KromDaniel) | [git](https://github.com/KromDaniel/rejonson) |
| NReJSON | .NET | MIT/Apache-2.0 | [Tommy Hanks @tombatron](https://github.com/tombatron) | [git](https://github.com/tombatron/NReJSON) |
| phpredis-json | PHP | MIT | [Rafa Campoy @averias](https://github.com/averias/) | [git](https://github.com/averias/phpredis-json) |
| redislabs-rejson | PHP | MIT | [Mehmet Korkmaz @mkorkmaz](https://github.com/mkorkmaz) | [git](https://github.com/mkorkmaz/redislabs-rejson/) |
| rejson-rb | Ruby | MIT | [Pavan Vachhani @vachhanihpavan](https://github.com/vachhanihpavan/) | [git](https://github.com/vachhanihpavan/rejson-rb) [rubygems](https://rubygems.org/gems/rejson-rb)|

## Acknowledgements

RedisJSON is developed with <3 at [Redis Labs](https://redislabs.com).

RedisJSON is made possible only because of the existance of this amazing open source project:

* [redis](https://github.com/antirez/redis)

## License

Redis Source Available License Agreement - see [LICENSE](LICENSE)
