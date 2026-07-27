#!/usr/bin/env bash
# Package-manager helpers and family-default install routines.
#
# Sourced by install_script.sh (and indirectly by os/<osnick>.sh). Reads
# $MODE (set by install_script.sh from $1; "sudo" or empty) and exports:
#   PM      -> brew | apt | dnf | yum | tdnf | apk
#   SUDO    -> "sudo" or "" (forced empty on macOS; brew refuses sudo)
#
# Helpers (each accepts a list of packages; no-op if none):
#   apt_install / dnf_install / yum_install / tdnf_install / apk_install
#   brew_install
#
# Family defaults (compose lib/packages.sh + groupinstall + repo enables):
#   debian_default_install
#   rhel_default_install   <- "Development Tools" + RHEL_BASE
#   tdnf_default_install
#   alpine_default_install
#   brew_default_install
#   el8_default_install    <- rhel_default_install + powertools/crb +
#                             gcc-toolset-11 (alma8 / rocky8)
#   el9_default_install    <- rhel_default_install + gcc-toolset-13
#                             (alma9 / rocky9)

if [ "$(uname -s)" = "Darwin" ]; then
    PM="brew"
    SUDO=""
elif command -v apt-get >/dev/null 2>&1; then PM="apt";  SUDO="${MODE:-}"
elif command -v dnf     >/dev/null 2>&1; then PM="dnf";  SUDO="${MODE:-}"
elif command -v tdnf    >/dev/null 2>&1; then PM="tdnf"; SUDO="${MODE:-}"
elif command -v yum     >/dev/null 2>&1; then PM="yum";  SUDO="${MODE:-}"
elif command -v apk     >/dev/null 2>&1; then PM="apk";  SUDO="${MODE:-}"
else
    echo "pm.sh: no supported package manager (apt/dnf/tdnf/yum/apk/brew)" >&2
    exit 1
fi

# ----------------------------------------------------------------------------
# list mode: when CHECK_DEPS=1 the install primitives below do NOT
# install — they query whether each package is already present and record it
# into DEPS_OK / DEPS_MISSING (printed as a summary by install_script.sh).
# SUDO is neutralised to a no-op so stray privileged side-commands
# (groupinstall, repo enables, ln/cp/update-alternatives) can't mutate the
# system during a check.
# ----------------------------------------------------------------------------
CHECK_DEPS="${CHECK_DEPS:-0}"
DEPS_OK=""
DEPS_MISSING=""
DEPS_OPT_OK=""
DEPS_OPT_MISSING=""

# Optional deps are marked in lib/packages.sh (OPTIONAL_PKGS); default empty
# here so a check works even before packages.sh is sourced. Still installed
# normally — this only splits them into a separate list bucket.
OPTIONAL_PKGS="${OPTIONAL_PKGS:-}"
_is_optional() { case " $OPTIONAL_PKGS " in *" $1 "*) return 0 ;; *) return 1 ;; esac; }

# MIN_VERSIONS (lib/packages.sh): sparse "pkg:minversion" list -- only deps
# with a real floor. Everything else is presence-only. Default empty.
MIN_VERSIONS="${MIN_VERSIONS:-}"
_min_for() { for _e in $MIN_VERSIONS; do case "$_e" in "$1:"*) echo "${_e#*:}"; return ;; esac; done; }

# Read a tool's installed version from the tool itself (not the package DB --
# e.g. cmake is overlaid into /usr/local by install_cmake.sh, so dpkg would
# report the stale apt version).
_get_installed_version() {
    case "$1" in
        gcc|g++) "$1" -dumpversion 2>/dev/null ;;
        *)       "$1" --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+(\.[0-9]+)?' | head -1 || true ;;
    esac
}

# version_ge HAVE WANT -> 0 if HAVE >= WANT (strips -rev/+build suffixes).
version_ge() {
    _have="${1%%[-+]*}"; _want="${2%%[-+]*}"
    _s=sort; sort -V </dev/null >/dev/null 2>&1 || { command -v gsort >/dev/null 2>&1 && _s=gsort; }
    [ "$(printf '%s\n%s\n' "$_want" "$_have" | "$_s" -V | head -1)" = "$_want" ]
}

