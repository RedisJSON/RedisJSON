#!/bin/bash

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
ROOT=$(cd $HERE/.. && pwd)
export READIES=$ROOT/deps/readies
. $READIES/shibumi/defs

cd $ROOT

export PYTHONWARNINGS=ignore

#----------------------------------------------------------------------------------------------

if [[ $1 == --help || $1 == help ]]; then
	cat <<-END
		[ARGVARS...] pack.sh [--help|help]
		
		Argument variables:
		MODULE=path       Path of module .so

		RAMP=1|0          Build RAMP file
		DEPS=0|1          Build dependencies file
		SYM=0|1           Build debug symbols file

		VARIANT=name      Build variant (default: empty)
		BRANCH=name       Branch name for snapshot packages
		GITSHA=1          Append Git SHA to shapshot package names

		ARTDIR=dir        Directory in which packages are created (default: bin/artifacts)

		VERBOSE=1         Print commands
		IGNERR=1          Do not abort on error

	END
	exit 0
fi

#----------------------------------------------------------------------------------------------

[[ $IGNERR == 1 ]] || set -e
[[ $V == 1 || $VERBOSE == 1 ]] && set -x

RAMP=${RAMP:-1}
DEPS=${DEPS:-1}
SYM=${SYM:-1}

[[ -z $ARTDIR ]] && ARTDIR=bin/artifacts
mkdir -p $ARTDIR $ARTDIR/snapshots
ARTDIR=$(cd $ARTDIR && pwd)

. $READIES/bin/enable-utf8

export ARCH=$($READIES/bin/platform --arch)
export OS=$($READIES/bin/platform --os)
export OSNICK=$($READIES/bin/platform --osnick)

# RLEC naming conventions
[[ $ARCH == x64 ]] && ARCH=x86_64
[[ $OS == linux ]] && OS=Linux

[[ $OSNICK == trusty ]]  && OSNICK=ubuntu14.04
[[ $OSNICK == xenial ]]  && OSNICK=ubuntu16.04
[[ $OSNICK == bionic ]]  && OSNICK=ubuntu18.04
[[ $OSNICK == focal ]]   && OSNICK=ubuntu20.04
[[ $OSNICK == centos7 ]] && OSNICK=rhel7
[[ $OSNICK == centos8 ]] && OSNICK=rhel8
[[ $OSNICK == ol8 ]] && OSNICK=rhel8

export PRODUCT=rejson
export PRODUCT_LIB=$PRODUCT.so
export DEPNAMES=""

export PACKAGE_NAME=${PACKAGE_NAME:-${PRODUCT}}

RAMP_CMD="python3 -m RAMP.ramp"

#----------------------------------------------------------------------------------------------

pack_ramp() {
	cd $ROOT

	local platform="$OS-$OSNICK-$ARCH"
	local stem=${PACKAGE_NAME}.${platform}

	local verspec=${SEMVER}${VARIANT}
	
	local fq_package=$stem.${verspec}.zip

	local packfile="$ARTDIR/$fq_package"
	local product_so="$MODULE"

	local xtx_vars=""
	local dep_fname=${PACKAGE_NAME}.${platform}.${verspec}.tgz

	if [[ -z $VARIANT ]]; then
		local rampfile=ramp.yml
	else
		local rampfile=ramp$VARIANT.yml
	fi

	python3 $READIES/bin/xtx \
		$xtx_vars \
		-e NUMVER -e SEMVER \
		$ROOT/$rampfile > /tmp/ramp.yml
	rm -f /tmp/ramp.fname $packfile
	$RAMP_CMD pack -m /tmp/ramp.yml --packname-file /tmp/ramp.fname --verbose --debug -o $packfile $product_so >/tmp/ramp.err 2>&1 || true
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

	cd $ROOT
}

#----------------------------------------------------------------------------------------------

pack_deps() {
	local dep="$1"

	local platform="$OS-$OSNICK-$ARCH"
	local verspec=${SEMVER}${VARIANT}
	local stem=${PACKAGE_NAME}-${dep}.${platform}

	local depdir=$(cat $ARTDIR/$dep.dir)

	local fq_dep=$stem.${verspec}.tgz
	local tar_path=$ARTDIR/$fq_dep
	local dep_prefix_dir=$(cat $ARTDIR/$dep.prefix)
	
	{ cd $depdir ;\
	  cat $ARTDIR/$dep.files | \
	  xargs tar -c --sort=name --owner=root:0 --group=root:0 --mtime='UTC 1970-01-01' \
		--transform "s,^,$dep_prefix_dir," 2>> /tmp/pack.err | \
	  gzip -n - > $tar_path ; E=$?; } || true
	rm -f $ARTDIR/$dep.prefix $ARTDIR/$dep.files $ARTDIR/$dep.dir

	cd $ROOT
	if [[ $E != 0 ]]; then
		eprint "Error creating $tar_path:"
		cat /tmp/pack.err >&2
		exit 1
	fi
	sha256sum $tar_path | awk '{print $1}' > $tar_path.sha256

	cd $ARTDIR/snapshots
	if [[ ! -z $BRANCH ]]; then
		local snap_dep=$stem.${BRANCH}${VARIANT}.tgz
		ln -sf ../$fq_dep $snap_dep
		ln -sf ../$fq_dep.sha256 $snap_dep.sha256
	fi
	
	cd $ROOT
}

#----------------------------------------------------------------------------------------------

prepare_symbols_dep() {
	echo "Preparing debug symbols dependencies ..."
	echo $(cd "$(dirname $MODULE)" && pwd) > $ARTDIR/debug.dir
	echo $PRODUCT.so.debug > $ARTDIR/debug.files
	echo "" > $ARTDIR/debug.prefix
	pack_deps debug
	echo "Done."
}

#----------------------------------------------------------------------------------------------

export NUMVER=$(NUMERIC=1 $ROOT/sbin/getver)
export SEMVER=$($ROOT/sbin/getver)

if [[ ! -z $VARIANT ]]; then
	VARIANT=-${VARIANT}
fi

[[ -z $BRANCH ]] && BRANCH=${CIRCLE_BRANCH:-`git rev-parse --abbrev-ref HEAD`}
BRANCH=${BRANCH//[^A-Za-z0-9._-]/_}
if [[ $GITSHA == 1 ]]; then
	GIT_COMMIT=$(git describe --always --abbrev=7 --dirty="+" 2>/dev/null || git rev-parse --short HEAD)
	BRANCH="${BRANCH}-${GIT_COMMIT}"
fi
export BRANCH

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
		eprint "$0: Cannot find redis-server. Aborting."
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
