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
* `<query>` - syntax is based on [RediSearch query syntax](https://oss.redislabs.com/redisearch/Query_Syntax/)

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

Make sure you have Rust installed:
https://www.rust-lang.org/tools/install

Then, build as usual:

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

| Project | Language | License | Author | Stars | Package |
| ------- | -------- | ------- | ------ | ----- | --- |
| [iorejson][iorejson-url] | Node.js | MIT | [Evan Huang @evanhuang8][iorejson-author] | [![iorejson-stars]][iorejson-url] | [npm][iorejson-package] |
| [node_redis-rejson][node_redis-rejson-url] | Node.js | MIT | [Kyle Davis @stockholmux][node_redis-rejson-author] | [![node_redis-rejson-stars]][node_redis-rejson-url] | [npm][node_redis-rejson-package]  |
| [redis-modules-sdk][redis-modules-sdk-url] | Node.js | BSD-3-Clause | [Dani Tseitlin @danitseitlin][redis-modules-sdk-author] | [![redis-modules-sdk-stars]][redis-modules-sdk-url] | [npm][redis-modules-sdk-package] |
| [JRedisJSON][JRedisJSON-url] | Java | BSD-2-Clause | [Redis Labs][JRedisJSON-author] | [![JRedisJSON-stars]][JRedisJSON-url] | [maven][JRedisJSON-package] |
| [redisjson-py][rejson-py-url]  | Python | BSD-2-Clause | [Redis Labs][rejson-py-author] | [![rejson-py-stars]][rejson-py-url] | [pypi][rejson-py-package] |
| [go-rejson][go-rejson-url] | Go | MIT | [Nitish Malhotra @nitishm][go-rejson-author] | [![go-rejson-stars]][go-rejson-url] | |
| [rejonson][rejonson-url] | Go | Apache-2.0 | [Daniel Krom @KromDaniel][rejonson-author] | [![rejonson-stars]][rejonson-url] | |
| [NReJSON][NReJSON-url]  | .NET | MIT/Apache-2.0 | [Tommy Hanks @tombatron][NReJSON-author] | [![NReJSON-stars]][NReJSON-url] | [nuget][NReJSON-package] |
| [phpredis-json][phpredis-json-url]  | PHP | MIT | [Rafa Campoy @averias][phpredis-json-author] | [![phpredis-json-stars]][phpredis-json-url] | [composer][phpredis-json-package] |
| [redislabs-rejson][redislabs-rejson-url]  | PHP | MIT | [Mehmet Korkmaz @mkorkmaz][redislabs-rejson-author] | [![redislabs-rejson-stars]][redislabs-rejson-url] | [composer][redislabs-rejson-package] |
| [rejson-rb][rejson-rb-url]  | Ruby | MIT | [Pavan Vachhani @vachhanihpavan][rejson-rb-author] | [![rejson-rb-stars]][rejson-rb-url] | [rubygems][rejson-rb-package]|

[iorejson-author]: https://github.com/evanhuang8
[iorejson-url]: https://github.com/evanhuang8/iorejson
[iorejson-package]: https://www.npmjs.com/package/iorejson
[iorejson-stars]: https://img.shields.io/github/stars/evanhuang8/iorejson.svg?style=social&amp;label=Star&amp;maxAge=2592000

[node_redis-rejson-author]: https://github.com/stockholmux
[node_redis-rejson-url]: https://github.com/stockholmux/node_redis-rejson
[node_redis-rejson-package]: https://www.npmjs.com/package/redis-rejson
[node_redis-rejson-stars]: https://img.shields.io/github/stars/stockholmux/node_redis-rejson.svg?style=social&amp;label=Star&amp;maxAge=2592000

[redis-modules-sdk-author]: https://github.com/danitseitlin/
[redis-modules-sdk-url]: https://github.com/danitseitlin/redis-modules-sdk
[redis-modules-sdk-package]: https://www.npmjs.com/package/redis-modules-sdk
[redis-modules-sdk-stars]: https://img.shields.io/github/stars/danitseitlin/redis-modules-sdk.svg?style=social&amp;label=Star&amp;maxAge=2592000

[JRedisJSON-author]: https://redislabs.com
[JRedisJSON-url]: https://github.com/RedisJSON/JRedisJSON
[JRedisJSON-package]: https://search.maven.org/artifact/com.redislabs/jrejson/1.2.0/jar
[JRedisJSON-stars]: https://img.shields.io/github/stars/RedisJSON/JRedisJSON.svg?style=social&amp;label=Star&amp;maxAge=2592000

[rejson-py-author]: https://redislabs.com
[rejson-py-url]: https://github.com/RedisJSON/redisjson-py
[rejson-py-package]: https://pypi.python.org/pypi/rejson
[rejson-py-stars]: https://img.shields.io/github/stars/RedisJSON/redisjson-py.svg?style=social&amp;label=Star&amp;maxAge=2592000

[go-rejson-author]: https://github.com/nitishm
[go-rejson-url]: https://github.com/nitishm/go-rejson/
[go-rejson-package]: https://www.npmjs.com/package/iorejson
[go-rejson-stars]: https://img.shields.io/github/stars/nitishm/go-rejson.svg?style=social&amp;label=Star&amp;maxAge=2592000

[rejonson-author]: https://github.com/KromDaniel
[rejonson-url]: https://github.com/KromDaniel/rejonson
[rejonson-package]: https://www.npmjs.com/package/iorejson
[rejonson-stars]: https://img.shields.io/github/stars/KromDaniel/rejonson?style=social&amp;label=Star&amp;maxAge=2592000

[NReJSON-author]: https://github.com/tombatron
[NReJSON-url]: https://github.com/tombatron/NReJSON
[NReJSON-package]: https://www.nuget.org/packages/NReJSON/
[NReJSON-stars]: https://img.shields.io/github/stars/tombatron/NReJSON.svg?style=social&amp;label=Star&amp;maxAge=2592000

[phpredis-json-author]: https://github.com/averias
[phpredis-json-url]: https://github.com/averias/phpredis-json
[phpredis-json-package]: https://packagist.org/packages/averias/phpredis-json
[phpredis-json-stars]: https://img.shields.io/github/stars/averias/phpredis-json.svg?style=social&amp;label=Star&amp;maxAge=2592000

[redislabs-rejson-author]: https://github.com/mkorkmaz
[redislabs-rejson-url]: https://github.com/mkorkmaz/redislabs-rejson
[redislabs-rejson-package]: https://packagist.org/packages/mkorkmaz/redislabs-rejson
[redislabs-rejson-stars]: https://img.shields.io/github/stars/mkorkmaz/redislabs-rejson.svg?style=social&amp;label=Star&amp;maxAge=2592000

[rejson-rb-author]: https://github.com/vachhanihpavan
[rejson-rb-url]: https://github.com/vachhanihpavan/rejson-rb
[rejson-rb-package]: https://rubygems.org/gems/rejson-rb
[rejson-rb-stars]: https://img.shields.io/github/stars/vachhanihpavan/rejson-rb.svg?style=social&amp;label=Star&amp;maxAge=2592000


## Acknowledgements

RedisJSON is developed with <3 at [Redis Labs](https://redislabs.com).

RedisJSON is made possible only because of the existance of this amazing open source project:

* [redis](https://github.com/antirez/redis)

## License

Redis Source Available License Agreement - see [LICENSE](LICENSE)