# DRY_RUN=1: run the bootstrap flow but install nothing — for each MISSING
# package, print the exact install command that WOULD run (the same single
# line the primitive executes normally, via _run — no duplicated command).
DRY_RUN="${DRY_RUN:-0}"

if [ "$CHECK_DEPS" = 1 ] || [ "$DRY_RUN" = 1 ]; then
    _SUDO_DISPLAY="$SUDO"   # remember the real sudo prefix for dry-run printing
    SUDO=":"                # neutralize privileged side-commands (no mutation)
fi

# dry-run output is blue on a real terminal, plain when piped (CI logs).
if [ "$DRY_RUN" = 1 ] && [ -t 1 ]; then
    _DRY_C="$(printf '\033[0;34m')"; _DRY_H="$(printf '\033[1;36m')"; _DRY_R="$(printf '\033[0m')"
else _DRY_C=""; _DRY_H=""; _DRY_R=""; fi
_dry_line() { printf '%s%s%s\n' "$_DRY_C" "$*" "$_DRY_R"; }   # a command  (blue)
_dry_head() { printf '%s%s%s\n' "$_DRY_H" "$*" "$_DRY_R"; }   # a headline (cyan)

# _run CMD... — one wrapper for every "would-install" command, so callers
# never branch on the mode:
#   install  -> execute it (with the real sudo prefix)
#   dry-run  -> print it (blue), don't execute
#   list     -> skip it (a check neither installs nor prints)
_apt_df_shown=0
_run() {
    if [ "$CHECK_DEPS" = 1 ]; then return 0
    elif [ "$DRY_RUN" = 1 ]; then
        case " $* " in
            *' apt-get '*|*' dnf '*|*' yum '*|*' tdnf '*|*' apk '*)
                # First pkg-manager command: on apt, emit the noninteractive
                # frontend right before it — only when there IS an apt command,
                # so a box needing no apt install prints no export noise.
                if [ "$PM" = apt ] && [ "$_apt_df_shown" = 0 ]; then
                    _dry_line 'export DEBIAN_FRONTEND=noninteractive'; _apt_df_shown=1
                fi
                # apt-get/dnf/... read stdin; in a pasted dry-run that stdin IS
                # the following commands, so the pkg manager swallows the rest of
                # the paste. Redirect from /dev/null so a paste runs to the end.
                _dry_line "${_SUDO_DISPLAY:+$_SUDO_DISPLAY }$* < /dev/null" ;;
            *)  _dry_line "${_SUDO_DISPLAY:+$_SUDO_DISPLAY }$*" ;;
        esac
    else ${_SUDO_DISPLAY:-$SUDO} "$@"; fi
}

# _sh 'RAW SHELL' — a mutating step that doesn't fit _run (pipes, redirects,
# cd/&&-chains, source builds). dry-run: print it verbatim so the output stays a
# copy-pasteable script; list: skip; else: execute via eval. Write the string
# with a literal `sudo` (these raw steps are Linux-only, where bootstrap ensures
# sudo) so the printed line runs as-is.
_sh() {
    if [ "$DRY_RUN" = 1 ]; then _dry_line "$1"
    elif [ "$CHECK_DEPS" = 1 ]; then return 0
    else eval "$1"; fi
}

# Echo (space-separated) only the packages from "$@" that are NOT installed.
_missing_only() { for _p in "$@"; do _pkg_installed "$_p" || printf '%s ' "$_p"; done; }

# Read-only "is this package installed?" probe, per package manager.
_pkg_installed() {
    case "$PM" in
        apt)          dpkg-query -W -f='${Status}' "$1" 2>/dev/null | grep -q 'ok installed' ;;
        dnf|yum|tdnf) rpm -q "$1" >/dev/null 2>&1 ;;
        apk)          apk info -e "$1" >/dev/null 2>&1 ;;
        # Judge by stdout, not exit code: brew often prints unrelated
        # warnings to stderr and returns non-zero even when the formula is
        # installed. A non-empty "<name> <version>" line means installed.
        brew)         [ -n "$(brew list --versions "$1" 2>/dev/null)" ] ;;
        *)            return 1 ;;
    esac
}

