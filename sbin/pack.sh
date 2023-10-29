#!/bin/bash

PROGNAME="${BASH_SOURCE[0]}"
HERE="$(cd "$(dirname "$PROGNAME")" &>/dev/null && pwd)"
ROOT=$(cd $HERE/.. && pwd)
export READIES=$ROOT/deps/readies
. $READIES/shibumi/defs

SBIN=$ROOT/sbin

export PYTHONWARNINGS=ignore

cd $ROOT

#----------------------------------------------------------------------------------------------

if [[ $1 == --help || $1 == help || $HELP == 1 ]]; then
	cat <<-END
		Generate RedisJSON distribution packages.

		[ARGVARS...] pack.sh [--help|help]

		Argument variables:
		MODULE=path         Path of module .so

		RAMP=0|1            Build RAMP package
		DEPS=0|1            Build dependencies files
		SYM=0|1             Build debug symbols file

		BRANCH=name         Branch name for snapshot packages
		WITH_GITSHA=1       Append Git SHA to shapshot package names
		VARIANT=name        Build variant
		RAMP_VARIANT=name   RAMP variant (e.g. ramp-{name}.yml)

		ARTDIR=dir          Directory in which packages are created (default: bin/artifacts)
		
		RAMP_YAML=path      RAMP configuration file path
		RAMP_ARGS=args      Extra arguments to RAMP

		JUST_PRINT=1        Only print package names, do not generate
		VERBOSE=1           Print commands
		HELP=1              Show help
		NOP=1               Print commands, do not execute

	END
	exit 0
fi

#----------------------------------------------------------------------------------------------

OP=""
[[ $NOP == 1 ]] && OP=echo

# RLEC naming conventions

ARCH=$($READIES/bin/platform --arch)
[[ $ARCH == x64 ]] && ARCH=x86_64
[[ $ARCH == arm64v8 ]] && ARCH=aarch64

OS=$($READIES/bin/platform --os)
[[ $OS == linux ]] && OS=Linux

OSNICK=$($READIES/bin/platform --osnick)
[[ $OSNICK == trusty ]]  && OSNICK=ubuntu14.04
[[ $OSNICK == xenial ]]  && OSNICK=ubuntu16.04
[[ $OSNICK == bionic ]]  && OSNICK=ubuntu18.04
[[ $OSNICK == focal ]]   && OSNICK=ubuntu20.04
[[ $OSNICK == jammy ]]   && OSNICK=ubuntu22.04
[[ $OSNICK == centos7 ]] && OSNICK=rhel7
[[ $OSNICK == centos8 ]] && OSNICK=rhel8
[[ $OSNICK == centos9 ]] && OSNICK=rhel9
[[ $OSNICK == ol8 ]]     && OSNICK=rhel8
[[ $OSNICK == rocky8 ]]  && OSNICK=rhel8
[[ $OSNICK == rocky9 ]]  && OSNICK=rhel9

if [[ $OS == macos ]]; then
	# as we don't build on macOS for every platform, we converge to a least common denominator
	if [[ $ARCH == x86_64 ]]; then
		OSNICK=catalina  # to be aligned with the rest of the modules in redis stack
	else
		[[ $OSNICK == ventura ]] && OSNICK=monterey
	fi
fi

PLATFORM="$OS-$OSNICK-$ARCH"

#----------------------------------------------------------------------------------------------

if [[ -z $MODULE || ! -f $MODULE ]]; then
	eprint "MODULE is not defined or does not refer to a file"
	exit 1
fi

RAMP=${RAMP:-1}
DEPS=${DEPS:-1}
SYM=${SYM:-1}

[[ -z $ARTDIR ]] && ARTDIR=bin/artifacts
mkdir -p $ARTDIR $ARTDIR/snapshots
ARTDIR=$(cd $ARTDIR && pwd)

MODULE_NAME=${MODULE_NAME:-ReJSON}
PACKAGE_NAME=rejson-oss

DEP_NAMES=""

RAMP_CMD="python3 -m RAMP.ramp"

#----------------------------------------------------------------------------------------------

pack_ramp() {
	cd $ROOT

	local stem=${PACKAGE_NAME}.${PLATFORM}

	local verspec=${SEMVER}${_VARIANT}
	
	local fq_package=$stem.${verspec}.zip

	[[ ! -d $ARTDIR ]] && mkdir -p $ARTDIR

	local packfile="$ARTDIR/$fq_package"

	local xtx_vars=""
	local dep_fname="${PACKAGE_NAME}.${PLATFORM}.${verspec}.tgz"

	if [[ -z $RAMP_YAML ]]; then
		RAMP_YAML=$ROOT/ramp.yml
	elif [[ -z $RAMP_VARIANT ]]; then
		RAMP_YAML=$ROOT/ramp.yml
	else
		RAMP_YAML=$ROOT/ramp${_RAMP_VARIANT}.yml
	fi

	python3 $READIES/bin/xtx \
		$xtx_vars \
		-e NUMVER -e SEMVER \
		$RAMP_YAML > /tmp/ramp.yml
	if [[ $VERBOSE == 1 ]]; then
		echo "# ramp.yml:"
		cat /tmp/ramp.yml
	fi

	runn rm -f /tmp/ramp.fname $packfile
	
	# ROOT is required so ramp will detect the right git commit
	cd $ROOT
	runn @ <<-EOF
		$RAMP_CMD pack -m /tmp/ramp.yml \
			$RAMP_ARGS \
			-n $MODULE_NAME \
			--verbose \
			--debug \
			--packname-file /tmp/ramp.fname \
			-o $packfile \
			$MODULE \
			>/tmp/ramp.err 2>&1 || true
		EOF

	if [[ $NOP != 1 ]]; then
		if [[ ! -e $packfile ]]; then
			eprint "Error generating RAMP file:"
			>&2 cat /tmp/ramp.err
			exit 1
		else
			local packname=`cat /tmp/ramp.fname`
			echo "# Created $packname"
		fi
	fi

	cd $ARTDIR/snapshots
	if [[ ! -z $BRANCH ]]; then
		local snap_package=$stem.${BRANCH}${_VARIANT}.zip
		ln -sf ../$fq_package $snap_package
	fi

	local packname=`cat /tmp/ramp.fname`
	echo "Created $packname"
	cd $ROOT
}

