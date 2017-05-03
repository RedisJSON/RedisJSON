#set environment variable RM_INCLUDE_DIR to the location of redismodule.h
ifndef RM_INCLUDE_DIR
	RM_INCLUDE_DIR=$(shell pwd)/../src
endif

DEPS_DIR=$(shell pwd)/../deps

# Setting the DEBUG env variable to 1 will cause us to build with -O0
ifndef DEBUG
	DEBUG = 0
endif
DEBUGFLAGS = -g -ggdb -O2
ifeq ($(DEBUG), 1)
	DEBUGFLAGS = -g -ggdb -O0
endif

# find the OS
uname_S := $(shell sh -c 'uname -s 2>/dev/null || echo not')
INCLUDE_DIRS = -I"$(RM_INCLUDE_DIR)" -I"$(DEPS_DIR)/jsonsl"  -I"$(DEPS_DIR)/RedisModuleSDK/rmutil"
CFLAGS =  $(INCLUDE_DIRS) -Wall $(DEBUGFLAGS) -fPIC -std=gnu99  -D_GNU_SOURCE  -D_REDIS_MODULE_TARGET
CC:=$(shell sh -c 'type $(CC) >/dev/null 2>/dev/null && echo $(CC) || echo gcc')

# Compile flags for linux / osx
ifneq ($(uname_S),Linux)
	CFLAGS += -mmacosx-version-min=10.6
endif

export CFLAGS
export DEBUGFLAGS

all: build test
test:  test_object test_json_object test_json_printer
build: object json_object json_printer

# Build jsonsl library
jsonsl:
	$(MAKE) -C $(DEPS_DIR)/jsonsl static
.PHONY: jsonsl

# build rmutil library
rmutil:
	$(MAKE) -C $(DEPS_DIR)/RedisModuleSDK/rmutil clean all
.PHONY: rmutil

librejson:
	$(MAKE) -C $(RM_INCLUDE_DIR) $@.a
.PHONY: librejson

# Dependency libraries
LIBS = $(DEPS_DIR)/jsonsl/libjsonsl.a $(DEPS_DIR)/RedisModuleSDK/rmutil/librmutil.a $(RM_INCLUDE_DIR)/librejson.a

#building of json printer test
json_printer:
	$(CC) $(CFLAGS) -o test_$@ $(LIBS) json_printer.c

# execute json_printer test
test_json_printer: json_printer
	./$@
.PHONY: test_json_printer

# Build test_object
object: jsonsl rmutil librejson
	$(CC) $(CFLAGS) -o test_$@ $(LIBS) test_$@.c
# Run test_object
test_object: object
	./$@
.PHONY: test_object

# Buid json_object
json_object: jsonsl rmutil librejson
	$(CC) $(CFLAGS) -o test_$@ $(LIBS) test_$@.c
# Run test_json_object
test_json_object: json_object
	./$@
.PHONY: test_json_object

clean:
	rm -rvf *.xo *.so *.o *.a