_check_pkgs() {
    for _p in "$@"; do
        if _is_optional "$_p"; then
            if _pkg_installed "$_p"; then DEPS_OPT_OK="$DEPS_OPT_OK $_p"; else DEPS_OPT_MISSING="$DEPS_OPT_MISSING $_p"; fi
        else
            _min=$(_min_for "$_p")
            if ! _pkg_installed "$_p"; then
                if [ -n "$_min" ]; then DEPS_MISSING="$DEPS_MISSING $_p:$_min"; else DEPS_MISSING="$DEPS_MISSING $_p"; fi
            elif [ -n "$_min" ]; then
                # has a floor: treat an unknown/unparseable version as NOT
                # satisfied (fail-safe) rather than silently recording it OK.
                _have=$(_get_installed_version "$_p")
                if [ -z "$_have" ] || ! version_ge "$_have" "$_min"; then
                    DEPS_MISSING="$DEPS_MISSING $_p:$_min"
                else
                    DEPS_OK="$DEPS_OK $_p"
                fi
            else
                DEPS_OK="$DEPS_OK $_p"
            fi
        fi
    done
}

# ----------------------------------------------------------------------------
# Per-PM install primitives
# ----------------------------------------------------------------------------

# apt-get update is expensive on slow mirrors; only run it once per script
# invocation, on the first apt_install.
_pm_apt_updated=0
apt_install() {
    [ "$#" -gt 0 ] || return 0
    if [ "$CHECK_DEPS" = 1 ]; then _check_pkgs "$@"; return 0; fi
    if [ "$DRY_RUN" = 1 ]; then set -- $(_missing_only "$@"); [ "$#" -gt 0 ] || return 0; fi
    # Acquire::Retries: ports.ubuntu.com (arm64 mirror) intermittently drops
    # connections mid-build; without retries a single dropped fetch fails the
    # whole docker build (exit 100). Retry each download before giving up.
    local apt_retry="-o Acquire::Retries=5"
    if [ "$_pm_apt_updated" = 0 ]; then
        export DEBIAN_FRONTEND=noninteractive
        _run apt-get update -qq $apt_retry
        _pm_apt_updated=1
    fi
    # env goes THROUGH sudo: sudo's env_reset strips exported variables, so a
    # plain export upstream never reaches dpkg — debconf (e.g. tzdata on focal,
    # which the base image doesn't preinstall) then blocks on an interactive
    # prompt and the bootstrap hangs.
    _run env DEBIAN_FRONTEND=noninteractive apt-get install -yqq --no-install-recommends $apt_retry "$@"
}

# `--allowerasing` lets dnf pick our `curl` over the slimmer `curl-minimal`
# preinstalled on amazon linux 2023 / EL10 base images.
dnf_install() {
    [ "$#" -gt 0 ] || return 0
    if [ "$CHECK_DEPS" = 1 ]; then _check_pkgs "$@"; return 0; fi
    if [ "$DRY_RUN" = 1 ]; then set -- $(_missing_only "$@"); [ "$#" -gt 0 ] || return 0; fi
    _run dnf -y install --allowerasing --skip-broken "$@"
}

yum_install() {
    [ "$#" -gt 0 ] || return 0
    if [ "$CHECK_DEPS" = 1 ]; then _check_pkgs "$@"; return 0; fi
    if [ "$DRY_RUN" = 1 ]; then set -- $(_missing_only "$@"); [ "$#" -gt 0 ] || return 0; fi
    _run yum -y install --skip-broken "$@"
}

# tdnf has no --skip-broken and aborts the whole transaction if any single
# package is missing from the repos (CBL-Mariner / AzureLinux ship a much
# smaller subset than dnf). Install one at a time so a missing package
# becomes a warning rather than a build failure.
tdnf_install() {
    [ "$#" -gt 0 ] || return 0
    if [ "$CHECK_DEPS" = 1 ]; then _check_pkgs "$@"; return 0; fi
    if [ "$DRY_RUN" = 1 ]; then set -- $(_missing_only "$@"); fi
    local pkg out
    for pkg in "$@"; do
        if [ "$DRY_RUN" = 1 ]; then _run tdnf -y install "$pkg"; continue; fi
        # Capture combined output so a real failure (network/GPG/conflict) is
        # distinguishable from the "package not in repo" common case. We still
        # tolerate the miss, but surface the last line of tdnf's diagnostic so
        # operator doesn't have to re-run by hand to see what went wrong.
        if ! out=$($SUDO tdnf -y install "$pkg" 2>&1); then
            echo "pm.sh: tdnf skipped unavailable package '$pkg': $(printf '%s\n' "$out" | tail -n1)"
        fi
    done
}

