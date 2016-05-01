#set environment variable RM_INCLUDE_DIR to the location of redismodule.h

CFLAGS = -g -fPIC -lc -lm -O3 -std=gnu99 -I$(RM_INCLUDE_DIR)
CC=gcc
CFLAGS = -I$(RM_INCLUDE_DIR) 
.SUFFIXES: .c .so .xo .o

OBJS=util.o strings.o sds.o

all: rmutil.a

clean:
	rm -rf *.xo *.so *.o

rmutil.a: $(OBJS)
	ar rcs $@ $^

