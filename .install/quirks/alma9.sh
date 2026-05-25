#!/usr/bin/env bash
#
# AlmaLinux 9 extras: pulls gcc-13 from gcc-toolset to match Dockerfile.alma9.
# After this quirk runs, the Dockerfile is responsible for exposing the
# toolset on PATH/LD_LIBRARY_PATH.
#
# Sourced by install_script.sh; receives MODE as $1.

set -eu

MODE="${1:-}"

$MODE dnf -y install \
    gcc-toolset-13-gcc \
    gcc-toolset-13-gcc-c++ \
    gcc-toolset-13-libatomic-devel

$MODE cp /opt/rh/gcc-toolset-13/enable /etc/profile.d/gcc-toolset-13.sh
