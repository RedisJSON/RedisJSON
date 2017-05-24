[![Build Status](https://travis-ci.org/RedisLabsModules/rejson.svg?branch=master)](https://travis-ci.org/RedisLabsModules/rejson)

# ReJSON - a JSON data type for Redis

ReJSON is a [Redis](http://redis.io/) module that implements [ECMA-404 The JSON Data Interchange Standard](http://json.org/) as a native data type. It allows storing, updating and fetching JSON values from Redis keys (documents).

Primary features:

* Full support of the JSON standard
* [JSONPath](http://goessner.net/articles/JsonPath/)-like syntax for selecting element inside documents
* Documents are stored as binary data in a tree structure, allowing fast access to sub-elements
* Typed atomic operations for all JSON values types

## Quickstart

1.  [Build the ReJSON module library](https://redislabsmodules.github.io/rejson/#building-the-module)
1.  [Load ReJSON to Redis](https://redislabsmodules.github.io/rejson/#loading-the-module-to-redis)
1.  [Use it from **any** Redis client](https://redislabsmodules.github.io/rejson/#using-rejson), e.g.:

![ReJSON with `redis-cli`](docs/images/demo.gif)

## Documentation

Read the docs at https://redislabsmodules.github.io/rejson

## Limitations and known issues

* Beta quality
* Searching for object keys is O(N)
* Containers are not scaled down after deleting items (i.e. free memory isn't reclaimed)
* Numbers are stored using 64 bits integers or doubles, out of range values are not accepted

## Acknowledgements

ReJSON is developed with <3 at [Redis Labs](https://redislabs.com).

ReJSON is made possible only because of the existance of these amazing open source projects:

* [jsonsl](https://github.com/mnunberg/jsonsl)
* [redis](https://github.com/antirez/redis)

## License

AGPLv3 - see [LICENSE](LICENSE)
