#!/usr/bin/env bash
# Shared package sets, by package-manager family. Single source of truth for
# everything that is the same across (most of) a family. Per-OS files in
# ../os/ compose these and add their own deltas (extra packages, repo enables,
# update-alternatives, profile snippets, ...).
#
# Sourced by os/<osnick>.sh after lib/pm.sh. All variables here are plain
# space-separated strings so callers can splat them with `apt_install $SET`.

# ----------------------------------------------------------------------------
# Debian family (apt)
# ----------------------------------------------------------------------------
DEBIAN_BASE="
    ca-certificates wget curl git make autoconf automake libtool pkg-config
    build-essential clang libclang-dev clang-format
    openssl libssl-dev libbz2-dev libffi-dev zlib1g-dev libblocksruntime-dev
    libev-dev libevent-dev
    tcl
    python3 python3-pip python3-venv python3-dev
    cmake
    unzip rsync valgrind lcov jq tar gdb
"

# ----------------------------------------------------------------------------
# RHEL family (dnf / yum). EL8/EL9 ship lcov (EL8 via EPEL, EL9 via base);
# EL10 does not. lcov is listed unconditionally and dnf_install/yum_install's
# --skip-broken silently drops it on EL10. clang-format is clang-tools-extra
# on dnf only — installed in os/<nick>.sh for dnf-based images (not Amazon
# Linux 2 yum).
# ----------------------------------------------------------------------------
RHEL_BASE="
    ca-certificates wget curl git make autoconf automake libtool
    gcc gcc-c++
    openssl openssl-devel bzip2-devel libffi-devel zlib-devel
    libev-devel libevent-devel
    clang clang-devel
    tcl
    python3 python3-pip python3-devel
    cmake
    unzip rsync valgrind lcov jq tar which gdb
"

# ----------------------------------------------------------------------------
# CBL-Mariner / Azure Linux (tdnf). Smaller repo set than dnf; readline-devel
# is here because Azure Linux's Python build setup requests it.
# ----------------------------------------------------------------------------
TDNF_BASE="
    ca-certificates wget curl git
    build-essential gcc g++ make cmake autoconf automake libtool clang
    openssl-devel bzip2-devel libffi-devel zlib-devel readline-devel
    libev-devel libevent-devel
    python3 python3-pip python3-devel
    unzip jq tar which gdb valgrind
"

# ----------------------------------------------------------------------------
# Alpine (apk). musl rustc/cargo from apk (no rustup). No valgrind package in
# the RedisJSON dependency map for apk.
# ----------------------------------------------------------------------------
ALPINE_BASE="
    ca-certificates wget curl git make autoconf automake libtool
    build-base g++ clang18 clang18-libclang cargo
    openssl openssl-dev bzip2-dev libffi-dev zlib-dev
    libev-dev libevent-dev
    tcl
    python3 python3-dev py3-pip py-virtualenv
    cmake
    unzip rsync lcov jq tar gdb
    bash bsd-compat-headers gcompat libstdc++ libgcc linux-headers musl-dev
    xz openssh
"

# ----------------------------------------------------------------------------
# macOS (homebrew). Apple clang already ships at /usr/bin/cc; we layer
# llvm@18 on top via PATH (see os/macos.sh). Python is intentionally absent
# here — most Macs already have one and `brew install python@3.11` collides
# with Apple's framework symlinks (see os/macos.sh's conditional install).
# ----------------------------------------------------------------------------
BREW_BASE="
    autoconf automake libtool
    make cmake
    llvm@18 openssl@3 libffi bzip2 zlib
    libev libevent clang-format
    coreutils wget curl rsync jq lcov
"
