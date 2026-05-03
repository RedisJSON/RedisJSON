#!/usr/bin/env bash
#
# Ubuntu 18.04 (bionic) extras:
#   - Enable Ubuntu universe and use gcc-8/g++-8 from official archives (no Launchpad PPA).
#   - Build cmake 3.28 from source (bionic's apt cmake is too old).
#
# Sourced by install_script.sh; receives MODE as $1.

set -eu

MODE="${1:-}"

$MODE apt-get install -yqq --no-install-recommends \
    software-properties-common lsb-core binfmt-support cargo zlib1g-dev

$MODE add-apt-repository -y universe
$MODE apt-get update -qq
$MODE apt-get install -yqq --no-install-recommends gcc-8 g++-8
$MODE update-alternatives --install /usr/bin/gcc gcc /usr/bin/gcc-8 80 \
    --slave /usr/bin/g++ g++ /usr/bin/g++-8

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
