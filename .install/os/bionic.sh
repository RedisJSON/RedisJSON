#!/usr/bin/env bash
# Ubuntu 18.04 (bionic). cmake 3.28 built from source — apt cmake is 3.10.
# gcc-10 via toolchain-r PPA; || true so launchpad failures are non-fatal
# (ESM repos already carry gcc-10 on Ubuntu Pro machines).
# No distro cargo — Rust comes from getrust.sh after setup-python.

. "$LIB/packages.sh"

apt_install software-properties-common lsb-core binfmt-support zlib1g-dev
echo "deb http://ppa.launchpad.net/ubuntu-toolchain-r/test/ubuntu bionic main" | $SUDO tee /etc/apt/sources.list.d/ubuntu-toolchain-r-test.list
$SUDO apt-key adv --keyserver keyserver.ubuntu.com --recv-keys 1E9377A2BA9EF27F || true
$SUDO apt-get update -qq
debian_default_install
apt_install gcc-10 g++-10 awscli
$SUDO update-alternatives --install /usr/bin/gcc gcc /usr/bin/gcc-10 60 \
    --slave /usr/bin/g++ g++ /usr/bin/g++-10

if ! cmake --version 2>/dev/null | grep -qE 'cmake version 3\.(2[89]|[3-9][0-9])'; then
    cd /tmp
    wget -q https://cmake.org/files/v3.28/cmake-3.28.0.tar.gz
    tar -xzf cmake-3.28.0.tar.gz
    cd cmake-3.28.0
    ./configure
    make -j"$(nproc)"
    $SUDO make install
    cd /
    rm -rf /tmp/cmake-3.28.0 /tmp/cmake-3.28.0.tar.gz
    $SUDO ln -sf /usr/local/bin/cmake /usr/bin/cmake
fi

pip3 install dataclasses
