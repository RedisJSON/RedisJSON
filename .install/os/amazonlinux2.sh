#!/usr/bin/env bash
# Amazon Linux 2 — yum, devtoolset-11, cmake3 symlink (see timeseries).

. "$LIB/packages.sh"

$SUDO amazon-linux-extras install epel -y
$SUDO yum -y install epel-release yum-utils
$SUDO yum-config-manager --add-repo http://vault.centos.org/centos/7/sclo/x86_64/rh/

yum_install autogen centos-release-scl scl-utils cmake3 awscli
$SUDO yum -y install --nogpgcheck --skip-broken \
    devtoolset-11-gcc devtoolset-11-gcc-c++ devtoolset-11-make || true

rhel_default_install

# Amazon Linux 2's base `openssl` is 1.0.2 — too old for Redis/Rust. Layer
# openssl 1.1 from amzn2 and symlink /usr/bin/openssl so the build picks it up.
# `openssl11-devel` ships /usr/include/openssl/* headers and conflicts with the
# `openssl-devel` 1.0.2 just pulled in by rhel_default_install — yum refuses
# the install otherwise. Remove the old -devel first; the runtime `openssl11`
# package itself doesn't conflict with `openssl`.
$SUDO yum -y remove openssl-devel || true
$SUDO yum -y install openssl11 openssl11-devel
if command -v openssl11 >/dev/null 2>&1; then
    $SUDO ln -sf "$(command -v openssl11)" /usr/bin/openssl
fi

if command -v cmake3 >/dev/null 2>&1; then
    $SUDO ln -sf "$(command -v cmake3)" /usr/bin/cmake
fi
