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
		MODULE=path       Path of module .so

		RAMP=1|0          Build RAMP file
		DEPS=0|1          Build dependencies file
		SYM=0|1           Build debug symbols file

		BRANCH=name       Branch name for snapshot packages
		WITH_GITSHA=1     Append Git SHA to shapshot package names
		VARIANT=name      Build variant (default: empty)

		ARTDIR=dir        Directory in which packages are created (default: bin/artifacts)

		JUST_PRINT=1      Only print package names, do not generate
		VERBOSE=1         Print commands
		IGNERR=1          Do not abort on error

	END
	exit 0
fi

#----------------------------------------------------------------------------------------------

# RLEC naming conventions

ARCH=$($READIES/bin/platform --arch)
[[ $ARCH == x64 ]] && ARCH=x86_64

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
[[ $OSNICK == ol8 ]]     && OSNICK=rhel8
[[ $OSNICK == rocky8 ]]  && OSNICK=rhel8

[[ $OSNICK == bigsur ]]  && OSNICK=catalina

#----------------------------------------------------------------------------------------------

if [[ -z $MODULE || ! -f $MODULE ]]; then
	eprint "MODULE is undefined or is not referring to a file"
	exit 1
fi

RAMP=${RAMP:-1}
DEPS=${DEPS:-1}
SYM=${SYM:-1}
DEPNAMES=""

[[ -z $ARTDIR ]] && ARTDIR=bin/artifacts
mkdir -p $ARTDIR $ARTDIR/snapshots
ARTDIR=$(cd $ARTDIR && pwd)

export DEPNAMES=""

PACKAGE_NAME=rejson-oss

RAMP_CMD="python3 -m RAMP.ramp"

#----------------------------------------------------------------------------------------------

pack_ramp() {
	cd $ROOT

	local platform="$OS-$OSNICK-$ARCH"
	local stem=${PACKAGE_NAME}.${platform}

	local verspec=${SEMVER}${VARIANT}
	
	local fq_package=$stem.${verspec}.zip

	[[ ! -d $ARTDIR ]] && mkdir -p $ARTDIR

	local packfile="$ARTDIR/$fq_package"

	local xtx_vars=""
	local dep_fname=${PACKAGE_NAME}.${platform}.${verspec}.tgz

	local rampfile=ramp.yml

	python3 $READIES/bin/xtx \
		$xtx_vars \
		-e NUMVER -e SEMVER \
		$ROOT/$rampfile > /tmp/ramp.yml
	rm -f /tmp/ramp.fname $packfile
	$RAMP_CMD pack -m /tmp/ramp.yml --packname-file /tmp/ramp.fname --verbose --debug \
		-o $packfile $MODULE >/tmp/ramp.err 2>&1 || true
	if [[ ! -e $packfile ]]; then
		eprint "Error generating RAMP file:"
		>&2 cat /tmp/ramp.err
		exit 1
	fi

	cd $ARTDIR/snapshots
	if [[ ! -z $BRANCH ]]; then
		local snap_package=$stem.${BRANCH}${VARIANT}.zip
		ln -sf ../$fq_package $snap_package
	fi

	local packname=`cat /tmp/ramp.fname`
	echo "Created $packname"
	cd $ROOT
}

#----------------------------------------------------------------------------------------------

pack_deps() {
	local dep="$1"

	local platform="$OS-$OSNICK-$ARCH"
	local stem=${PACKAGE_NAME}.${dep}.${platform}
	local verspec=${SEMVER}${VARIANT}

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
	sha256sum $tar_path | awk '{print $1}' > $tar_path.sha256

	cd $ARTDIR/snapshots
	if [[ -n $BRANCH ]]; then
		local snap_dep=$stem.${BRANCH}${VARIANT}.tgz
		ln -sf ../$fq_dep $snap_dep
		ln -sf ../$fq_dep.sha256 $snap_dep.sha256
	fi
	
	cd $ROOT
}

#----------------------------------------------------------------------------------------------

prepare_symbols_dep() {
	if [[ ! -f $MODULE.debug ]]; then return 0; fi
	echo "Preparing debug symbols dependencies ..."
	echo $(cd "$(dirname $MODULE)" && pwd) > $ARTDIR/debug.dir
	echo $(basename $MODULE.debug) > $ARTDIR/debug.files
	echo "" > $ARTDIR/debug.prefix
	pack_deps debug
	echo "Done."
}

#----------------------------------------------------------------------------------------------

NUMVER=$(NUMERIC=1 $SBIN/getver)
SEMVER=$($SBIN/getver)

if [[ ! -z $VARIANT ]]; then
	VARIANT=-${VARIANT}
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
		for dep in $DEPNAMES; do
			echo "${PACKAGE_NAME}.${dep}.${OS}-${OSNICK}-${ARCH}.${SEMVER}${VARIANT}.tgz"
		done
	fi
	exit 0
fi

#----------------------------------------------------------------------------------------------

if [[ $DEPS == 1 ]]; then
	echo "Building dependencies ..."

	[[ $SYM == 1 ]] && prepare_symbols_dep

	for dep in $DEPNAMES; do
			echo "$dep ..."
			pack_deps $dep
	done
fi

if [[ $RAMP == 1 ]]; then
	if ! command -v redis-server > /dev/null; then
		eprint "$PROGNAME: Cannot find redis-server. Aborting."
		exit 1
	fi

	echo "Building RAMP $VARIANT files ..."
	pack_ramp
	echo "Done."
fi

if [[ $VERBOSE == 1 ]]; then
	echo "Artifacts:"
	du -ah --apparent-size $ARTDIR
fi

exit 0
