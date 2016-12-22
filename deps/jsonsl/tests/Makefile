TESTMODS= json_test jpr_test unescape cxxtest

all: $(TESTMODS)
	./json_test ../share/*
	./jpr_test
	./unescape
	./json_test ../share/jsc/pass*.json
	JSONSL_FAIL_TESTS=1 ./json_test ../share/jsc/fail*.json
	@echo "   Some tests were skipped."
	@echo "   See share/jsc/nyi_fail/README.skipped for details"
	@echo "All Tests OK"

%: %.c
	$(CC) $(CFLAGS) $^ -o $@ $(LDFLAGS)

clean:
	rm -f $(TESTMODS)
