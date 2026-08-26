
CC = gcc
CPPFLAGS = -I../../src/include -DREDISMODULE_EXPERIMENTAL_API
CFLAGS = -Wall -g -fPIC -lc -lm -std=gnu99

LDFLAGS = -fno-common -g -ggdb -shared -Bsymbolic
LDLIBS = -lc

include deps
.DEFAULT_GOAL = mod

.PHONY: all mod
all mod: libtestmod.so

ifneq ($(C),1)
libtestmod.so: src/lib.rs
	cargo build
	cp target/debug/$@ $@
else
libtestmod.so: testLLAPI.o
	$(LINK.o) -o $@ $^ $(LDLIBS)
endif

deps: src/testLLAPI.c
	$(CC) $(CPPFLAGS) -MM $< -MF $@

testLLAPI.o: src/testLLAPI.c
	$(COMPILE.c) -o $@ $<

.PHONY: clean run

run: libtestmod.so
	redis-server --loadmodule ./$^ --loadmodule ../../target/release/librejson.so &
	sleep 1
	redis-cli RJ_llapi.test_all
	killall redis-server

clean:
	rm -rf deps *.o *.so *.rdb