apk_install() {
    [ "$#" -gt 0 ] || return 0
    if [ "$CHECK_DEPS" = 1 ]; then _check_pkgs "$@"; return 0; fi
    if [ "$DRY_RUN" = 1 ]; then set -- $(_missing_only "$@"); [ "$#" -gt 0 ] || return 0; fi
    _run apk add --no-cache "$@"
}

# brew exits non-zero when a formula is already installed/linked. We tolerate
# that on rerun; real failures get caught later by the caller's feature
# checks (compiler probes, `command -v`, ...).
brew_install() {
    [ "$#" -gt 0 ] || return 0
    if [ "$CHECK_DEPS" = 1 ]; then _check_pkgs "$@"; return 0; fi
    if [ "$DRY_RUN" = 1 ]; then set -- $(_missing_only "$@"); [ "$#" -gt 0 ] || return 0; _run brew install "$@"; return 0; fi
    if ! command -v brew >/dev/null 2>&1; then
        echo "pm.sh: brew not installed; install from https://brew.sh" >&2
        exit 1
    fi
    HOMEBREW_NO_AUTO_UPDATE=1 brew install "$@" || true
}

# ----------------------------------------------------------------------------
# Family-default installers — composes lib/packages.sh + family-wide quirks
# (groupinstall, repo enables, EL8/EL9 toolsets). Per-OS files just call
# the matching one and add their own delta on top.
# ----------------------------------------------------------------------------

debian_default_install() {
    apt_install $DEBIAN_BASE
}

rhel_default_install() {
    # "Development Tools" is a large meta-group; skip it once the core compiler
    # trio is present so re-runs / dry-run don't keep re-listing it.
    if ! rpm -q gcc gcc-c++ make >/dev/null 2>&1; then
        case "$PM" in
            dnf) _sh 'sudo dnf -y groupinstall "Development Tools" || true' ;;
            yum) _sh 'sudo yum -y groupinstall "Development Tools" || true' ;;
        esac
    fi
    case "$PM" in
        dnf) dnf_install $RHEL_BASE ;;
        yum) yum_install $RHEL_BASE ;;
    esac
}

tdnf_default_install() {
    tdnf_install $TDNF_BASE
}

alpine_default_install() {
    apk_install $ALPINE_BASE
}

brew_default_install() {
    brew_install $BREW_BASE
}

# Enable EL8 secondary repo so packages like libatomic-devel resolve.
# Names: powertools (Alma/Rocky/CentOS), crb (newer rebuilds), and
# codeready-builder-for-rhel-8-* (real RHEL 8 via RHUI/subscription).
_pm_enable_el8_extras() {
    if [ -r /etc/os-release ]; then
        # shellcheck disable=SC1091
        . /etc/os-release
        if [ "${ID:-}" = "rhel" ] && [ "${VERSION_ID%%.*}" = "8" ]; then
            # `local rid=$(...)` (single line) is intentional: the `local`
            # builtin returns its own exit status (always 0), which masks
            # the pipeline's pipefail-propagated grep exit when no repo
            # matches. Splitting into `local rid; rid=$(...)` makes the
            # assignment a simple command and set -e aborts install_script.sh
            # before the diagnostic below can run.
            local rid=$(dnf repolist --all 2>/dev/null \
                | grep -i 'codeready-builder-for-rhel-8' \
                | grep -vi source \
                | head -1 \
                | awk '{print $1}')
            if [ -n "$rid" ]; then
                dnf repolist --enabled 2>/dev/null | grep -q "$rid" \
                    || _run dnf config-manager --set-enabled "$rid"
                return 0
            fi
            echo "pm.sh: RHEL 8 needs CodeReady Builder; no codeready-builder-for-rhel-8 repo found." >&2
            echo "  Fix RHUI/subscription repos, then re-run bootstrap." >&2
            return 1
        fi
    fi
    # Skip if a secondary repo (powertools/crb/codeready) is already enabled.
    dnf repolist --enabled 2>/dev/null | grep -qiE 'powertools|crb|codeready' \
        || _sh 'sudo dnf config-manager --set-enabled powertools 2>/dev/null || sudo dnf config-manager --set-enabled crb 2>/dev/null || true'
}

