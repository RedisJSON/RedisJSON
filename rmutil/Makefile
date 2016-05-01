# set environment variable RM_INCLUDE_DIR to the location of redismodule.h
ifndef RM_INCLUDE_DIR
	RM_INCLUDE_DIR=../
endif

CFLAGS = -g -fPIC -lc -lm -O3 -std=gnu99 -I$(RM_INCLUDE_DIR)
CC=gcc

OBJS=util.o strings.o sds.o

all: librmutil.a

clean:
	rm -rf *.o *.a

librmutil.a: $(OBJS)
	ar rcs $@ $^

