#set environment variable RM_INCLUDE_DIR to the location of redismodule.h
ifndef RM_INCLUDE_DIR
	RM_INCLUDE_DIR=../../
endif

# find the OS
uname_S := $(shell sh -c 'uname -s 2>/dev/null || echo not')

# Compile flags for linux / osx
ifeq ($(uname_S),Linux)
	SHOBJ_CFLAGS ?=  -fno-common -g -ggdb
	SHOBJ_LDFLAGS ?= -shared
else
	SHOBJ_CFLAGS ?= -dynamic -fno-common -g -ggdb
	SHOBJ_LDFLAGS ?= -bundle -undefined dynamic_lookup
endif
CFLAGS = -I$(RM_INCLUDE_DIR) -g -fPIC -lc -lm -Og -std=gnu99 
CC=gcc
.SUFFIXES: .c .so .xo .o

all: module.so 

.c.xo:
	$(CC) -I. $(CFLAGS) $(SHOBJ_CFLAGS) -fPIC -c $< -o $@

#mdma.xo: ./index.h

module.so: module.o
	$(LD) -o $@ module.o $(SHOBJ_LDFLAGS) $(LIBS) -L../ -lrmutil -lc 

clean:
	rm -rf *.xo *.so *.o

