#!/usr/bin/env bash
# Ubuntu 20.04 (focal). gcc-10 for C++20 features missing in default gcc-9.

. "$LIB/packages.sh"

debian_default_install
apt_install gcc-10 g++-10
# Only move the active compiler up, never down — another module's bootstrap
# may have already pinned something newer in this shared build container.
_cur=$(gcc -dumpversion 2>/dev/null | cut -d. -f1 || echo 0)
if [ "$_cur" -lt 10 ]; then
    $SUDO update-alternatives --install /usr/bin/cc  cc  /usr/bin/gcc-10 60
    $SUDO update-alternatives --set     cc  /usr/bin/gcc-10
    $SUDO update-alternatives --install /usr/bin/gcc gcc /usr/bin/gcc-10 60
    $SUDO update-alternatives --set     gcc /usr/bin/gcc-10
    $SUDO update-alternatives --install /usr/bin/g++ g++ /usr/bin/g++-10 60
    $SUDO update-alternatives --set     g++ /usr/bin/g++-10
fi
