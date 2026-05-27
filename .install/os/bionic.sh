#!/usr/bin/env bash
# Ubuntu 18.04 (bionic). Toolchain PPA + gcc-10; cmake 3.28 from source.
# No distro cargo — Rust comes from getrust.sh after setup-python.

. "$LIB/packages.sh"

apt_install software-properties-common lsb-core binfmt-support zlib1g-dev
$SUDO add-apt-repository ppa:ubuntu-toolchain-r/test -y
debian_default_install
apt_install gcc-10 g++-10 awscli
$SUDO update-alternatives --install /usr/bin/gcc gcc /usr/bin/gcc-10 60 \
    --slave /usr/bin/g++ g++ /usr/bin/g++-10

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
