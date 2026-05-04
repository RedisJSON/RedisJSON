#!/usr/bin/env bash
#
# Ubuntu 18.04 (bionic) extras: pulls a newer gcc-10 from the toolchain test
# PPA (bionic only ships gcc-7) and builds cmake 3.28 from source because
# bionic's apt cmake is too old for our CMakeLists. Mirrors the legacy
# Dockerfile.bionic logic.
#
# Sourced by install_script.sh; receives MODE as $1.

set -eu

MODE="${1:-}"

$MODE apt-get install -yqq --no-install-recommends \
    software-properties-common lsb-core binfmt-support cargo zlib1g-dev
$MODE add-apt-repository ppa:ubuntu-toolchain-r/test -y
$MODE apt-get update -qq
$MODE apt-get install -yqq --no-install-recommends gcc-10 g++-10
$MODE update-alternatives --install /usr/bin/gcc gcc /usr/bin/gcc-10 60 \
    --slave /usr/bin/g++ g++ /usr/bin/g++-10

# bionic's cmake is too old; build the version pack.sh expects.
cd /tmp
wget -q https://cmake.org/files/v3.28/cmake-3.28.0.tar.gz
tar -xzf cmake-3.28.0.tar.gz
cd cmake-3.28.0
./configure
make -j"$(nproc)"
$MODE make install
cd /
rm -rf /tmp/cmake-3.28.0 /tmp/cmake-3.28.0.tar.gz
$MODE ln -sf /usr/local/bin/cmake /usr/bin/cmake
