[![GitHub issues](https://img.shields.io/github/release/RedisJSON/RedisJSON2.svg)](https://github.com/RedisJSON/RedisJSON2/releases/latest)
[![CircleCI](https://circleci.com/gh/RedisJSON/RedisJSON2/tree/master.svg?style=svg)](https://circleci.com/gh/RedisJSON/RedisJSON2/tree/master)
[![Mailing List](https://img.shields.io/badge/Mailing%20List-RedisJSON-blue)](https://groups.google.com/forum/#!forum/redisjson)
[![Gitter](https://badges.gitter.im/RedisLabs/RedisJSON.svg)](https://gitter.im/RedisLabs/RedisJSON?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge)

# RedisJSON2

RedisJSON2 ([RedisJSON](https://github.com/RedisJSON/RedisJSON) nextgen) is a [Redis](https://redis.io/) module that implements [ECMA-404 The JSON Data Interchange Standard](http://json.org/) as a native data type. It allows storing, updating and fetching JSON values from Redis keys (documents).

## Primary features:

* Full support of the JSON standard
* [JSONPath](http://goessner.net/articles/JsonPath/) syntax for selecting elements inside documents
* Documents are stored as binary data in a tree structure, allowing fast access to sub-elements
* Typed atomic operations for all JSON values types
* Secondery index support based on [RediSeach](http://redisearch.io)


## New Commands in RedisJSON2

    JSON.INDEX ADD <index> <field> <path>
    JSON.QGET <index> <query> <path>

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
```
    


## Build

```bash
cargo build --release
```

## Run

```
redis-server --loadmodule ./target/release/libredisjson.so
```
