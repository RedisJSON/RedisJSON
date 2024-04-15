[![GitHub issues](https://img.shields.io/github/release/RedisJSON/RedisJSON.svg)](https://github.com/RedisJSON/RedisJSON/releases/latest)
[![CircleCI](https://circleci.com/gh/RedisJSON/RedisJSON/tree/master.svg?style=svg)](https://circleci.com/gh/RedisJSON/RedisJSON/tree/master)
[![macos](https://github.com/RedisJSON/RedisJSON/workflows/macos/badge.svg)](https://github.com/RedisJSON/RedisJSON/actions?query=workflow%3Amacos)
[![Dockerhub](https://img.shields.io/docker/pulls/redis/redis-stack-server?label=redis-stack-server)](https://hub.docker.com/r/redis/redis-stack-server/)
[![Codecov](https://codecov.io/gh/RedisJSON/RedisJSON/branch/master/graph/badge.svg)](https://codecov.io/gh/RedisJSON/RedisJSON)

# RedisJSON

[![Forum](https://img.shields.io/badge/Forum-RedisJSON-blue)](https://forum.redislabs.com/c/modules/redisjson)
[![Discord](https://img.shields.io/discord/697882427875393627?style=flat-square)](https://discord.gg/QUkjSsk)

<img src="docs/docs/images/logo.svg" alt="logo" width="300"/>

## Overview

RedisJSON is a [Redis](https://redis.io/) module that implements [ECMA-404 The JSON Data Interchange Standard](https://json.org/) as a native data type. It allows storing, updating and fetching JSON values from Redis keys (documents).

## Primary features

* Full support of the JSON standard
* [JSONPath](https://goessner.net/articles/JsonPath/) syntax for selecting elements inside documents
* Documents are stored as binary data in a tree structure, allowing fast access to sub-elements
* Typed atomic operations for all JSON values types
* Secondary index support when combined with [RediSearch](https://redis.io/docs/latest/develop/interact/search-and-query/)

## Quick start

```bash
docker run -p 6379:6379 --name redis-stack redis/redis-stack:latest
```

## Documentation

Read the docs at <https://redis.io/docs/latest/develop/data-types/json/>

### How do I Redis?

[Learn for free at Redis University](https://university.redis.com/)

[Build faster with the Redis Launchpad](https://launchpad.redis.com/)

[Try the Redis Cloud](https://redis.com/try-free/)

[Dive in developer tutorials](https://developer.redis.com/)

[Join the Redis community](https://redis.com/community/)

[Work at Redis](https://redis.com/company/careers/jobs/)

## Build

Make sure you have Rust installed:
<https://www.rust-lang.org/tools/install>

Then, build as usual:

```bash
cargo build --release
```

When running the tests, you need to explicitly specify the `test` feature to disable use of the Redis memory allocator when testing:

```bash
cargo test
```

If you forget to do this, you'll see an error mentioning `signal: 4, SIGILL: illegal instruction`.

## Run

### Linux

```bash
redis-server --loadmodule ./target/release/librejson.so
```

### Mac OS

```bash
redis-server --loadmodule ./target/release/librejson.dylib
```

## Client libraries

### Official clients

| [<img width="75" src="https://user-images.githubusercontent.com/1655867/228534778-d0b41ce8-3ce4-4340-bd32-754f01ebed43.svg" />][dotnet-quickstart]  | [<img width="75" src="https://raw.githubusercontent.com/devicons/devicon/master/icons/java/java-plain-wordmark.svg" />][java-quickstart]  | [<img width="75" src="https://raw.githubusercontent.com/devicons/devicon/master/icons/nodejs/nodejs-original-wordmark.svg" />][nodejs-quickstart]   | [<img width="75" src="https://raw.githubusercontent.com/devicons/devicon/master/icons/python/python-original-wordmark.svg" />][python-quickstart]  |
|---|---|---|---|
|  [NRedisStack][dotnet-quickstart] | [Jedis][java-quickstart]  | [node-redis][nodejs-quickstart]  |  [redis-py][python-quickstart] |
|  [Redis.OM][dotnet-om] | [Redis OM Spring][java-om]  | [redis-om-node][nodejs-om]  |  [redis-om][python-om] |

[dotnet-quickstart]: https://redis.io/docs/redis-clients/dotnet/
[dotnet-om]: https://github.com/redis/redis-om-dotnet

[java-quickstart]: https://redis.io/docs/redis-clients/java/
[java-om]: https://github.com/redis/redis-om-spring

[nodejs-quickstart]: https://redis.io/docs/redis-clients/nodejs/
[nodejs-om]: https://github.com/redis/redis-om-node

[python-quickstart]: https://redis.io/docs/redis-clients/python/
[python-om]: https://github.com/redis/redis-om-python

### Community supported clients

| Project | Language | License | Author | Stars | Package | Comment |
| ------- | -------- | ------- | ------ | ----- | ------- | ------- |
| [Redisson][Redisson-url] | Java | Apache-2.0 | [Redisson][Redisson-author] | [![Redisson-stars]][Redisson-url] | [Maven][Redisson-package] |
| [redis-modules-java][redis-modules-java-url] | Java | Apache-2.0 | [Liming Deng @dengliming][redis-modules-java-author] | [![redis-modules-java-stars]][redis-modules-java-url] | [maven][redis-modules-java-package] |
| [ioredis-rejson][ioredis-rejson-url] | Node.js | MIT | [Felipe Schulz @schulzf][ioredis-rejson-author] | [![ioredis-rejson-stars]][ioredis-rejson-url] | [npm][ioredis-rejson-package] |
| [go-rejson][go-rejson-url] | Go | MIT | [Nitish Malhotra @nitishm][go-rejson-author] | [![go-rejson-stars]][go-rejson-url] | |
| [rejonson][rejonson-url] | Go | Apache-2.0 | [Daniel Krom @KromDaniel][rejonson-author] | [![rejonson-stars]][rejonson-url] | |
| [rueidis][rueidis-url] | Go | Apache-2.0 | [Rueian @rueian][rueidis-author] | [![rueidis-stars]][rueidis-url] | |
| [NReJSON][NReJSON-url]  | .NET | MIT/Apache-2.0 | [Tommy Hanks @tombatron][NReJSON-author] | [![NReJSON-stars]][NReJSON-url] | [nuget][NReJSON-package] |
| [phpredis-json][phpredis-json-url]  | PHP | MIT | [Rafa Campoy @averias][phpredis-json-author] | [![phpredis-json-stars]][phpredis-json-url] | [composer][phpredis-json-package] |
| [redislabs-rejson][redislabs-rejson-url]  | PHP | MIT | [Mehmet Korkmaz @mkorkmaz][redislabs-rejson-author] | [![redislabs-rejson-stars]][redislabs-rejson-url] | [composer][redislabs-rejson-package] |
| [rejson-rb][rejson-rb-url]  | Ruby | MIT | [Pavan Vachhani @vachhanihpavan][rejson-rb-author] | [![rejson-rb-stars]][rejson-rb-url] | [rubygems][rejson-rb-package]|
| [rustis][rustis-url] | Rust | MIT | [Dahomey Technologies][rustis-author] | [![rustis-stars]][rustis-url] | [crate][rustis-package]| [Documentation](https://docs.rs/rustis/latest/rustis/commands/trait.JsonCommands.html) |
| [coredis][coredis-url] | Python | MIT | [Ali-Akber Saifee @alisaifee][coredis-author] | [![coredis-stars]][coredis-url] | [pypi][coredis-package]| [Documentation][coredis-documentation] |

[Redisson-author]: https://github.com/redisson/
[Redisson-url]: https://github.com/redisson/redisson
[Redisson-package]: https://search.maven.org/artifact/org.redisson/redisson/
[Redisson-stars]: https://img.shields.io/github/stars/redisson/redisson.svg?style=social&amp;label=Star&amp;maxAge=2592000

[redis-modules-java-author]: https://github.com/dengliming/
[redis-modules-java-url]: https://github.com/dengliming/redis-modules-java
[redis-modules-java-package]: https://search.maven.org/artifact/io.github.dengliming.redismodule/redis-modules-java/
[redis-modules-java-stars]: https://img.shields.io/github/stars/dengliming/redis-modules-java.svg?style=social&amp;label=Star&amp;maxAge=2592000

[ioredis-rejson-author]: https://github.com/schulzf
[ioredis-rejson-url]: https://github.com/schulzf/ioredis-rejson
[ioredis-rejson-package]: https://www.npmjs.com/package/ioredis-rejson
[ioredis-rejson-stars]: https://img.shields.io/github/stars/schulzf/ioredis-rejson.svg?style=social&amp;label=Star&amp;maxAge=2592000

[go-rejson-author]: https://github.com/nitishm
[go-rejson-url]: https://github.com/nitishm/go-rejson/
[go-rejson-stars]: https://img.shields.io/github/stars/nitishm/go-rejson.svg?style=social&amp;label=Star&amp;maxAge=2592000

[rueidis-url]: https://github.com/rueian/rueidis
[rueidis-author]: https://github.com/rueian
[rueidis-stars]: https://img.shields.io/github/stars/rueian/rueidis.svg?style=social&amp;label=Star&amp;maxAge=2592000

[rejonson-author]: https://github.com/KromDaniel
[rejonson-url]: https://github.com/KromDaniel/rejonson
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

[rustis-url]: https://github.com/dahomey-technologies/rustis
[rustis-author]: https://github.com/dahomey-technologies
[rustis-stars]: https://img.shields.io/github/stars/dahomey-technologies/rustis.svg?style=social&amp;label=Star&amp;maxAge=2592000
[rustis-package]: https://crates.io/crates/rustis

[coredis-author]: https://github.com/alisaifee
[coredis-url]: https://github.com/alisaifee/coredis
[coredis-package]: https://pypi.org/project/coredis/
[coredis-stars]: https://img.shields.io/github/stars/alisaifee/coredis.svg?style=social&amp;label=Star&amp;maxAge=2592000
[coredis-documentation]: https://coredis.readthedocs.io/en/stable/handbook/modules.html#redisjson

## Acknowledgments

RedisJSON is developed with <3 at [Redis Labs](https://redislabs.com).

RedisJSON is made possible only because of the existence of this amazing open source project:

* [redis](https://github.com/antirez/redis)

## License

RedisJSON is licensed under the [Redis Source Available License 2.0 (RSALv2)](https://redis.com/legal/rsalv2-agreement) or the [Server Side Public License v1 (SSPLv1)](https://www.mongodb.com/licensing/server-side-public-license).
