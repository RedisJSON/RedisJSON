#!/bin/bash

# [[ $VERBOSE == 1 ]] && set -x
# [[ $IGNERR == 1 ]] || set -e

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
export ROOT=$(cd $HERE/../.. && pwd)
READIES=$ROOT/deps/readies
. $READIES/shibumi/defs

cd $HERE

#----------------------------------------------------------------------------------------------

help() {
	cat <<-END
		Run Python tests
	
		[ARGVARS...] tests.sh [--help|help] [<module-so-path>]
		
		Argument variables:
		VERBOSE=1        Print commands
		IGNERR=1         Do not abort on error
		NOP=1            Dry run

		MODULE=path      Path to redisai.so
		TESTMOD=path     Path to API test module
		
		GEN=0|1          General tests
		AOF=0|1          Tests with --test-aof
		SLAVES=0|1       Tests with --test-slaves
		
		TEST=test        Run specific test (e.g. test.py:test_name)
		LOG=0|1          Write to log
		VALGRIND|VD=1    Run with Valgrind
		CALLGRIND|CG=1   Run with Callgrind
		MEMINFO=1        Show memory information
		REDIS=addr       Use redis-server at addr

	END
}

#----------------------------------------------------------------------------------------------

install_git_lfs() {
	[[ $NO_LFS == 1 ]] && return
	[[ $(git lfs env > /dev/null 2>&1 ; echo $?) != 0 ]] && git lfs install
	git lfs pull
}

#----------------------------------------------------------------------------------------------

check_redis_server() {
	if ! command -v redis-server > /dev/null; then
		echo "Cannot find redis-server. Aborting."
		exit 1
	fi
}

#----------------------------------------------------------------------------------------------

valgrind_config() {
	export VG_OPTIONS="
		-q \
		--leak-check=full \
		--show-reachable=no \
		--show-possibly-lost=no"

	VALGRIND_SUPRESSIONS=$ROOT/tests/redis_valgrind.sup

	RLTEST_ARGS+="\
		--use-valgrind \
		--vg-suppressions $VALGRIND_SUPRESSIONS"
}

valgrind_summary() {
	# Collect name of each flow log that contains leaks
	FILES_WITH_LEAKS=$(grep -l "definitely lost" logs/*.valgrind.log)
	if [[ ! -z $FILES_WITH_LEAKS ]]; then
		echo "Memory leaks introduced in flow tests."
		echo $FILES_WITH_LEAKS
		# Print the full Valgrind output for each leaking file
		echo $FILES_WITH_LEAKS | xargs cat
		exit 1
	else
		echo Valgrind test ok
	fi
}

#----------------------------------------------------------------------------------------------

run_tests() {
	local title="$1"
	[[ ! -z $title ]] && { $ROOT/opt/readies/bin/sep -0; printf "Tests with $title:\n\n"; }
	cd $ROOT/tests/pytest
	[[ ! -z $TESTMOD ]] && RLTEST_ARGS+=--module $TESTMOD
	$OP python3 -m RLTest --clear-logs --module $MODULE --module-args "JSON_BACKEND SERDE_JSON" $RLTEST_ARGS
	$OP python3 -m RLTest --clear-logs --module $MODULE $RLTEST_ARGS
}

#----------------------------------------------------------------------------------------------

[[ $1 == --help || $1 == help ]] && { help; exit 0; }

GEN=${GEN:-1}
SLAVES=${SLAVES:-0}
AOF=${AOF:-0}

GDB=${GDB:-0}

OP=""
[[ $NOP == 1 ]] && OP="echo"

MODULE=${MODULE:-$1}
[[ -z $MODULE || ! -f $MODULE ]] && { echo "Module not found at ${MODULE}. Aborting."; exit 1; }

[[ ! -z $TESTMOD ]] && echo "Test module path is ${TESTMOD}"

[[ $VALGRIND == 1 || $VD == 1 ]] && valgrind_config

if [[ ! -z $TEST ]]; then
	RLTEST_ARGS+=" --test $TEST"
	if [[ $LOG != 1 ]]; then
		RLTEST_ARGS+=" -s"
		export BB=${BB:-1}
	fi
	export RUST_BACKTRACE=1
fi

[[ $VERBOSE == 1 ]] && RLTEST_ARGS+=" -v"
[[ $GDB == 1 ]] && RLTEST_ARGS+=" -i --verbose"

export OS=$($READIES/bin/platform --os)

#----------------------------------------------------------------------------------------------

cd $ROOT/tests/pytest

# install_git_lfs
check_redis_server

if [[ ! -z $REDIS ]]; then
	RL_TEST_ARGS+=" --env exiting-env --existing-env-addr $REDIS"
fi

if [[ $CLUSTER == 1 ]]; then
	RLTEST_ARGS+=" --env oss-cluster --shards-count 1" run_tests "--env oss-cluster"
elif [[ $VALGRIND != 1 && $SLAVES == 1 ]]; then
	RLTEST_ARGS+=" --use-slaves" run_tests "--use-slaves"
elif [[ $AOF == 1 ]]; then
	RLTEST_ARGS+=" --use-aof" run_tests "--use-aof"
else
	run_tests
fi
[[ $VG == 1 ]] && valgrind_summary
exit 0