#----------------------------------------------------------------------------------------------

pack_deps() {
	local dep="$1"

	local stem=${PACKAGE_NAME}.${dep}.${PLATFORM}
	local verspec=${SEMVER}${_VARIANT}

	local depdir=$(cat $ARTDIR/$dep.dir)

	local fq_dep=$stem.${verspec}.tgz
	local tar_path=$ARTDIR/$fq_dep
	local dep_prefix_dir=$(cat $ARTDIR/$dep.prefix)
	
	{ cd $depdir ;\
	  cat $ARTDIR/$dep.files | \
	  xargs tar -c --sort=name --owner=root:0 --group=root:0 --mtime='UTC 1970-01-01' \
		--transform "s,^,$dep_prefix_dir," 2> /tmp/pack.err | \
	  gzip -n - > $tar_path ; E=$?; } || true
	rm -f $ARTDIR/$dep.prefix $ARTDIR/$dep.files $ARTDIR/$dep.dir

	cd $ROOT
	if [[ $E != 0 || -s /tmp/pack.err ]]; then
		eprint "Error creating $tar_path:"
		cat /tmp/pack.err >&2
		exit 1
	fi
	runn @ <<-EOF
		sha256sum $tar_path | awk '{print $1}' > $tar_path.sha256
		EOF

	cd $ARTDIR/snapshots
	if [[ -n $BRANCH ]]; then
		local snap_dep=$stem.${BRANCH}${_VARIANT}.tgz
		runn ln -sf ../$fq_dep $snap_dep
		runn ln -sf ../$fq_dep.sha256 $snap_dep.sha256
	fi

	cd $ROOT
}

#----------------------------------------------------------------------------------------------

prepare_symbols_dep() {
	if [[ ! -f $MODULE.debug ]]; then return 0; fi
	echo "# Preparing debug symbols dependencies ..."
	echo $(cd "$(dirname $MODULE)" && pwd) > $ARTDIR/debug.dir
	echo $(basename $MODULE.debug) > $ARTDIR/debug.files
	echo "" > $ARTDIR/debug.prefix
	pack_deps debug
	echo "# Done."
}

#----------------------------------------------------------------------------------------------

NUMVER="$(NUMERIC=1 $SBIN/getver)"
SEMVER="$($SBIN/getver)"

if [[ -n $VARIANT ]]; then
	_VARIANT="-${VARIANT}"
fi
if [[ ! -z $RAMP_VARIANT ]]; then
	_RAMP_VARIANT="-${RAMP_VARIANT}"
fi

#----------------------------------------------------------------------------------------------

if [[ -z $BRANCH ]]; then
	BRANCH=$(git rev-parse --abbrev-ref HEAD)
	# this happens of detached HEAD
	if [[ $BRANCH == HEAD ]]; then
		BRANCH="$SEMVER"
	fi
fi
BRANCH=${BRANCH//[^A-Za-z0-9._-]/_}
if [[ $WITH_GITSHA == 1 ]]; then
	GIT_COMMIT=$(git rev-parse --short HEAD)
	BRANCH="${BRANCH}-${GIT_COMMIT}"
fi
export BRANCH

#----------------------------------------------------------------------------------------------

if [[ $JUST_PRINT == 1 ]]; then
	if [[ $RAMP == 1 ]]; then
		echo "${PACKAGE_NAME}.${OS}-${OSNICK}-${ARCH}.${SEMVER}${VARIANT}.zip"
	fi
	if [[ $DEPS == 1 ]]; then
		for dep in $DEP_NAMES; do
			echo "${PACKAGE_NAME}.${dep}.${OS}-${OSNICK}-${ARCH}.${SEMVER}${VARIANT}.tgz"
		done
	fi
	exit 0
fi

#----------------------------------------------------------------------------------------------

mkdir -p $ARTDIR

if [[ $DEPS == 1 ]]; then
	echo "# Building dependencies ..."

	[[ $SYM == 1 ]] && prepare_symbols_dep

	for dep in $DEP_NAMES; do
		echo "# $dep ..."
		pack_deps $dep
	done
	echo "# Done."
fi

#----------------------------------------------------------------------------------------------

cd $ROOT

if [[ $RAMP == 1 ]]; then
	if ! command -v redis-server > /dev/null; then
		eprint "Cannot find redis-server. Aborting."
		exit 1
	fi

	echo "# Building RAMP $RAMP_VARIANT files ..."
	pack_ramp
	echo "# Done."
fi

if [[ $VERBOSE == 1 ]]; then
	echo "# Artifacts:"
	$OP du -ah --apparent-size $ARTDIR
fi

exit 0
