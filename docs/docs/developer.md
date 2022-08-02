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
By invoking the following command, RedisJSON module and its submodules are cloned:
```sh
git clone --recursive https://github.com/RedisJSON/RedisJSON.git
```
## Working in an isolated environment
There are several reasons to develop in an isolated environment, like keeping your workstation clean, and developing for a different Linux distribution.
The most general option for an isolated environment is a virtual machine (it's very easy to set one up using [Vagrant](https://www.vagrantup.com)).
Docker is even a more agile, as it offers an almost instant solution:

```
search=$(docker run -d -it -v $PWD:/build debian:bullseye bash)
docker exec -it $search bash
```
Then, from within the container, ```cd /build``` and go on as usual.

In this mode, all installations remain in the scope of the Docker container.
Upon exiting the container, you can either re-invoke it with the above ```docker exec``` or commit the state of the container to an image and re-invoke it on a later stage:

```
docker commit $search redisjson1
docker stop $search
search=$(docker run -d -it -v $PWD:/build redisjson1 bash)
docker exec -it $search bash
```

You can replace `debian:bullseye` with your OS of choice, with the host OS being the best choice (so you can run the RedisJSON binary on your host once it is built).

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
* Use a Python virtual environment, as Python installations are known to be sensitive when not used in isolation: `python2 -m virtualenv venv; . ./venv/bin/activate`

## Installing Redis
As a rule of thumb, you're better off running the latest Redis version.

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
```make build``` will build RedisJSON.

Notes:

* Binary files are placed under `target/release/`, according to platform and build variant.

* RedisJSON uses [Cargo](https://github.com/rust-lang/cargo) as its build system. ```make build``` will invoke both Cargo and the subsequent make command that's required to complete the build.

Use ```make clean``` to remove built artifacts. ```make clean ALL=1``` will remove the entire bin subdirectory.

## Running tests
Python is required for RedisJSON's module test. Install it with `apt-get install python`. You'll also
need to have [redis-py](https://github.com/redis/redis-py) installed. The easiest way to get
it is using pip and running `pip install redis`.

There are several sets of unit tests:
* Rust tests, integrated in the source code, run by ```make cargo_test```.
* Python tests (enabled by RLTest), located in ```tests/pytests```, run by ```make pytest```.

One can run all tests by invoking ```make test```.
A single test can be run using the ```TEST``` parameter, e.g. ```make test TEST=regex```.

The module's test can be run against an "embedded" disposable Redis instance, or against an instance
you provide to it. The "embedded" mode requires having the `redis-server` executable in your `PATH`.

You can override the spawning of the embedded server by specifying a Redis port via the `REDIS_PORT`
environment variable, e.g.:

```bash
$ # use an existing local Redis instance for testing the module
$ REDIS_PORT=6379 make test
```

## Debugging
Compile after settting the environment variable `DEBUG`, e.g. `export DEBUG=1`, to include the
debugging information.

Breakpoints can be added to Python tests in a single-test mode, one can set a breakpoint by using the ```BB()``` function inside a test.