# EL8 (Alma 8, Rocky 8, RHEL 8): base-RHEL gcc is 8.5; we layer gcc-toolset-11
# and drop a profile.d snippet so subsequent shells (including Dockerfile RUNs
# that don't `source enable`) have it on PATH.
#
# Pins SETUP_PYTHON_VERSION=3.11 because EL8's base python3 is 3.6 (too old)
# and we install python3.11 + python3.11-devel here. Without the pin uv would
# download its own 3.12, then psutil's wheel-less aarch64 source build would
# fail looking for Python.h that matches the wrong interpreter.
el8_default_install() {
    rpm -q epel-release >/dev/null 2>&1 || _run dnf -y install epel-release
    _pm_enable_el8_extras
    rhel_default_install
    dnf_install \
        gcc-toolset-11-gcc gcc-toolset-11-gcc-c++ gcc-toolset-11-libatomic-devel \
        python3.11 python3.11-devel xz
    # Symlink the toolset compiler into /usr/local/bin — skip once gcc already points there.
    if [ "$(readlink -f /usr/local/bin/gcc 2>/dev/null)" != /opt/rh/gcc-toolset-11/root/usr/bin/gcc ]; then
        _run cp /opt/rh/gcc-toolset-11/enable /etc/profile.d/gcc-toolset-11.sh 2>/dev/null || true
        _run ln -sf /opt/rh/gcc-toolset-11/root/usr/bin/gcc  /usr/local/bin/gcc  || true
        _run ln -sf /opt/rh/gcc-toolset-11/root/usr/bin/g++  /usr/local/bin/g++  || true
        _run ln -sf /opt/rh/gcc-toolset-11/root/usr/bin/cc   /usr/local/bin/cc   || true
        _run ln -sf /opt/rh/gcc-toolset-11/root/usr/bin/as   /usr/local/bin/as   || true
        _run ln -sf /opt/rh/gcc-toolset-11/root/usr/bin/make /usr/local/bin/make || true
    fi
    # Point python3 at 3.11 — skip once it already is.
    if ! python3 --version 2>/dev/null | grep -q '3\.11'; then
        _run update-alternatives --install /usr/bin/python3 python3 /usr/bin/python3.11 2000000
        _run update-alternatives --set python3 /usr/bin/python3.11
    fi
    export SETUP_PYTHON_VERSION="${SETUP_PYTHON_VERSION:-3.11}"
}

# EL9 (Alma 9, Rocky 9): base gcc is 11; we layer gcc-toolset-13 to match
# what the Dockerfiles' ENV PATH expects.
el9_default_install() {
    rhel_default_install
    dnf_install \
        gcc-toolset-13-gcc gcc-toolset-13-gcc-c++ gcc-toolset-13-libatomic-devel
    # Symlink the toolset compiler into /usr/local/bin — skip once gcc already points there.
    if [ "$(readlink -f /usr/local/bin/gcc 2>/dev/null)" != /opt/rh/gcc-toolset-13/root/usr/bin/gcc ]; then
        _run cp /opt/rh/gcc-toolset-13/enable /etc/profile.d/gcc-toolset-13.sh 2>/dev/null || true
        _run ln -sf /opt/rh/gcc-toolset-13/root/usr/bin/gcc  /usr/local/bin/gcc  || true
        _run ln -sf /opt/rh/gcc-toolset-13/root/usr/bin/g++  /usr/local/bin/g++  || true
        _run ln -sf /opt/rh/gcc-toolset-13/root/usr/bin/cc   /usr/local/bin/cc   || true
        _run ln -sf /opt/rh/gcc-toolset-13/root/usr/bin/as   /usr/local/bin/as   || true
        _run ln -sf /opt/rh/gcc-toolset-13/root/usr/bin/make /usr/local/bin/make || true
    fi
}
