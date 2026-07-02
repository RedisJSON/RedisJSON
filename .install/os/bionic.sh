#!/usr/bin/env bash
# Ubuntu 18.04 (bionic). cmake 3.28 built from source — apt cmake is 3.10.
# gcc-10 via toolchain-r PPA; || true so launchpad failures are non-fatal
# (ESM repos already carry gcc-10 on Ubuntu Pro machines).
# No distro cargo — Rust comes from getrust.sh after setup-python.

. "$LIB/packages.sh"

export DEBIAN_FRONTEND=noninteractive
echo 'debconf debconf/frontend select Noninteractive' | $SUDO debconf-set-selections 2>/dev/null || true
echo 'tzdata tzdata/Areas select Etc' | $SUDO debconf-set-selections 2>/dev/null || true
echo 'tzdata tzdata/Zones/Etc select UTC' | $SUDO debconf-set-selections 2>/dev/null || true
$SUDO apt-get update -qq
$SUDO apt-get install -y --no-install-recommends gnupg wget curl ca-certificates
wget -qO- "https://keyserver.ubuntu.com/pks/lookup?op=get&search=0x1E9377A2BA9EF27F" | $SUDO gpg --batch --no-tty --yes --dearmor -o /etc/apt/trusted.gpg.d/ubuntu-toolchain-r.gpg || true
wget -qO- "https://keyserver.ubuntu.com/pks/lookup?op=get&search=0x2C277A0A352154E5" | $SUDO gpg --batch --no-tty --yes --dearmor -o /etc/apt/trusted.gpg.d/ubuntu-toolchain-r-2.gpg || true
echo "deb http://ppa.launchpad.net/ubuntu-toolchain-r/test/ubuntu bionic main" | $SUDO tee /etc/apt/sources.list.d/ubuntu-toolchain-r-test.list
apt_install software-properties-common lsb-core binfmt-support zlib1g-dev
$SUDO apt-get update -qq
debian_default_install
apt_install gcc-10 g++-10
install_aws_cli
# Only move the active compiler up, never down — another module's bootstrap
# may have already pinned something newer in this shared build container.
_cur=$(gcc -dumpversion | cut -d. -f1)
if [ "$_cur" -lt 10 ]; then
    # g++ is registered as its own independent master by debian_default_install,
    # not as a slave of gcc — --slave-grouping it here would conflict with that.
    $SUDO update-alternatives --install /usr/bin/gcc gcc /usr/bin/gcc-10 60
    $SUDO update-alternatives --set     gcc /usr/bin/gcc-10
    $SUDO update-alternatives --install /usr/bin/g++ g++ /usr/bin/g++-10 60
    $SUDO update-alternatives --set     g++ /usr/bin/g++-10
fi

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
