# Set environment variable RM_INCLUDE_DIR to the location of redismodule.h
ifndef RM_INCLUDE_DIR
	RM_INCLUDE_DIR=$(shell pwd)
endif

DEPS_DIR="$(RM_INCLUDE_DIR)/../deps"

# Setting the DEBUG env variable to 1 will cause us to build with -O0
ifndef DEBUG
	DEBUG = 0
endif
DEBUGFLAGS = -g -ggdb -O2
ifeq ($(DEBUG), 1)
	DEBUGFLAGS = -g -ggdb -O0
endif

# Find the OS
uname_S := $(shell sh -c 'uname -s 2>/dev/null || echo not')
INCLUDE_DIRS = -I"$(RM_INCLUDE_DIR)" -I"$(DEPS_DIR)/jsonsl"  -I"$(DEPS_DIR)/RedisModuleSDK"
CFLAGS = $(INCLUDE_DIRS) -Wall $(DEBUGFLAGS) -fPIC -std=gnu99  -D_GNU_SOURCE
CC:=$(shell sh -c 'type $(CC) >/dev/null 2>/dev/null && echo $(CC) || echo gcc')

# Compile flags for linux / osx
ifeq ($(uname_S),Linux)
	SHOBJ_CFLAGS ?=  -fno-common -g -ggdb
	SHOBJ_LDFLAGS ?= -shared -Bsymbolic -Bsymbolic-functions
else
	CFLAGS += -mmacosx-version-min=10.6
	SHOBJ_CFLAGS ?= -dynamic -fno-common -g -ggdb
	SHOBJ_LDFLAGS ?= -dylib -exported_symbol _RedisModule_OnLoad -macosx_version_min 10.6
endif
export CFLAGS
export DEBUGFLAGS

SOURCEDIR=$(shell pwd -P)
CC_SOURCES = $(wildcard $(SOURCEDIR)/*.c) $(wildcard $(SOURCEDIR)/dep/*.c)
CC_OBJECTS = $(patsubst $(SOURCEDIR)/%.c, $(SOURCEDIR)/%.o, $(CC_SOURCES))
LIBS = $(DEPS_DIR)/jsonsl/jsonsl.o $(DEPS_DIR)/RedisModuleSDK/rmutil/librmutil.a
MODULE=rejson.so

all: $(MODULE) librejson.a

jsonsl:
	$(MAKE) -C $(DEPS_DIR)/jsonsl static
.PHONY: jsonsl

rmutil:
	$(MAKE) -C $(DEPS_DIR)/RedisModuleSDK/rmutil all

.PHONY: rmutil

$(MODULE): jsonsl rmutil $(CC_OBJECTS)
	$(LD) -o $@ $(CC_OBJECTS) $(LIBS) $(SHOBJ_LDFLAGS) -lc -lm

librejson.a: jsonsl rmutil $(CC_OBJECTS)
	ar rcs $@ $(LIBS) $(CC_OBJECTS)

clean:
	rm -rvf *.xo *.so *.o *.a

deepclean:
	find ./ -name "*.[oa]" -exec rm {} \; -print
	find ./ -name "*.so" -exec rm {} \; -print

RAMP_COMMAND=ramp-packer -v -m ramp.yml
PACKAGE_NAME=rejson
BUILD_DIR=../build
## Package the module into a RAMP package on master builds and version tag builds
package: $(MODULE) print_version
	@mkdir -p $(BUILD_DIR) && rm -f $(BUILD_DIR)/*
	@# On release tags (v1.2.3) - package with the version, and package the latest-stable
	@set -e; if (expr "$$TRAVIS_TAG" : "^v[0-9]"); then\
		$(RAMP_COMMAND) --output "$(BUILD_DIR)/$(PACKAGE_NAME).{os}-{architecture}.`./print_version`.zip" "`pwd`/$(MODULE)";\
		$(RAMP_COMMAND) --output "$(BUILD_DIR)/$(PACKAGE_NAME).{os}-{architecture}.latest.zip" "`pwd`/$(MODULE)";\
	fi

	@# On master builds - package the master latest
	@set -e; if [ "$$TRAVIS_BRANCH" = "master" ]; then\
		$(RAMP_COMMAND) --output "$(BUILD_DIR)/$(PACKAGE_NAME).{os}-{architecture}.master.zip" "`pwd`/$(MODULE)";\
	fi
# Compile an executable that prints the current version
print_version: version.h print_version.c
	@$(CC) -o $@ -DPRINT_VERSION_TARGET $@.c
