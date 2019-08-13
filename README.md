[![GitHub issues](https://img.shields.io/github/release/RedisJSON/RedisDoc.svg)](https://github.com/RedisJSON/RedisDoc/releases/latest)
[![CircleCI](https://circleci.com/gh/RedisJSON/RedisDoc/tree/master.svg?style=svg)](https://circleci.com/gh/RedisJSON/RedisDoc/tree/master)

# RedisJSON


## Build
```bash
cargo build --release
```

## Run
```
 redis-server --loadmodule ./target/release/libredisjson.so
```
