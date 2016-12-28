# Developer notes

## Design

You can find some information about ReJSON's design in [design.md](design.md).

## Testing

Python is required for ReJSON's module test. Install it with `apt-get install python`.

Also, the module's test requires a path to the `redis-server` executable. The path is stored in the
`REDIS_SERVER_PATH` variable and can be set using CMake's `-D` switch as follows:

```bash
~/rejson$ cmake -D REDIS_SERVER_PATH=/path/to/redis-server --build build
```

And then run the tests:

```bash
~/rejson$ cmake --build build --target test
...
```
