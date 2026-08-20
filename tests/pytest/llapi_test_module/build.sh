#!/usr/bin/env bash
#
# Builds the LLAPI test consumer module and prints the path of the produced .so
# on stdout (everything else goes to stderr, so callers can capture the path).
#
# Two callers:
#   - tests/pytest/tests.sh, for the test_llapi.py flow tests
#   - .github/workflows/benchmark-flow.yml, which loads it next to the module
#     under test so the benchmarks can drive the LLAPI through LLAPI.* commands
#
set -e

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"

if [[ ! -f "$ROOT/deps/RedisModulesSDK/redismodule.h" ]]; then
	git -C "$ROOT" submodule update --init deps/RedisModulesSDK >&2
fi

case "$(uname -s)" in
	Darwin) LLAPI_TEST_LDFLAGS="-bundle -undefined dynamic_lookup" ;;
	*)      LLAPI_TEST_LDFLAGS="-shared" ;;
esac

"${LLAPI_TEST_CC:-cc}" -fPIC $LLAPI_TEST_LDFLAGS -O2 -Wall \
	-I"$ROOT/deps/RedisModulesSDK" -I"$ROOT/redis_json/src/include" \
	-o "$HERE/llapi_test.so" "$HERE/module.c" >&2

echo "$HERE/llapi_test.so"
