---
title: "Client Libraries"
linkTitle: "Clients"
weight: 4
description: >
    List of RedisJSON client libraries
---

RedisJSON has several client libraries, written by the module authors and community members - abstracting the API in different programming languages.

While it is possible and simple to use the raw Redis commands API, in most cases it's easier to just use a client library abstracting it.

## Currently available Libraries

| Project | Language | License | Author | Stars | Package |
| ------- | -------- | ------- | ------ | ----- | --- |
| [node-redis][node-redis-url] | Node.js | MIT | [Redis][node-redis-author] | [![node-redis-stars]][node-redis-url] | [npm][node-redis-package] |
| [iorejson][iorejson-url] | Node.js | MIT | [Evan Huang @evanhuang8][iorejson-author] | [![iorejson-stars]][iorejson-url] | [npm][iorejson-package] |
| [redis-om-node][redis-om-node-url]  | Node | BSD-3-Clause | [Redis][redis-om-node-author] | [![redis-om-node-stars]][redis-om-node-url] | [npm][redis-om-node-package] |
| [node_redis-rejson][node_redis-rejson-url] | Node.js | MIT | [Kyle Davis @stockholmux][node_redis-rejson-author] | [![node_redis-rejson-stars]][node_redis-rejson-url] | [npm][node_redis-rejson-package]  |
| [redis-modules-sdk][redis-modules-sdk-url] | Node.js | BSD-3-Clause | [Dani Tseitlin @danitseitlin][redis-modules-sdk-author] | [![redis-modules-sdk-stars]][redis-modules-sdk-url] | [npm][redis-modules-sdk-package] |
| [Jedis][Jedis-url] | Java | MIT | [Redis][JRedisJSON-author] | [![Jedis-stars]][Jedis-url] | [maven][Jedis-package] |
| [redis-modules-java][redis-modules-java-url] | Java | Apache-2.0 | [Liming Deng @dengliming][redis-modules-java-author] | [![redis-modules-java-stars]][redis-modules-java-url] | [maven][redis-modules-java-package] |
| [Redisson][redisson-url]  | Java | Apache-2.0 | [Nikita Koksharov @mrniko][redisson-author] | [![redisson-stars]][redisson-url] | [maven][redisson-package] |
| [redis-py][redis-py-url]  | Python | MIT | [Redis][redis-py-author] | [![redis-py-stars]][redis-py-url] | [pypi][redis-py-package] |
| [redis-om-spring][redis-om-spring-url]  | Java | BSD-3-Clause | [Redis][redis-om-spring-author] | [![redis-om-spring-stars]][redis-om-spring-url] | |
| [redis-om-python][redis-om-python-url]  | Python | BSD-3-Clause | [Redis][redis-om-python-author] | [![redis-om-python-stars]][redis-om-python-url] | [PyPi][redis-om-python-package] |
| [go-rejson][go-rejson-url] | Go | MIT | [Nitish Malhotra @nitishm][go-rejson-author] | [![go-rejson-stars]][go-rejson-url] | |
| [rejonson][rejonson-url] | Go | Apache-2.0 | [Daniel Krom @KromDaniel][rejonson-author] | [![rejonson-stars]][rejonson-url] | |
| [rueidis][rueidis-url] | Go | Apache-2.0 | [Rueian @rueian][rueidis-author] | [![rueidis-stars]][rueidis-url] | |
| [NReJSON][NReJSON-url]  | .NET | MIT/Apache-2.0 | [Tommy Hanks @tombatron][NReJSON-author] | [![NReJSON-stars]][NReJSON-url] | [nuget][NReJSON-package] |
| [redis-om-dotnet][redis-om-dotnet-url]  | .NET | BSD-3-Clause | [Redis][redis-om-dotnet-author] | [![redis-om-dotnet-stars]][redis-om-dotnet-url] | [nuget][redis-om-dotnet-package] |
| [phpredis-json][phpredis-json-url]  | PHP | MIT | [Rafa Campoy @averias][phpredis-json-author] | [![phpredis-json-stars]][phpredis-json-url] | [composer][phpredis-json-package] |
| [redislabs-rejson][redislabs-rejson-url]  | PHP | MIT | [Mehmet Korkmaz @mkorkmaz][redislabs-rejson-author] | [![redislabs-rejson-stars]][redislabs-rejson-url] | [composer][redislabs-rejson-package] |
| [rejson-rb][rejson-rb-url]  | Ruby | MIT | [Pavan Vachhani @vachhanihpavan][rejson-rb-author] | [![rejson-rb-stars]][rejson-rb-url] | [rubygems][rejson-rb-package]|

[node-redis-author]: https://redis.com
[node-redis-url]: https://github.com/redis/node-redis
[node-redis-package]: https://www.npmjs.com/package/redis
[node-redis-stars]: https://img.shields.io/github/stars/redis/node-redis.svg?style=social&amp;label=Star&amp;maxAge=2592000

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

[Jedis-author]: https://redis.com
[Jedis-url]: https://github.com/redis/jedis
[Jedis-package]: https://search.maven.org/artifact/redis.clients/jedis
[Jedis-stars]: https://img.shields.io/github/stars/redis/jedis.svg?style=social&amp;label=Star&amp;maxAge=2592000

[JRedisJSON-author]: https://redis.com
[JRedisJSON-url]: https://github.com/RedisJSON/JRedisJSON
[JRedisJSON-package]: https://search.maven.org/artifact/com.redislabs/jrejson/1.2.0/jar
[JRedisJSON-stars]: https://img.shields.io/github/stars/RedisJSON/JRedisJSON.svg?style=social&amp;label=Star&amp;maxAge=2592000

[redis-modules-java-author]: https://github.com/dengliming/
[redis-modules-java-url]: https://github.com/dengliming/redis-modules-java
[redis-modules-java-package]: https://search.maven.org/artifact/io.github.dengliming.redismodule/redis-modules-java/
[redis-modules-java-stars]: https://img.shields.io/github/stars/dengliming/redis-modules-java.svg?style=social&amp;label=Star&amp;maxAge=2592000

[redisson-author]: https://github.com/redisson/redisson/graphs/contributors
[redisson-url]: https://redisson.org/
[redisson-package]: https://search.maven.org/artifact/org.redisson/redisson
[redisson-stars]: https://img.shields.io/github/stars/redisson/redisson.svg?style=social&amp;label=Star&amp;maxAge=2592000

[redis-py-author]: https://redis.com
[redis-py-url]: https://github.com/redis/redis-py
[redis-py-package]: https://pypi.python.org/pypi/redis
[redis-py-stars]: https://img.shields.io/github/stars/redis/redis-py.svg?style=social&amp;label=Star&amp;maxAge=2592000

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
[redis-om-dotnet-author]: htts://redis.com
[redis-om-dotnet-package]: https://www.nuget.org/packages/Redis.OM/
[redis-om-dotnet-stars]: https://img.shields.io/github/stars/redis/redis-om-dotnet.svg?style=social&amp;label=Star&amp;maxAge=2592000