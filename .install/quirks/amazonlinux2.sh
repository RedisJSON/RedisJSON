#!/usr/bin/env bash
#
# Amazon Linux 2 extras: AL2 ships ancient gcc/cmake. The standard abstract
# install only does the easy packages; this quirk:
#   * Enables EPEL and the (deprecated, served from CentOS Vault) SCL repo
#   * Installs devtoolset-11 (gcc-11/g++-11) so we have a modern toolchain
#   * Installs cmake3 and symlinks it as /usr/bin/cmake
#
# This mirrors the legacy .install/amazon_linux_2.sh / Dockerfile.amazonlinux2
# behaviour. It does NOT (re)build Python from source: callers that need a
# newer Python should layer that step in their Dockerfile.
#
# Sourced by install_script.sh; receives MODE as $1.

set -eu

MODE="${1:-}"

$MODE amazon-linux-extras install epel -y
$MODE yum -y install epel-release yum-utils
$MODE yum-config-manager --add-repo http://vault.centos.org/centos/7/sclo/x86_64/rh/

$MODE yum -y install \
    autogen \
    centos-release-scl \
    cmake3 \
    scl-utils

# devtoolset-11 lives in the CentOS Vault SCL repo and is x86_64-only there;
# aarch64 hosts (e.g. Apple Silicon dev laptops) will see "No package
# devtoolset-11-* available" and that's expected. --skip-broken makes that a
# warning instead of a hard build failure so dependencies.yaml-driven builds
# can at least exercise the abstract path on aarch64 even if the produced
# image isn't usable for actual compilation.
$MODE yum -y install --nogpgcheck --skip-broken \
    devtoolset-11-gcc \
    devtoolset-11-gcc-c++ \
    devtoolset-11-make || true

# Force `cmake` -> cmake3. AL2's base repo also ships an ancient `cmake`
# (2.8.12) which `yum install cmake` (from the abstract path) pulls in;
# leaving that on PATH breaks anything needing cmake>=3 (e.g. cpu_features).
# This symlink is unconditional on purpose to override the 2.8 binary —
# the legacy Dockerfile.amazonlinux2 did exactly the same thing.
$MODE ln -sf "$(command -v cmake3)" /usr/bin/cmake
