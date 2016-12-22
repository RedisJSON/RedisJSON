all: bench yajl-perftest

CFLAGS+= -Wno-overlength-strings -fvisibility=hidden -DJSONSL_NO_JPR -DNDEBUG
#CFLAGS+=-DJSONSL_USE_METRICS
BENCH_LFLAGS=

bench: bench.c ../jsonsl.c
	$(CC) -o $@ $(CFLAGS) $^ $(BENCH_LFLAGS)

yajl-perftest: documents.c perftest.c ../jsonsl.c
	$(CC) $(CFLAGS) $^ -o $@ $(BENCH_LFLAGS)

.PHONY: run-benchmarks

run-benchmarks: bench yajl-perftest
	@echo "Running against single file"
	./bench ../share/auction 100
	@echo "Running yajl tests on JSONSL"
	./yajl-perftest

clean:
	-rm -f bench yajl-perftest
