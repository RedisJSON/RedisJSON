#!/bin/bash

[[ $IGNERR == 1 ]] || set -e
# [[ $VERBOSE == 1 ]] && set -x

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
		MODULE=path      Path to redisai.so
		TESTMOD=path     Path to API test module
		TEST=test        Run specific test (e.g. test.py:test_name)
		REDIS=addr       Use redis-server at addr
		
		GEN=0|1          General tests
		AOF=0|1          Tests with --test-aof
		SLAVES=0|1       Tests with --test-slaves
		
		VALGRIND|VG=1    Run with Valgrind
		SAN=type         Use LLVM sanitizer (type=address|memory|leak|thread) 

		VERBOSE=1        Print commands
		IGNERR=1         Do not abort on error
		NOP=1            Dry run
		LOG=0|1          Write to log

	END
}

#----------------------------------------------------------------------------------------------

install_git_lfs() {
	[[ $NO_LFS == 1 ]] && return
	[[ $(git lfs env > /dev/null 2>&1 ; echo $?) != 0 ]] && git lfs install
	git lfs pull
}

#----------------------------------------------------------------------------------------------

setup_redis_server() {
	if [[ -n $SAN ]]; then
		if [[ $SAN == addr || $SAN == address ]]; then
			REDIS_SERVER=${REDIS_SERVER:-redis-server-asan-6.2}
			if ! command -v $REDIS_SERVER > /dev/null; then
				echo Building Redis for clang-asan ...
				$READIES/bin/getredis --force -v 6.2 --own-openssl --no-run --suffix asan --clang-asan --clang-san-blacklist /build/redis.blacklist
			fi

			export ASAN_OPTIONS=detect_odr_violation=0
			# :detect_leaks=0

		elif [[ $SAN == mem || $SAN == memory ]]; then
			REDIS_SERVER=${REDIS_SERVER:-redis-server-msan-6.2}
			if ! command -v $REDIS_SERVER > /dev/null; then
				echo Building Redis for clang-msan ...
				$READIES/bin/getredis --force -v 6.2  --no-run --own-openssl --suffix msan --clang-msan --llvm-dir /opt/llvm-project/build-msan --clang-san-blacklist /build/redis.blacklist
			fi
		fi

	elif [[ $VG == 1 ]]; then
		REDIS_SERVER=${REDIS_SERVER:-redis-server-vg}
		if ! is_command $REDIS_SERVER; then
			echo Building Redis for Valgrind ...
			$READIES/bin/getredis -v 6 --valgrind --suffix vg
		fi

	else
		REDIS_SERVER=${REDIS_SERVER:-redis-server}
	fi

	if ! is_command $REDIS_SERVER; then
		echo "Cannot find $REDIS_SERVER. Aborting."
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
	shift
	if [[ -n $title ]]; then
		$READIES/bin/sep -0
		printf "Running $title:\n\n"
	fi

	cd $ROOT/tests/pytest
#	[[ ! -z $TESTMOD ]] && RLTEST_ARGS+=--module $TESTMOD
#	$OP python3 -m RLTest --clear-logs --module $MODULE --module-args "JSON_BACKEND SERDE_JSON" $RLTEST_ARGS
#	$OP python3 -m RLTest --clear-logs --module $MODULE $RLTEST_ARGS





	if [[ $EXISTING_ENV != 1 ]]; then
		rltest_config=$(mktemp "${TMPDIR:-/tmp}/rltest.XXXXXXX")
		rm -f $rltest_config
		cat <<-EOF > $rltest_config
			--oss-redis-path=$REDIS_SERVER
			--module $MODULE
			--module-args '$MODARGS'
			$RLTEST_ARGS
			$VALGRIND_ARGS
			$SAN_ARGS
			$COV_ARGS

			EOF

	else # existing env
		xredis_conf=$(mktemp "${TMPDIR:-/tmp}/xredis_conf.XXXXXXX")
		rm -f $xredis_conf
		cat <<-EOF > $xredis_conf
			loadmodule $MODULE $MODARGS
			EOF

		rltest_config=$(mktemp "${TMPDIR:-/tmp}/xredis_rltest.XXXXXXX")
		rm -f $rltest_config
		cat <<-EOF > $rltest_config
			--env existing-env
			$RLTEST_ARGS

			EOF

		if [[ $VERBOSE == 1 ]]; then
			echo "External redis-server configuration:"
			cat $xredis_conf
		fi

		$REDIS_SERVER $xredis_conf &
		XREDIS_PID=$!
		echo "External redis-server pid: " $XREDIS_PID
	fi

	# Use configuration file in the current directory if it exists
	if [[ -n $CONFIG_FILE && -e $CONFIG_FILE ]]; then
		cat $CONFIG_FILE >> $rltest_config
	fi

	if [[ $VERBOSE == 1 ]]; then
		echo "RLTest configuration:"
		cat $rltest_config
	fi

	local E=0
	if [[ $NOP != 1 ]]; then
		{ $OP python3 -m RLTest @$rltest_config; (( E |= $? )); } || true
	else
		$OP python3 -m RLTest @$rltest_config
	fi
	rm -f $rltest_config

	if [[ -n $XREDIS_PID ]]; then
		echo "killing external redis-server: $XREDIS_PID"
		kill -9 $XREDIS_PID
	fi

	return $E
}

