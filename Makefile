
ROOT=.

include $(ROOT)/deps/readies/mk/main

#----------------------------------------------------------------------------------------------

define HELPTEXT
make setup         # install prerequisites

make build
  NIGHTLY=1        # use nightly toolchain
  DEBUG=1          # build debug variant
  SAN=type         # build with LLVM sanitizer (type=address|memory|leak|thread)
  VALGRIND|VG=1    # build for testing with Valgrind
make clean         # remove binary files
  ALL=1            # remove binary directories

make all           # build all libraries and packages

make run           # run redis-server with RedisJSON module

make pytest        # run flow tests using RLTest
  TEST=file:name     # run test matching `name` from `file`
  TEST_ARGS="..."    # RLTest arguments
  QUICK=1            # run only general tests
  GEN=1              # run general tests on a standalone Redis topology
  AOF=1              # run AOF persistency tests on a standalone Redis topology
  SLAVES=1           # run replication tests on standalone Redis topology
  CLUSTER=1          # run general tests on a OSS Redis Cluster topology
  VERBOSE=1          # display more RLTest-related information
  SAN=type           # use LLVM sanitizer (type=address|memory|leak|thread) 
  VG=1               # run specified tests with Valgrind

make bench   # run benchmarks

make pack               # build package (RAMP file)
make upload-artifacts   # copy snapshot packages to S3
  OSNICK=nick             # copy snapshots for specific OSNICK
make upload-release     # copy release packages to S3

common options for upload operations:
  STAGING=1             # copy to staging lab area (for validation)
  FORCE=1               # allow operation outside CI environment
  VERBOSE=1             # show more details
  NOP=1                 # do not copy, just print commands

make coverage     # perform coverage analysis
make show-cov     # show coverage analysis results (implies COV=1)
make upload-cov   # upload coverage analysis results to codecov.io (implies COV=1)

make docker       # build for specific Linux distribution
  OSNICK=nick       # Linux distribution to build for
  REDIS_VER=ver     # use Redis version `ver`
  TEST=1            # test aftar build
  PACK=1            # create packages
  ARTIFACTS=1       # copy artifacts from docker image
  PUBLISH=1         # publish (i.e. docker push) after build

make sanbox   # create container for CLang Sanitizer tests

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
		RUST_FLAGS += -Zsanitizer=$(SAN) -C link-args=-znostart-stop-gc
		ifeq ($(SAN),memory)
			RUST_FLAGS += -Zsanitizer-memory-track-origins
		endif
	endif
else
	CARGO_FLAGS += --release
	TARGET_DIR=$(BINDIR)/target/release
endif

ifeq ($(COV),1)
RUST_FLAGS += -C instrument-coverage
endif

ifeq ($(PROFILE),1)
RUST_FLAGS += -g -C force-frame-pointers=yes
endif

ifeq ($(NIGHTLY),1)
	TARGET_DIR=$(BINDIR)/target/$(RUST_TARGET)/debug

	ifeq ($(RUST_GOOD_NIGHTLY),)
		CARGO_TOOLCHAIN = +nightly
	else
		CARGO_TOOLCHAIN = +$(RUST_GOOD_NIGHTLY)
	endif
endif

export CARGO_TARGET_DIR=$(BINDIR)/target
TARGET=$(BINDIR)/$(MODULE_NAME)

#----------------------------------------------------------------------------------------------

#----------------------------------------------------------------------------------------------
# `setup` mirrors the .github/workflows/flow-macos.yml flow (CI is the ground
# truth). Three phases:
#   1. `.install/install_script.sh` -> abstract deps from dependencies.yaml (+ quirks)
#      or legacy `.install/<os>.sh`
#   2. ensure Rust/cargo on PATH — same as Docker/CI: `.install/getrust.sh` when missing,
#      then `source ~/.cargo/env` (that script pins toolchain via rust-toolchain.toml)
#   3. local `venv/` + `common_installations.sh` -> pip deps inside the venv
# After setup, activate the venv before running tests:
#     . src/venv/bin/activate && make test
# This avoids `sbin/setup` -> readies/getpy3 which is broken on macOS+Python>=3.13
# (PEP 668 + missing `python3-pip`/`python3-virtualenv` brew formulas).
#----------------------------------------------------------------------------------------------

