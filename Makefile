
ifneq ($(SAN),)
ifeq ($(SAN),mem)
override SAN=memory
else ifeq ($(SAN),addr)
override SAN=address
endif

override DEBUG:=1
ifeq ($(SAN),memory)
else ifeq ($(SAN),address)
else ifeq ($(SAN),leak)
else ifeq ($(SAN),thread)
else
$(error SAN=mem|addr|leak|thread)
endif

export SAN
endif # SAN

ROOT=.
ifeq ($(wildcard $(ROOT)/deps/readies/mk),)
$(error Submodules not present. Please run 'git submodule update --init --recursive')
endif
MK.pyver:=3
include $(ROOT)/deps/readies/mk/main

#----------------------------------------------------------------------------------------------

define HELPTEXT
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

make sanbox        # create container with CLang Sanitizer

make builddocs
make localdocs
make deploydocs

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
RUST_FLAGS=
RUST_DOCFLAGS=

ifeq ($(DEBUG),1)
ifeq ($(SAN),)
TARGET_DIR=$(BINDIR)/target/debug
else
TARGET_DIR=$(BINDIR)/target/$(RUST_TARGET)/debug
CARGO_TOOLCHAIN = +nightly
CARGO_FLAGS += -Zbuild-std
RUST_FLAGS += -Zsanitizer=$(SAN)
ifeq ($(SAN),memory)
RUST_FLAGS += -Zsanitizer-memory-track-origins
endif
endif
else
CARGO_FLAGS += --release
TARGET_DIR=$(BINDIR)/target/release
endif

ifeq ($(PROFILE),1)
RUST_FLAGS += " -g -C force-frame-pointers=yes"
endif

export CARGO_TARGET_DIR=$(BINDIR)/target
TARGET=$(BINDIR)/$(MODULE_NAME)

#----------------------------------------------------------------------------------------------

setup:
	$(SHOW)./deps/readies/bin/getpy3
	$(SHOW)./sbin/system-setup.py

.PHONY: setup

#----------------------------------------------------------------------------------------------

lint:
	$(SHOW)cargo fmt -- --check

.PHONY: lint

#----------------------------------------------------------------------------------------------

define extract_symbols
$(SHOW)objcopy --only-keep-debug $1 $1.debug
$(SHOW)objcopy --strip-debug $1
$(SHOW)objcopy --add-gnu-debuglink $1.debug $1
endef

RUST_SOEXT.linux=so
RUST_SOEXT.freebsd=so
RUST_SOEXT.macos=dylib

build:
ifeq ($(SAN),)
	$(SHOW)set -e ;\
	export RUSTFLAGS="$(RUST_FLAGS)" ;\
	cargo build --all --all-targets $(CARGO_FLAGS)
else
	$(SHOW)set -e ;\
	export RUSTFLAGS="$(RUST_FLAGS)" ;\
	export RUSTDOCFLAGS="$(RUST_DOCFLAGS)" ;\
	cargo $(CARGO_TOOLCHAIN) build --target $(RUST_TARGET) $(CARGO_FLAGS)
endif
	$(SHOW)cp $(TARGET_DIR)/librejson.$(RUST_SOEXT.$(OS)) $(TARGET)
ifneq ($(DEBUG),1)
ifneq ($(OS),macos)
	$(SHOW)$(call extract_symbols,$(TARGET))
endif
endif

clean:
ifneq ($(ALL),1)
	$(SHOW)cargo clean
else
	$(SHOW)rm -rf $(BINDIR)
endif

.PHONY: build clean

#----------------------------------------------------------------------------------------------

test: pytest

pytest:
	$(SHOW)MODULE=$(abspath $(TARGET)) $(realpath ./tests/pytest/tests.sh)

cargo_test:
	$(SHOW)cargo $(CARGO_TOOLCHAIN) test --features test --all

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
	$(SHOW)set -e ;\
	cd tests/benchmarks ;\
	redisbench-admin $(BENCHMARK_ARGS)

.PHONY: bench benchmark

#----------------------------------------------------------------------------------------------

pack:
	$(SHOW)MODULE=$(abspath $(TARGET)) ./sbin/pack.sh

.PHONY: pack

#----------------------------------------------------------------------------------------------

docker:
	$(SHOW)make -C build/platforms build

docker_push:
	$(SHOW)make -C build/platforms publish

.PHONY: docker docker_push

#----------------------------------------------------------------------------------------------

platform:
	$(SHOW)make -C build/platforms build
ifeq ($(PUBLISH),1)
	$(SHOW)make -C build/platforms publish
endif

ifneq ($(wildcard /w/*),)
SANBOX_ARGS += -v /w:/w
endif

sanbox:
	@docker run -it -v $(PWD):/search -w /search --cap-add=SYS_PTRACE --security-opt seccomp=unconfined $(SANBOX_ARGS) redisfab/clang:13-x64-bullseye bash

.PHONY: sanbox

#----------------------------------------------------------------------------------------------

builddocs:
	$(SHOW)mkdocs build

localdocs: builddocs
	$(SHOW)mkdocs serve

deploydocs: builddocs
	$(SHOW)mkdocs gh-deploy

.PHONY: builddocs localdocs deploydocs
