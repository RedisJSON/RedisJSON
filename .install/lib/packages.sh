#!/usr/bin/env bash
# Shared package sets, by package-manager family. Single source of truth for
# everything that is the same across (most of) a family. Per-OS files in
# ../os/ compose these and add their own deltas (extra packages, repo enables,
# update-alternatives, profile snippets, ...).
#
# Sourced by os/<osnick>.sh after lib/pm.sh. All variables here are plain
# space-separated strings so callers can splat them with `apt_install $SET`.

# Optional = installed by bootstrap but NOT in the README's minimal build-dep
# list: tests/coverage/debug and feature libs the core build/run doesn't need.
# Only affects `make bootstrap list` (reported separately, never fails).
# Shared superset across modules; names this module doesn't install are simply
# never matched.
OPTIONAL_PKGS="valgrind gdb lcov tcl jq clang-format py3-numpy python3-numpy py3-psutil python3-psutil py3-cryptography python3-cryptography openssh xsimd openblas-dev curl tar uv zlib1g-dev zlib-dev zlib-devel libbz2-dev bzip2-dev bzip2-devel libblocksruntime-dev libev-dev libev-devel libevent-dev libevent-devel awscli"

# MIN_VERSIONS: sparse "pkg:minversion" list — only deps with a real floor a
# package-manager install can actually satisfy. Everything else is presence-only.
# (cmake is intentionally NOT here: its >= 3.25 requirement is provisioned by
# install_cmake.sh during a real `make bootstrap`, not by the distro package, so
# a version floor here would be unsatisfiable by the apt/dnf `cmake` and would
# only make `list`/`dry-run` disagree.)
MIN_VERSIONS=""

# Install AWS CLI v2 from the official installer (arch-aware). Skips if
# already present — handles pre-installed AMIs without failing.
install_aws_cli() {
    # aws-cli is an optional CI artifact-upload tool, not a build/run dep.
    # list/dry-run must not mutate: the curl+unzip below hit the network and
    # write /tmp, so short-circuit before them. list records it as optional.
    if [ "${CHECK_DEPS:-0}" = 1 ] || [ "${DRY_RUN:-0}" = 1 ]; then
        if command -v aws >/dev/null 2>&1; then
            DEPS_OPT_OK="$DEPS_OPT_OK awscli"
        else
            DEPS_OPT_MISSING="$DEPS_OPT_MISSING awscli"
            [ "${DRY_RUN:-0}" = 1 ] && _dry_line 'curl -fSL --retry 3 "https://awscli.amazonaws.com/awscli-exe-linux-$(uname -m).zip" -o /tmp/awscliv2.zip && unzip -o /tmp/awscliv2.zip -d /tmp/awscli-install && sudo /tmp/awscli-install/aws/install && rm -rf /tmp/awscliv2.zip /tmp/awscli-install'
        fi
        return 0
    fi
    if command -v aws &>/dev/null; then
        return 0
    fi
    local arch
    arch=$(uname -m)
    local url="https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip"
    [ "$arch" = "aarch64" ] && url="https://awscli.amazonaws.com/awscli-exe-linux-aarch64.zip"
    curl -fSL --retry 3 "$url" -o /tmp/awscliv2.zip
    unzip -o /tmp/awscliv2.zip -d /tmp/awscli-install
    $SUDO /tmp/awscli-install/aws/install
    rm -rf /tmp/awscliv2.zip /tmp/awscli-install
}

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
