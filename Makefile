
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

ifneq ($(filter coverage show-cov upload-cov,$(MAKECMDGOALS)),)
COV=1
endif

ROOT=.
ifeq ($(wildcard $(ROOT)/deps/readies),)
$(shell git submodule update --init --recursive)
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

make docker
make docker_push

make platform      # build for specific Linux distribution
  OSNICK=nick        # Linux distribution to build for
  REDIS_VER=ver      # use Redis version `ver`
  TEST=1             # test aftar build
  PACK=1             # create packages
  ARTIFACTS=1        # copy artifacts from docker image
  PUBLISH=1          # publish (i.e. docker push) after build

make sanbox        # create container for CLang Sanitizer tests

make builddocs
make localdocs
make deploydocs

endef

#----------------------------------------------------------------------------------------------

SRCDIR=src

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
NIGHTLY=1
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

ifeq ($(COV),1)
NIGHTLY=1
RUST_FLAGS += -Zinstrument-coverage
export LLVM_PROFILE_FILE=$(BINROOT)/cov/cov-%p-%m.profraw

COV_DIR=$(BINROOT)/cov
COV_PROFDATA=$(COV_DIR)/cov.profdata
COV_INFO=$(COV_DIR)/cov.info

COV_EXCLUDE.llvm += \
    "$$HOME/.cargo/*"

define COVERAGE_RESET.llvm
$(SHOW)set -e ;\
echo "Starting coverage analysys." ;\
rm -rf $(COV_DIR) ;\
mkdir -p $(COV_DIR)
endef

define COVERAGE_COLLECT.llvm
$(SHOW)set -e ;\
echo "Collecting coverage data ..." ;\
llvm-profdata merge --sparse `ls $(COV_DIR)/*.profraw` -o $(COV_PROFDATA) &> /dev/null ;\
llvm-cov export --format=lcov --instr-profile $(COV_PROFDATA) $(TARGET) > $(COV_INFO).all ;\
lcov -o $(COV_INFO) -r $(COV_INFO).all $(COV_EXCLUDE.llvm) &> /dev/null
endef

define COVERAGE_REPORT.llvm
$(SHOW)set -e ;\
lcov -l $(COV_INFO) ;\
genhtml --legend --ignore-errors source -o $(COV_DIR) $(COV_INFO) > /dev/null 2>&1 ;\
echo "Coverage info at $$(realpath $(COV_DIR))/index.html"
endef

define COVERAGE_COLLECT_REPORT.llvm
$(COVERAGE_COLLECT.llvm)
$(COVERAGE_REPORT.llvm)
endef

endif # COV

ifeq ($(PROFILE),1)
RUST_FLAGS += -g -C force-frame-pointers=yes
endif

ifeq ($(NIGHTLY),1)
TARGET_DIR=$(BINDIR)/target/$(RUST_TARGET)/debug
CARGO_TOOLCHAIN = +nightly
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
ifneq ($(NIGHTLY),1)
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

test: cargo_test pytest

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

COV_EXCLUDE_DIRS += deps tests
COV_EXCLUDE+=$(foreach D,$(COV_EXCLUDE_DIRS),'$(realpath $(ROOT))/$(D)/*')

coverage:
	$(SHOW)$(MAKE) build COV=1
	$(SHOW)$(COVERAGE_RESET.llvm)
	-$(SHOW)$(MAKE) test COV=1
	$(SHOW)$(COVERAGE_COLLECT_REPORT.llvm)

show-cov:
	$(SHOW)lcov -l $(COV_INFO)

upload-cov:
	$(SHOW)bash <(curl -s https://raw.githubusercontent.com/codecov/codecov-bash/master/codecov) -f $(COV_INFO)

.PHONY: coverage show-cov upload-cov

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
	@docker run -it -v $(PWD):/rejson -w /rejson --cap-add=SYS_PTRACE --security-opt seccomp=unconfined $(SANBOX_ARGS) redisfab/clang:13-x64-bullseye bash

.PHONY: sanbox

#----------------------------------------------------------------------------------------------

builddocs:
	$(SHOW)mkdocs build

localdocs: builddocs
	$(SHOW)mkdocs serve

deploydocs: builddocs
	$(SHOW)mkdocs gh-deploy

.PHONY: builddocs localdocs deploydocs
