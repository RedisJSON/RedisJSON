#!/usr/bin/env bash
# Install build/test dependencies for RedisJSON.
#
# Flow:
#   1. detect canonical OSNICK (uname + /etc/os-release)
#   2. source lib/pm.sh — exports PM, SUDO, install helpers
#   3. source os/<osnick>.sh — installs OS packages and inlines any quirks
#   4. source lib/setup-python.sh — uv + venv + pip deps (incl. RLTest pins)
#   5. non-Alpine: run getrust.sh — Rust toolchain via rustup (Alpine uses apk cargo)
#
# Same calling convention as before:
#   ./install_script.sh [sudo]    # "sudo" wraps installs (Linux); empty
#                                 # for macOS or already-root containers.

set -euo pipefail

MODE="${1:-}"

HERE="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
LIB="$HERE/lib"

# shellcheck source=lib/detect-osnick.sh
. "$LIB/detect-osnick.sh"
# shellcheck source=lib/pm.sh
. "$LIB/pm.sh"

OSNICK="$(detect_osnick)"
if [ -z "$OSNICK" ]; then
    echo "install_script.sh: cannot detect OSNICK" >&2
    exit 1
fi

osfile="$HERE/os/$OSNICK.sh"
if [ ! -f "$osfile" ]; then
    echo "install_script.sh: unsupported OSNICK '$OSNICK' (no $osfile)" >&2
    echo "Supported: $(ls "$HERE/os" 2>/dev/null | sed 's/\.sh$//' | xargs)" >&2
    exit 1
fi

echo "==> [redisjson] OSNICK=$OSNICK PM=$PM"

# shellcheck disable=SC1090
. "$osfile"

# Allow git operations on any checked-out source even when its uid doesn't
# match the current user (common in CI containers). `--global` with wildcard
# `*` is intentional: `git config --local` would itself fail under "dubious
# ownership", so a global rule is needed before any per-repo command can run.
# Skipped in check-deps mode — a check must not mutate anything.
if [ "${CHECK_DEPS:-0}" != 1 ]; then
    git config --global --add safe.directory '*' || true
fi

# shellcheck source=lib/setup-python.sh
. "$LIB/setup-python.sh"

# Install Rust toolchain via rustup (skipped on Alpine: musl rustc/cargo
# come from apk via lib/packages.sh's ALPINE_BASE). In check-deps mode we
# record whether cargo is present instead of installing it.
if [ "$OSNICK" != "alpine" ]; then
    if [ "${CHECK_DEPS:-0}" = 1 ]; then
        if command -v cargo >/dev/null 2>&1; then DEPS_OK="$DEPS_OK rust"; else DEPS_MISSING="$DEPS_MISSING rust"; fi
    elif [ -x "$HERE/getrust.sh" ]; then
        echo "==> [redisjson] installing Rust via .install/getrust.sh"
        (cd "$ROOT" && bash "$HERE/getrust.sh" "$MODE")
    else
        echo "install_script.sh: getrust.sh missing at $HERE/getrust.sh" >&2
        exit 1
    fi
fi

if [ "${CHECK_DEPS:-0}" = 1 ]; then
    n_ok=$(set -- $DEPS_OK; echo $#)
    n_missing=$(set -- $DEPS_MISSING; echo $#)
    total=$((n_ok + n_missing))
    echo
    echo "==> [redisjson] dependency check (OSNICK=$OSNICK, PM=$PM) — nothing was installed"
    # Colors on a real terminal; plain text when piped (CI logs) so no
    # escape-code noise. RED = missing (the headline), GREEN = installed.
    if [ -t 1 ]; then RED="$(printf '\033[1;31m')"; GRN="$(printf '\033[1;32m')"; RST="$(printf '\033[0m')"; else RED=""; GRN=""; RST=""; fi
    # "not installed" is the headline of a check run — print it first, bold red.
    if [ -n "$DEPS_MISSING" ]; then
        echo "${RED}NOT INSTALLED ($n_missing):${RST}"
        for _p in $DEPS_MISSING; do echo "${RED}    $_p${RST}"; done
    else
        echo "${GRN}not installed: (none)${RST}"
    fi
    # Full satisfied list is reassurance, not action: summarize by count,
    # print it in full only under VERBOSE=1.
    if [ "${VERBOSE:-0}" = 1 ]; then
        echo "${GRN}installed:${RST}"
        for _p in $DEPS_OK; do echo "${GRN}    $_p${RST}"; done
        [ -n "$DEPS_OK" ] || echo "    (none)"
    else
        echo "${GRN}installed: $n_ok/$total (set VERBOSE=1 to list)${RST}"
    fi
    # Non-zero exit when anything is missing so CI / callers can gate on it.
    [ "$n_missing" -eq 0 ] || exit 1
    exit 0
fi

echo "==> [redisjson] install_script.sh: done"
