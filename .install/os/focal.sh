#!/usr/bin/env bash
# Ubuntu 20.04 (focal). gcc-10 for C++20 features missing in default gcc-9.

. "$LIB/packages.sh"

debian_default_install
apt_install gcc-10 g++-10
$SUDO update-alternatives --install /usr/bin/gcc gcc /usr/bin/gcc-10 60 \
    --slave /usr/bin/g++ g++ /usr/bin/g++-10
