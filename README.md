[![GitHub issues](https://img.shields.io/github/release/RedisLabsModules/redisjson.svg)](https://github.com/RedisLabsModules/redisjson/releases/latest)
[![CircleCI](https://circleci.com/gh/RedisLabsModules/redisjson/tree/master.svg?style=svg)](https://circleci.com/gh/RedisLabsModules/redisjson/tree/master)
<!--[![DockerHub](https://dockerbuildbadges.quelltext.eu/status.svg?organization=redislabs&repository=rejson)](https://hub.docker.com/r/redislabs/rejson/builds/) -->

# RedisJSON - a JSON data type for Redis

RedisJSON is a [Redis](http://redis.io/) module that implements [ECMA-404 The JSON Data Interchange Standard](http://json.org/) as a native data type. It allows storing, updating and fetching JSON values from Redis keys (documents).

Primary features:

* Full support of the JSON standard
* [JSONPath](http://goessner.net/articles/JsonPath/)-like syntax for selecting element inside documents
* Documents are stored as binary data in a tree structure, allowing fast access to sub-elements
* Typed atomic operations for all JSON values types

## Quickstart

1.  [Launch RedisJSON with Docker](https://redislabsmodules.github.io/redisjson/#launch-redisjson-with-docker)
1.  [Use RedisJSON from **any** Redis client](https://redislabsmodules.github.io/redisjson/#using-redisjson), e.g.:

![RedisJSON with `redis-cli`](docs/images/demo.gif)

## Documentation

Read the docs at https://redislabsmodules.github.io/redisjson

## Current limitations and known issues

* Searching for object keys is O(N)
* Containers are not scaled down after deleting items (i.e. free memory isn't reclaimed)
* Numbers are stored using 64-bit integers or doubles, and out of range values are not accepted

## Acknowledgements

RedisJSON is developed with <3 at [Redis Labs](https://redislabs.com).

RedisJSON is made possible only because of the existance of these amazing open source projects:

* [jsonsl](https://github.com/mnunberg/jsonsl)
* [redis](https://github.com/antirez/redis)

## License

Redis Source Available License Agreement - see [LICENSE](LICENSE)

