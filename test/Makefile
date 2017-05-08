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
CFLAGS =  $(INCLUDE_DIRS) -Wall $(DEBUGFLAGS) -fPIC -std=gnu99 -D_GNU_SOURCE
CC:=$(shell sh -c 'type $(CC) >/dev/null 2>/dev/null && echo $(CC) || echo gcc')

# Compile flags for linux / osx
ifneq ($(uname_S),Linux)
	CFLAGS += -mmacosx-version-min=10.6
endif

export CFLAGS
export DEBUGFLAGS

all: build test
test:  test_object test_json_object test_json_printer unittest
build: object json_object json_printer

# Build jsonsl library
jsonsl:
	$(MAKE) -C $(DEPS_DIR)/jsonsl clean static
.PHONY: jsonsl

# Build rmutil library
rmutil:
	$(MAKE) -C $(DEPS_DIR)/RedisModuleSDK/rmutil clean all
.PHONY: rmutil

# Build rejson library
librejson:
	$(MAKE) -C $(RM_INCLUDE_DIR) $@.a
.PHONY: librejson

# Build rejson module
rejson:
	$(MAKE) -C $(RM_INCLUDE_DIR) $@.so
.PHONY: rejson

# Dependency libraries
LIBS_DIRS = -L$(DEPS_DIR)/jsonsl -L$(DEPS_DIR)/RedisModuleSDK/rmutil -L$(RM_INCLUDE_DIR)
LIBS = -ljsonsl -lrmutil -lrejson

#building of json printer test
json_printer:
	$(CC) $(CFLAGS) -o test_$@ json_printer.c $$(LIBS_DIRS) (LIBS)

# execute json_printer test
test_json_printer: json_printer
	./$@
.PHONY: test_json_printer

# Build test_object
object: jsonsl rmutil librejson 
	$(CC) $(CFLAGS) -o test_$@ test_$@.c $(LIBS_DIRS) $(LIBS)

# Run test_object
test_object: object
	./$@
.PHONY: test_object

# Buid json_object
json_object: jsonsl rmutil librejson
	$(CC) $(CFLAGS) -o test_$@ test_$@.c $(LIBS) $(LIBS_DIRS)

# Run test_json_object
test_json_object: json_object
	./$@
.PHONY: test_json_object

# Unit testing
unittest: rejson
	$(MAKE) -C pytest

clean:
	rm -rvf *.xo *.so *.o *.a
	rm -rvf json_printer test_json_object test_object
