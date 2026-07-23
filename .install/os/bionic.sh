#!/usr/bin/env bash
# Ubuntu 18.04 (bionic). cmake 3.28 built from source — apt cmake is 3.10.
# gcc-10 via toolchain-r PPA. No distro cargo — Rust comes from getrust.sh
# after setup-python. Every step is guarded on what's already present, so
# `make bootstrap` is idempotent (a second run is a near-no-op) and `dry-run`
# prints only the commands still needed — never a step whose target exists.

# shellcheck source=../lib/packages.sh
. "$LIB/packages.sh"

export DEBIAN_FRONTEND=noninteractive
# debconf pre-seed (suppresses tzdata prompts). Idempotent apt prep, not a dep —
# apt_install also carries DEBIAN_FRONTEND=noninteractive, so keep it silent.
echo 'debconf debconf/frontend select Noninteractive' | $SUDO debconf-set-selections 2>/dev/null || true
echo 'tzdata tzdata/Areas select Etc' | $SUDO debconf-set-selections 2>/dev/null || true
echo 'tzdata tzdata/Zones/Etc select UTC' | $SUDO debconf-set-selections 2>/dev/null || true
apt_install gnupg wget curl ca-certificates
# gcc-10 comes from the ubuntu-toolchain-r PPA. Only add the PPA when gcc-10
# isn't installed yet — so a re-run / dry-run on a provisioned host skips it.
if ! dpkg-query -W -f='${Status}' gcc-10 2>/dev/null | grep -q 'ok installed'; then
    _sh 'wget -qO- "https://keyserver.ubuntu.com/pks/lookup?op=get&search=0x1E9377A2BA9EF27F" | sudo gpg --batch --no-tty --yes --dearmor -o /etc/apt/trusted.gpg.d/ubuntu-toolchain-r.gpg || true'
    _sh 'wget -qO- "https://keyserver.ubuntu.com/pks/lookup?op=get&search=0x2C277A0A352154E5" | sudo gpg --batch --no-tty --yes --dearmor -o /etc/apt/trusted.gpg.d/ubuntu-toolchain-r-2.gpg || true'
    _sh 'echo "deb http://ppa.launchpad.net/ubuntu-toolchain-r/test/ubuntu bionic main" | sudo tee /etc/apt/sources.list.d/ubuntu-toolchain-r-test.list'
    _run apt-get update -qq
fi
apt_install software-properties-common lsb-core binfmt-support zlib1g-dev
debian_default_install
apt_install gcc-10 g++-10
install_aws_cli
# Only move the active compiler up, never down — another module's bootstrap
# may have already pinned something newer in this shared build container.
_cur=$(gcc -dumpversion 2>/dev/null | cut -d. -f1 || echo 0)
if [ "$_cur" -lt 10 ]; then
    # cc/gcc/g++ are each registered as their own independent master by
    # debian_default_install, not slaves of each other — --slave-grouping
    # would conflict with that.
    _run update-alternatives --install /usr/bin/cc  cc  /usr/bin/gcc-10 60
    _run update-alternatives --set     cc  /usr/bin/gcc-10
    _run update-alternatives --install /usr/bin/gcc gcc /usr/bin/gcc-10 60
    _run update-alternatives --set     gcc /usr/bin/gcc-10
    _run update-alternatives --install /usr/bin/g++ g++ /usr/bin/g++-10 60
    _run update-alternatives --set     g++ /usr/bin/g++-10
fi

# cmake 3.28 from source (apt ships 3.10) — only when the installed cmake is
# older. Each step prints in dry-run; skipped entirely once cmake is new enough.
if ! cmake --version 2>/dev/null | grep -qE 'cmake version 3\.(2[89]|[3-9][0-9])'; then
    _sh 'cd /tmp && wget -q https://cmake.org/files/v3.28/cmake-3.28.0.tar.gz && tar -xzf cmake-3.28.0.tar.gz'
    _sh 'cd /tmp/cmake-3.28.0 && ./configure && make -j"$(nproc)" && sudo make install'
    _sh 'cd / && rm -rf /tmp/cmake-3.28.0 /tmp/cmake-3.28.0.tar.gz && sudo ln -sf /usr/local/bin/cmake /usr/bin/cmake'
fi
# dataclasses backport (Python 3.6 lacks it) — skip if already importable.
if ! python3 -c 'import dataclasses' >/dev/null 2>&1; then
    _sh 'pip3 install dataclasses'
fi
