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
INCLUDE_DIRS = -I"$(RM_INCLUDE_DIR)" -I"$(DEPS_DIR)/jsonsl"  -I"$(DEPS_DIR)/RedisModuleSDK/rmutil"
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

all: rejson.so librejson.a

jsonsl:
	$(MAKE) -C $(DEPS_DIR)/jsonsl static
.PHONY: jsonsl

rmutil:
	$(MAKE) -C $(DEPS_DIR)/RedisModuleSDK/rmutil all

.PHONY: rmutil

rejson.so: jsonsl rmutil $(CC_OBJECTS)
	$(LD) -o $@ $(CC_OBJECTS) $(LIBS) $(SHOBJ_LDFLAGS) -lc -lm

librejson.a: jsonsl rmutil $(CC_OBJECTS)
	ar rcs $@ $(LIBS) $(CC_OBJECTS)

clean:
	rm -rvf *.xo *.so *.o *.a

deepclean:
	find ./ -name "*.[oa]" -exec rm {} \; -print
	find ./ -name "*.so" -exec rm {} \; -print

package: rejson.so
	mkdir -p ../build
	module_packer -v \
		-o "../build/rejson.{os}-{architecture}.latest.zip" \
		"`pwd`/rejson.so"