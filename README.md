[![GitHub issues](https://img.shields.io/github/release/RedisJSON/RedisJSON.svg)](https://github.com/RedisJSON/RedisJSON/releases/latest)
[![CircleCI](https://circleci.com/gh/RedisJSON/RedisJSON/tree/master.svg?style=svg)](https://circleci.com/gh/RedisJSON/RedisJSON/tree/master)
[![macos](https://github.com/RedisJSON/RedisJSON/workflows/macos/badge.svg)](https://github.com/RedisJSON/RedisJSON/actions?query=workflow%3Amacos)
[![Dockerhub](https://img.shields.io/docker/pulls/redis/redis-stack-server?label=redis-stack-server)](https://img.shields.io/docker/pulls/redis/redis-stack-server)
[![Codecov](https://codecov.io/gh/RedisJSON/RedisJSON/branch/master/graph/badge.svg)](https://codecov.io/gh/RedisJSON/RedisJSON)

# RedisJSON
[![Forum](https://img.shields.io/badge/Forum-RedisJSON-blue)](https://forum.redislabs.com/c/modules/redisjson)
[![Discord](https://img.shields.io/discord/697882427875393627?style=flat-square)](https://discord.gg/QUkjSsk)


RedisJSON is a [Redis](https://redis.io/) module that implements [ECMA-404 The JSON Data Interchange Standard](https://json.org/) as a native data type. It allows storing, updating and fetching JSON values from Redis keys (documents).

## Primary features:

* Full support of the JSON standard
* [JSONPath](https://goessner.net/articles/JsonPath/) syntax for selecting elements inside documents
* Documents are stored as binary data in a tree structure, allowing fast access to sub-elements
* Typed atomic operations for all JSON values types
* Secondary index support when combined with [RediSearch](https://redisearch.io)

## Quick start

```
docker run -p 6379:6379 --name redis-stack redis/redis-stack:latest
```

## Documentation

Read the docs at http://redisjson.io

## Build

Make sure you have Rust installed:
https://www.rust-lang.org/tools/install

Then, build as usual:

```bash
cargo build --release
```

When running the tests, you need to explicitly specify the `test` feature to disable use of the Redis memory allocator when testing:

```bash
cargo test --features test
```

If you forget to do this, you'll see an error mentioning `signal: 4, SIGILL: illegal instruction`.

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

| Project | Language | License | Author | Stars | Package | Comment |
| ------- | -------- | ------- | ------ | ----- | ------- | ------- |
| [jedis][jedis-url] | Java | MIT | [Redis][redis-url] | ![Stars][jedis-stars] | [Maven][jedis-package]||
| [redis-py][redis-py-url] | Python | MIT | [Redis][redis-url] | ![Stars][redis-py-stars] | [pypi][redis-py-package]||
| [node-redis][node-redis-url] | Node.js | MIT | [Redis][redis-url] | ![Stars][node-redis-stars] | [npm][node-redis-package]||
| [nredisstack][nredisstack-url] | .NET | MIT | [Redis][redis-url] | ![Stars][nredisstack-stars] | [nuget][nredisstack-package]||
| [redis-om-python][redis-om-python-url]  | python | BSD-3-Clause | [Redis][redis-om-python-author] | [![redis-om-python-stars]][redis-om-python-url] | [PyPi][redis-om-python-package] |
| [Redisson][Redisson-url] | Java | Apache-2.0 | [Redisson][Redisson-author] | [![Redisson-stars]][Redisson-url] | [Maven][Redisson-package] |
| [redis-modules-java][redis-modules-java-url] | Java | Apache-2.0 | [Liming Deng @dengliming][redis-modules-java-author] | [![redis-modules-java-stars]][redis-modules-java-url] | [maven][redis-modules-java-package] |
| [redis-om-spring][redis-om-spring-url]  | Java | BSD-3-Clause | [Redis][redis-om-spring-author] | [![redis-om-spring-stars]][redis-om-spring-url] | |
| [iorejson][iorejson-url] | Node.js | MIT | [Evan Huang @evanhuang8][iorejson-author] | [![iorejson-stars]][iorejson-url] | [npm][iorejson-package] |
| [ioredis-rejson][ioredis-rejson-url] | Node.js | MIT | [Felipe Schulz @schulzf][ioredis-rejson-author] | [![ioredis-rejson-stars]][ioredis-rejson-url] | [npm][ioredis-rejson-package] |
| [redis-om-node][redis-om-node-url]  | Node | BSD-3-Clause | [Redis][redis-om-node-author] | [![redis-om-node-stars]][redis-om-node-url] | [npm][redis-om-node-package] |
| [go-rejson][go-rejson-url] | Go | MIT | [Nitish Malhotra @nitishm][go-rejson-author] | [![go-rejson-stars]][go-rejson-url] | |
| [rejonson][rejonson-url] | Go | Apache-2.0 | [Daniel Krom @KromDaniel][rejonson-author] | [![rejonson-stars]][rejonson-url] | |
| [rueidis][rueidis-url] | Go | Apache-2.0 | [Rueian @rueian][rueidis-author] | [![rueidis-stars]][rueidis-url] | |
| [NReJSON][NReJSON-url]  | .NET | MIT/Apache-2.0 | [Tommy Hanks @tombatron][NReJSON-author] | [![NReJSON-stars]][NReJSON-url] | [nuget][NReJSON-package] |
| [redis-om-dotnet][redis-om-dotnet-url]  | .NET | BSD-3-Clause | [Redis][redis-om-dotnet-author] | [![redis-om-dotnet-stars]][redis-om-dotnet-url] | [nuget][redis-om-dotnet-package] |
| [phpredis-json][phpredis-json-url]  | PHP | MIT | [Rafa Campoy @averias][phpredis-json-author] | [![phpredis-json-stars]][phpredis-json-url] | [composer][phpredis-json-package] |
| [redislabs-rejson][redislabs-rejson-url]  | PHP | MIT | [Mehmet Korkmaz @mkorkmaz][redislabs-rejson-author] | [![redislabs-rejson-stars]][redislabs-rejson-url] | [composer][redislabs-rejson-package] |
| [rejson-rb][rejson-rb-url]  | Ruby | MIT | [Pavan Vachhani @vachhanihpavan][rejson-rb-author] | [![rejson-rb-stars]][rejson-rb-url] | [rubygems][rejson-rb-package]|
| [rustis][rustis-url] | Rust | MIT | [Dahomey Technologies][rustis-author] | [![rustis-stars]][rustis-url] | [crate][rustis-package]| [Documentation](https://docs.rs/rustis/latest/rustis/commands/trait.JsonCommands.html) |

[redis-url]: https://redis.com
[Jedis-url]: https://github.com/redis/jedis
[Jedis-package]: https://search.maven.org/artifact/redis.clients/jedis
[Jedis-stars]: https://img.shields.io/github/stars/redis/jedis.svg?style=social&amp;label=Star&amp;maxAge=2592000

[JRedisJSON-author]: https://redislabs.com
[JRedisJSON-url]: https://github.com/RedisJSON/JRedisJSON
[JRedisJSON-package]: https://search.maven.org/artifact/com.redislabs/jrejson/
[JRedisJSON-stars]: https://img.shields.io/github/stars/RedisJSON/JRedisJSON.svg?style=social&amp;label=Star&amp;maxAge=2592000

[Redisson-author]: https://github.com/redisson/
[Redisson-url]: https://github.com/redisson/redisson
[Redisson-package]: https://search.maven.org/artifact/org.redisson/redisson/
[Redisson-stars]: https://img.shields.io/github/stars/redisson/redisson.svg?style=social&amp;label=Star&amp;maxAge=2592000

[redis-modules-java-author]: https://github.com/dengliming/
[redis-modules-java-url]: https://github.com/dengliming/redis-modules-java
[redis-modules-java-package]: https://search.maven.org/artifact/io.github.dengliming.redismodule/redis-modules-java/
[redis-modules-java-stars]: https://img.shields.io/github/stars/dengliming/redis-modules-java.svg?style=social&amp;label=Star&amp;maxAge=2592000

[redis-py-url]: https://github.com/redis/redis-py
[redis-py-package]: https://pypi.python.org/pypi/redis
[redis-py-stars]: https://img.shields.io/github/stars/redis/redis-py.svg?style=social&amp;label=Star&amp;maxAge=2592000

[nredisstack-url]: https://github.com/redis/nredisstack
[nredisstack-stars]: https://img.shields.io/github/stars/redis/nredisstack.svg?style=social&amp;label=Star&amp;maxAge=2592000
[nredisstack-package]: https://www.nuget.org/packages/nredisstack/

[node-redis-author]: https://redis.com
[node-redis-url]: https://github.com/redis/node-redis
[node-redis-package]: https://www.npmjs.com/package/redis
[node-redis-stars]: https://img.shields.io/github/stars/redis/node-redis.svg?style=social&amp;label=Star&amp;maxAge=2592000

[iorejson-author]: https://github.com/evanhuang8
[iorejson-url]: https://github.com/evanhuang8/iorejson
[iorejson-package]: https://www.npmjs.com/package/iorejson
[iorejson-stars]: https://img.shields.io/github/stars/evanhuang8/iorejson.svg?style=social&amp;label=Star&amp;maxAge=2592000

[node-redis-url]: https://github.com/redis/node-redis
[node-redis-stars]: https://img.shields.io/github/stars/redis/node-redis.svg?style=social&amp;label=Star&amp;maxAge=2592000
[node-redis-package]: https://www.npmjs.com/package/redis

[ioredis-rejson-author]: https://github.com/schulzf
[ioredis-rejson-url]: https://github.com/schulzf/ioredis-rejson
[ioredis-rejson-package]: https://www.npmjs.com/package/ioredis-rejson
[ioredis-rejson-stars]: https://img.shields.io/github/stars/schulzf/ioredis-rejson.svg?style=social&amp;label=Star&amp;maxAge=2592000

[go-rejson-author]: https://github.com/nitishm
[go-rejson-url]: https://github.com/nitishm/go-rejson/
[go-rejson-package]: https://www.npmjs.com/package/iorejson
[go-rejson-stars]: https://img.shields.io/github/stars/nitishm/go-rejson.svg?style=social&amp;label=Star&amp;maxAge=2592000

[rueidis-url]: https://github.com/rueian/rueidis
[rueidis-author]: https://github.com/rueian
[rueidis-stars]: https://img.shields.io/github/stars/rueian/rueidis.svg?style=social&amp;label=Star&amp;maxAge=2592000

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

[redis-om-python-url]: https://github.com/redis/redis-om-python
[redis-om-python-author]: https://redis.com
[redis-om-python-package]: https://pypi.org/project/redis-om/
[redis-om-python-stars]: https://img.shields.io/github/stars/redis/redis-om-python.svg?style=social&amp;label=Star&amp;maxAge=2592000

[redis-om-spring-url]: https://github.com/redis/redis-om-spring
[redis-om-spring-author]: https://redis.com
[redis-om-spring-stars]: https://img.shields.io/github/stars/redis/redis-om-spring.svg?style=social&amp;label=Star&amp;maxAge=2592000

[redis-om-node-url]: https://github.com/redis/redis-om-node
[redis-om-node-author]: https://redis.com
[redis-om-node-package]: https://www.npmjs.com/package/redis-om
[redis-om-node-stars]: https://img.shields.io/github/stars/redis/redis-om-node.svg?style=social&amp;label=Star&amp;maxAge=2592000

[redis-om-dotnet-url]: https://github.com/redis/redis-om-dotnet
[redis-om-dotnet-author]: https://redis.com
[redis-om-dotnet-package]: https://www.nuget.org/packages/Redis.OM/
[redis-om-dotnet-stars]: https://img.shields.io/github/stars/redis/redis-om-dotnet.svg?style=social&amp;label=Star&amp;maxAge=2592000

[rustis-url]: https://github.com/dahomey-technologies/rustis
[rustis-author]: https://github.com/dahomey-technologies
[rustis-stars]: https://img.shields.io/github/stars/dahomey-technologies/rustis.svg?style=social&amp;label=Star&amp;maxAge=2592000
[rustis-package]: https://crates.io/crates/rustis

## Acknowledgments

RedisJSON is developed with <3 at [Redis Labs](https://redislabs.com).

RedisJSON is made possible only because of the existance of this amazing open source project:

* [redis](https://github.com/antirez/redis)

## License

RedisJSON is licensed under the [Redis Source Available License 2.0 (RSALv2)](https://redis.com/legal/rsalv2-agreement) or the [Server Side Public License v1 (SSPLv1)](https://www.mongodb.com/licensing/server-side-public-license).