setup:
	$(SHOW)cd .install && ./install_script.sh
	$(SHOW)set -e; \
		cd $(ROOT); \
		if [ -f "$$HOME/.cargo/env" ]; then . "$$HOME/.cargo/env"; fi; \
		if command -v cargo >/dev/null 2>&1; then \
			echo "==> Rust/cargo on PATH: $$(command -v cargo) ($$(cargo --version))"; \
		elif [ -x "$$HOME/.cargo/bin/cargo" ]; then \
			export PATH="$$HOME/.cargo/bin:$$PATH"; \
			echo "==> Using $$(command -v cargo) ($$(cargo --version))"; \
		else \
			echo "==> Rust/cargo not found; running .install/getrust.sh (same entrypoint as Docker)"; \
			chmod +x .install/getrust.sh && .install/getrust.sh; \
			. "$$HOME/.cargo/env"; \
			echo "==> cargo: $$(command -v cargo) ($$(cargo --version))"; \
		fi
	$(SHOW)test -d venv || python3 -m venv venv
	$(SHOW). ./venv/bin/activate && ./.install/common_installations.sh

update:
	$(SHOW)cargo update

.PHONY: setup update

#----------------------------------------------------------------------------------------------

lint:
	$(SHOW)cargo fmt -- --check

format:
	$(SHOW)cargo fmt

.PHONY: lint format

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
	$(SHOW)./sbin/extract_symbols_safe.sh $(TARGET)
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

run:
	$(SHOW)if ! command -v redis-server &> /dev/null; then \
		>&2 echo "redis-server not found." ;\
		>&2 echo "Install it with ./deps/readies/bin/getredis" ;\
	else \
		redis-server --loadmodule $(TARGET) ;\
	fi

.PHONY: run

#----------------------------------------------------------------------------------------------

test: cargo_test pytest

pytest:
	$(SHOW)MODULE=$(abspath $(TARGET)) RLTEST_ARGS='--no-progress' $(realpath ./tests/pytest/tests.sh)

cargo_test:
	$(SHOW)cargo $(CARGO_TOOLCHAIN) test --all

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
	$(SHOW) BINDIR=$(BINDIR) ./sbin/pack.sh $(abspath $(TARGET))

upload-release:
	$(SHOW)RELEASE=1 ./sbin/upload-artifacts

upload-artifacts:
	$(SHOW)SNAPSHOT=1 ./sbin/upload-artifacts

.PHONY: pack upload-artifacts upload-release

#----------------------------------------------------------------------------------------------

clang-install:
	./sbin/install_clang.sh

.PHONY: clang-install

#----------------------------------------------------------------------------------------------

COV_EXCLUDE_DIRS += bin deps tests
COV_EXCLUDE.llvm += $(foreach D,$(COV_EXCLUDE_DIRS),'$(realpath $(ROOT))/$(D)/*')

coverage:
	$(SHOW)$(MAKE) build COV=1
	$(SHOW)$(COVERAGE_RESET.llvm)
	-$(SHOW)$(MAKE) test COV=1
	$(SHOW)$(COVERAGE_COLLECT_REPORT.llvm)

.PHONY: coverage

#----------------------------------------------------------------------------------------------

docker:
	$(SHOW)$(MAKE) -C build/docker
ifeq ($(PUBLISH),1)
	$(SHOW)make -C build/docker publish
endif

.PHONY: docker

#----------------------------------------------------------------------------------------------

ifneq ($(wildcard /w/*),)
SANBOX_ARGS += -v /w:/w
endif

sanbox:
	@docker run -it -v $(PWD):/rejson -w /rejson --cap-add=SYS_PTRACE --security-opt seccomp=unconfined \
		$(SANBOX_ARGS) redisfab/clang:16-x64-focal bash

.PHONY: sanbox

#----------------------------------------------------------------------------------------------

info:
	$(SHOW)if command -v redis-server &> /dev/null; then redis-server --version; fi
	$(SHOW)rustc --version
	$(SHOW)cargo --version
	$(SHOW)rustup --version
	$(SHOW)rustup show
	$(SHOW)if command -v gcc &> /dev/null; then gcc --version; fi
	$(SHOW)if command -v clang &> /dev/null; then clang --version; fi
	$(SHOW)if command -v cmake &> /dev/null; then cmake --version; fi
	$(SHOW)python3 --version
	$(SHOW)python3 -m pip list -v

.PHONY: info
