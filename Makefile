ROOT=.
MK.pyver:=3

ifeq ($(wildcard $(ROOT)/deps/readies/mk),)
$(error Submodules not present. Please run 'git submodule update --init --recursive')
endif
include $(ROOT)/deps/readies/mk/main

ifneq ($(SAN),)
override DEBUG:=1
ifeq ($(SAN),mem)
else ifeq ($(SAN),memory)
else ifeq ($(SAN),addr)
else ifeq ($(SAN),address)
else ifeq ($(SAN),leak)
else ifeq ($(SAN),thread)
else
$(error SAN=mem|addr|leak|thread)
endif
endif

#----------------------------------------------------------------------------------------------

define HELP
make setup         # install prerequisites

make build
  DEBUG=1          # build debug variant
  SAN=type         # build with LLVM sanitizer (type=address|memory|leak|thread)
  VALGRIND|VG=1    # build for testing with Valgrind
make clean         # remove binary files
  ALL=1            # remove binary directories

make all           # build all libraries and packages

make pytest        # run tests
  TEST=name        # run test matching 'name'
  TEST_ARGS="..."  # RLTest arguments
  GEN=0|1          # run general tests on a standalone Redis topology
  AOF=0|1          # run AOF persistency tests on a standalone Redis topology
  SLAVES=0|1       # run replication tests on standalone Redis topology
  CLUSTER=0|1      # run general tests on a OSS Redis Cluster topology
  VALGRIND|VG=1    # run specified tests with Valgrind

make pack          # build package (RAMP file)

make docker
make docker_push

make platform      # build for specific Linux distribution
  OSNICK=nick        # Linux distribution to build for
  REDIS_VER=ver      # use Redis version `ver`
  TEST=1             # test aftar build
  PACK=1             # create packages
  ARTIFACTS=1        # copy artifacts from docker image
  PUBLISH=1          # publish (i.e. docker push) after build

make builddocs
make localdocs
make deploydocs

make nightly       # set rust default to nightly
make stable        # set rust default to stable


endef

#----------------------------------------------------------------------------------------------

MK_CUSTOM_CLEAN=1
BINDIR=$(BINROOT)

include $(MK)/defs
include $(MK)/rules

#----------------------------------------------------------------------------------------------

MODULE_NAME=rejson.so

RUST_TARGET:=$(shell eval $$(rustc --print cfg | grep =); echo $$target_arch-$$target_vendor-$$target_os-$$target_env)
CARGO_TOOLCHAIN=
CARGO_FLAGS=

ifeq ($(DEBUG),1)
ifeq ($(SAN),)
TARGET_DIR=target/debug
else
TARGET_DIR=target/$(RUST_TARGET)/debug
CARGO_TOOLCHAIN = +nightly
CARGO_FLAGS += -Zbuild-std
endif
else
CARGO_FLAGS += --release
TARGET_DIR=target/release
endif

TARGET=$(TARGET_DIR)/$(MODULE_NAME)

#----------------------------------------------------------------------------------------------

all: build

.PHONY: all

#----------------------------------------------------------------------------------------------

setup:
	./deps/readies/bin/getpy3
	./system-setup.py

.PHONY: setup

#----------------------------------------------------------------------------------------------

lint:
	cargo fmt -- --check

.PHONY: lint

#----------------------------------------------------------------------------------------------

RUST_SOEXT.linux=so
RUST_SOEXT.freebsd=so
RUST_SOEXT.macos=dylib

build:
ifeq ($(SAN),)
	cargo build --all --all-targets $(CARGO_FLAGS)
else
	export RUSTFLAGS=-Zsanitizer=$(SAN) ;\
	export RUSTDOCFLAGS=-Zsanitizer=$(SAN) ;\
	cargo $(CARGO_TOOLCHAIN) build --target $(RUST_TARGET) $(CARGO_FLAGS)
endif
	cp $(TARGET_DIR)/librejson.$(RUST_SOEXT.$(OS)) $(TARGET)

clean:
ifneq ($(ALL),1)
	cargo clean
else
	rm -rf target
endif

.PHONY: build clean

#----------------------------------------------------------------------------------------------

test: pytest

pytest:
	MODULE=$(abspath $(TARGET)) ./tests/pytest/tests.sh

cargo_test:
	cargo $(CARGO_TOOLCHAIN) test --features test --all

.PHONY: pytest cargo_test

#----------------------------------------------------------------------------------------------

ifneq ($(REMOTE),)
BENCHMARK_ARGS = run-remote
else
BENCHMARK_ARGS = run-local
endif

BENCHMARK_ARGS += --module_path $(realpath $(TARGET)) --required-module ReJSON

ifneq ($(BENCHMARK),)
BENCHMARK_ARGS += --test $(BENCHMARK)
endif

bench benchmark: $(TARGET)
	cd ./tests/benchmarks ;\
	redisbench-admin $(BENCHMARK_ARGS)

.PHONY: bench benchmark

#----------------------------------------------------------------------------------------------

pack:
	./sbin/pack.sh

.PHONY: pack

#----------------------------------------------------------------------------------------------

docker:
	docker build --pull -t rejson:latest .

docker_push:
	docker push redislabs/rejson:latest

.PHONY: docker docker_push

#----------------------------------------------------------------------------------------------

platform:
	@make -C build/platforms build
ifeq ($(PUBLISH),1)
	@make -C build/platforms publish
endif

#----------------------------------------------------------------------------------------------

builddocs:
	mkdocs build

localdocs: builddocs
	mkdocs serve

deploydocs: builddocs
	mkdocs gh-deploy

.PHONY: builddocs localdocs deploydocs

#----------------------------------------------------------------------------------------------

nightly:
	rustup default nightly
	rustup component add rust-src

stable:
	rustup default stable

.PHONY: nightly stable