#----------------------------------------------------------------------------------------------

[[ $1 == --help || $1 == help ]] && { help; exit 0; }

GEN=${GEN:-1}
SLAVES=${SLAVES:-0}
AOF=${AOF:-0}

GDB=${GDB:-0}

OP=""
[[ $NOP == 1 ]] && OP="echo"

[[ $VG == 1 ]] && VALGRIND=1
[[ $SAN == addr ]] && SAN=address
[[ $SAN == mem ]] && SAN=memory

MODULE=${MODULE:-$1}
[[ -z $MODULE || ! -f $MODULE ]] && { echo "Module not found at ${MODULE}. Aborting."; exit 1; }

[[ ! -z $TESTMOD ]] && echo "Test module path is ${TESTMOD}"

[[ $VALGRIND == 1 || $VG == 1 ]] && valgrind_config

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
setup_redis_server

if [[ ! -z $REDIS ]]; then
	RL_TEST_ARGS+=" --env exiting-env --existing-env-addr $REDIS"
fi

E=0

if [[ $CLUSTER == 1 ]]; then
	{ (RLTEST_ARGS+=" --env oss-cluster --shards-count 1" run_tests "--env oss-cluster"); (( E |= $? )); } || true
	{ (MODARGS+=" JSON_BACKEND SERDE_JSON" RLTEST_ARGS+=" --env oss-cluster --shards-count 1" run_tests "--env oss-cluster (with serde_json)"); (( E |= $? )); } || true
elif [[ $VALGRIND != 1 && $SLAVES == 1 ]]; then
	{ (RLTEST_ARGS+=" --use-slaves" run_tests "--use-slaves"); (( E |= $? )); } || true
	{ (MODARGS+=" JSON_BACKEND SERDE_JSON" RLTEST_ARGS+=" --use-slaves" run_tests "--use-slaves (with serde_json)"); (( E |= $? )); } || true
elif [[ $AOF == 1 ]]; then
	{ (RLTEST_ARGS+=" --use-aof" run_tests "--use-aof"); (( E |= $? )); } || true
	{ (MODARGS+=" JSON_BACKEND SERDE_JSON" RLTEST_ARGS+=" --use-aof" run_tests "--use-aof (with serde_json)"); (( E |= $? )); } || true
else
	{ (run_tests "general"); (( E |= $? )); } || true
	{ (MODARGS+=" JSON_BACKEND SERDE_JSON" run_tests "general (with serde_json)"); (( E |= $? )); } || true
fi
[[ $VG == 1 ]] && valgrind_summary
exit $E
