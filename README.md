# ReJSON - a JSON data type for Redis

ReJSON is a Redis module that implements
[ECMA-404 The JSON Data Interchange Standard](http://json.org/) as a native data type. It allows
storing, updating and fetching JSON values from Redis keys (documents). The JSON values are managed
as binary objects, thus allowing Redis-blazing performance. 

## Quickstart

1.  [Build the ReJSON module library](#building-the-module-library)
1.  [Load ReJSON to Redis](#loading-the-module-to-redis)
1.  [Use it from **any** Redis client](#using-rejson), e.g.:

````
~$/ redis-cli
127.0.0.1:6379> JSON.SET doc . '{ "foo": "bar", "baz": [42, true] }'
OK
127.0.0.1:6379> JSON.GET doc .baz[0]
"42"
127.0.0.1:6379> JSON.DEL doc .foo
1
127.0.0.1:6379> JSON.OBJKEYS doc .
1) "baz"
````

## What is ReJSON

## Limitations and known issues

* Alpha quality
* AOF rewrite will fail for documents with serialization over 0.5GB?
* Searching for object keys is O(N)

## Building the module library

Prerequirements:

* devtools
* cmake
* rejson repository (e.g. `git clone https://github.com/RedisLabsModules/rejson.git`)

Assuming that the repository's directory is at `~/rejson`, navigate to it and run the script
`bootstrap.sh` followed by `cmake`. This should look something like:

```
~/rejson$ ./bootstrap.sh
-- The C compiler identification is GNU 5.4.0
...
-- Configuring done
-- Generating done
-- Build files have been written to: rejson/build
rejson$ cmake --build build --target rejson
Scanning dependencies of target rmobject
...
[100%] Linking C shared library rejson/lib/rejson.so
[100%] Built target rejson
rejson$ 
```

Congratulations! You can find the compiled module library at `lib/rejson.so`.

#### MacOSX

TBD

#### Windows

Yeah, right :)

## Loading the module to Redis

Prerequirements:

* Redis v4.0 or above (see ...)

The recommended way have Redis load the module is during startup by by adding the following to the
`redis.conf` file:

```
loadmodule /path/to/module/rejson.so
```

In the line above replace `/path/to/module/rejson.so` with the actual path to the module's library.
Alternatively you, you can have Redis load the module using the following command line argument
syntax:

```
~/$ redis-server --loadmodule /path/to/module/rejson.so
```

Lastly, you can also use the [`MODULE LOAD`](http://redis.io/commands/module-load) command. Note,
however, that `MODULE LOAD` is a dangerous command and may be blocked/deprecated in the future due
to security considerations.

## Using ReJSON

Link to docs/commands
Basic Python and Node examples to illustrate the use of a raw command.

## Testing and development

Link to docs/design.md

Setting the path to the Redis server executable for unit testing: `REDIS_SERVER_PATH` CMake variable

`valgrind --tool=memcheck --suppressions=../redis/src/valgrind.sup ../redis/src/redis-server --loadmodule ./lib/rejson.so`

## Contributing

## License
AGPLv3 - see [LICENSE](LICENSE)
