#!/usr/bin/env bash
#
# Ubuntu 20.04 (focal) extras: pin gcc/g++ to the gcc-10 toolchain via
# update-alternatives, matching the legacy .install/ubuntu_20.04.sh behaviour.
# Run after the standard apt install of the abstract list.
#
# Sourced by install_script.sh; receives MODE as $1.

set -eu

MODE="${1:-}"

$MODE apt-get install -yqq --no-install-recommends gcc-10 g++-10 lcov
$MODE update-alternatives --install /usr/bin/gcc gcc /usr/bin/gcc-10 60 \
    --slave /usr/bin/g++ g++ /usr/bin/g++-10
