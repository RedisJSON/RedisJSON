#!/usr/bin/env bash
#
# AlmaLinux 8 extras: the abstract list installs a base gcc, but our build
# really wants gcc-11 from gcc-toolset. Mirrors Dockerfile.alma8's setup.
# After this quirk runs, the Dockerfile is responsible for exposing the
# toolset on PATH/LD_LIBRARY_PATH (see Dockerfile.alma8 for the env lines).
#
# Sourced by install_script.sh; receives MODE as $1.

set -eu

MODE="${1:-}"

$MODE dnf -y groupinstall "Development Tools"
$MODE dnf config-manager --set-enabled powertools
$MODE dnf -y install epel-release
$MODE dnf -y install \
    bzip2-devel \
    gcc-toolset-11-gcc \
    gcc-toolset-11-gcc-c++ \
    gcc-toolset-11-libatomic-devel \
    libffi-devel \
    python3.11-devel \
    xz \
    zlib-devel

# Drop a profile snippet so subsequent shells (including Dockerfile RUNs that
# don't `source enable` themselves) get the toolset on PATH.
$MODE cp /opt/rh/gcc-toolset-11/enable /etc/profile.d/gcc-toolset-11.sh
