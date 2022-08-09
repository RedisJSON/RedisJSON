---
title: "Developer notes"
linkTitle: "Developer notes"
weight: 7
description: >
    Notes on debugging, testing and documentation
---

# Developing RedisJSON

Developing RedisJSON involves setting up the development environment (which can be either Linux-based or macOS-based), building RedisJSON, running tests and benchmarks, and debugging both the RedisJSON module and its tests.

## Cloning the git repository
To clone the RedisJSON module and its submodules, run:
```sh
git clone --recursive https://github.com/RedisJSON/RedisJSON.git
```
## Working in an isolated environment
There are several reasons to use an isolated environment for development, like keeping your workstation clean and developing for a different Linux distribution.

You can use a virtual machine as an isolated development environment. To set one up, you can use [Vagrant](https://www.vagrantup.com) or Docker.

To set up a virtual machine with Docker:

```
search=$(docker run -d -it -v $PWD:/build debian:bullseye bash)
docker exec -it $search bash
```
Then run ```cd /build``` from within the container.

In this mode, all installations remain in the scope of the Docker container.
After you exit the container, you can either restart it with the previous ```docker exec``` command or save the state of the container to an image and resume it at a later time:

```
docker commit $search redisjson1
docker stop $search
search=$(docker run -d -it -v $PWD:/build redisjson1 bash)
docker exec -it $search bash
```

You can replace `debian:bullseye` with your OS of choice. If you use the same OS as your host machine, you can run the RedisJSON binary on your host after it is built.

## Installing prerequisites

To build and test RedisJSON one needs to install several packages, depending on the underlying OS. Currently, we support the Ubuntu/Debian, CentOS, Fedora, and macOS.

First, enter `RedisJSON` directory.

If you have ```gnu make``` installed, you can execute,

```
make setup
```
Alternatively, invoke the following:

```
./deps/readies/bin/getpy3
./sbin/setup.py
```
Note that ```system-setup.py``` **will install various packages on your system** using the native package manager and pip. It will invoke `sudo` on its own, prompting for permission.

If you prefer to avoid that, you can:

* Review `system-setup.py` and install packages manually,
* Use `system-setup.py --nop` to display installation commands without executing them,
* Use an isolated environment like explained above,
* Use a Python virtual environment, as Python installations are known to be sensitive when not used in isolation: `python -m virtualenv venv; . ./venv/bin/activate`

## Installing Redis
Generally, it is best to run the latest Redis version.

If your OS has a Redis 6.x package, you can install it using the OS package manager.

Otherwise, you can invoke ```./deps/readies/bin/getredis```.

## Getting help
```make help``` provides a quick summary of the development features:

```
make setup         # install prerequisites

make build
  DEBUG=1          # build debug variant
  SAN=type         # build with LLVM sanitizer (type=address|memory|leak|thread)
  VALGRIND|VG=1    # build for testing with Valgrind
make clean         # remove binary files
  ALL=1            # remove binary directories

make all           # build all libraries and packages

make test          # run both cargo and python tests
make cargo_test    # run inbuilt rust unit tests
make pytest        # run flow tests using RLTest
  TEST=file:name     # run test matching `name` from `file`
  TEST_ARGS="..."    # RLTest arguments
  QUICK=1            # run only general tests
  GEN=1              # run general tests on a standalone Redis topology
  AOF=1              # run AOF persistency tests on a standalone Redis topology
  SLAVES=1           # run replication tests on standalone Redis topology
  CLUSTER=1          # run general tests on a OSS Redis Cluster topology
  VALGRIND|VG=1      # run specified tests with Valgrind
  VERBOSE=1          # display more RLTest-related information

make pack          # build package (RAMP file)
make upload-artifacts   # copy snapshot packages to S3
  OSNICK=nick             # copy snapshots for specific OSNICK
make upload-release     # copy release packages to S3

common options for upload operations:
  STAGING=1             # copy to staging lab area (for validation)
  FORCE=1               # allow operation outside CI environment
  VERBOSE=1             # show more details
  NOP=1                 # do not copy, just print commands

make coverage      # perform coverage analysis
make show-cov      # show coverage analysis results (implies COV=1)
make upload-cov    # upload coverage analysis results to codecov.io (implies COV=1)

make docker        # build for specific Linux distribution
  OSNICK=nick        # Linux distribution to build for
  REDIS_VER=ver      # use Redis version `ver`
  TEST=1             # test after build
  PACK=1             # create packages
  ARTIFACTS=1        # copy artifacts from docker image
  PUBLISH=1          # publish (i.e. docker push) after build

make sanbox        # create container for CLang Sanitizer tests
```

## Building from source
Run ```make build``` to build RedisJSON.

Notes:

* Binary files are placed under `target/release/`, according to platform and build variant.

* RedisJSON uses [Cargo](https://github.com/rust-lang/cargo) as its build system. ```make build``` will invoke both Cargo and the subsequent `make` command that's required to complete the build.

Use ```make clean``` to remove built artifacts. ```make clean ALL=1``` will remove the entire bin subdirectory.

## Running tests
There are several sets of unit tests:
* Rust tests, integrated in the source code, run by ```make cargo_test```.
* Python tests (enabled by RLTest), located in ```tests/pytests```, run by ```make pytest```.

You can run all tests with ```make test```.
To run only specific tests, use the ```TEST``` parameter. For example, run ```make test TEST=regex```.

You can run the module's tests against an "embedded" disposable Redis instance or against an instance
you provide. To use the "embedded" mode, you must include the `redis-server` executable in your `PATH`.

You can override the spawning of the embedded server by specifying a Redis port via the `REDIS_PORT`
environment variable, e.g.:

```bash
$ # use an existing local Redis instance for testing the module
$ REDIS_PORT=6379 make test
```

## Debugging
To include debugging information, you need to set the `DEBUG` environment variable before you compile RedisJSON. For example, run `export DEBUG=1`.

You can add breakpoints to Python tests in single-test mode. To set a breakpoint, call the ```BB()``` function inside a test.

