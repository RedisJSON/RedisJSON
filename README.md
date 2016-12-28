# ReJSON - a JSON data type for Redis

ReJSON is a [Redis](1) module that implements
[ECMA-404 The JSON Data Interchange Standard](2) as a native data type. It allows storing, updating
and fetching JSON values from Redis keys (documents). The JSON values are managed as binary objects,
thus allowing Redis-blazing performance. 

## Quickstart

1.  [Build the ReJSON module library](3)
1.  [Load ReJSON to Redis](4)
1.  [Use it from **any** Redis client](5), e.g.:

![ReJSON with `redis-cli`](docs/images/demo.gif)

## Documentation

Read the docs at https://redislabsmodules.github.io/ReJSON

## Limitations and known issues

* Alpha stage
* AOF rewrite will probably fail for documents with serialization over 0.5GB
* Searching for object keys is O(N)
* Containers are not scaled down after deleting items
* Numbers are stored using 64 bits integers or doubles, out of range values are not accepted

## Acknowledgements

ReJSON is developed with <3 at [Redis Labs](6).

ReJSON is made possible only because of the existance of these amazing open source projects:

* [jsonsl](https://github.com/mnunberg/jsonsl)
* [redis](https://github.com/antirez/redis)

## License
AGPLv3 - see [LICENSE](LICENSE)

  [1]:  http://redis.io/
  [2]:  http://json.org/
  [3]:  https://redislabsmodules.github.io/ReJSON/#building-the-module-library
  [4]:  https://redislabsmodules.github.io/ReJSON/#loading-the-module-to-redis
  [5]:  https://redislabsmodules.github.io/ReJSON/##using-rejson
  [6]:  https://redislabs.com