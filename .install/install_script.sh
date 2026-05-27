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
git config --global --add safe.directory '*' || true

# shellcheck source=lib/setup-python.sh
. "$LIB/setup-python.sh"

# Install Rust toolchain via rustup (skipped on Alpine: musl rustc/cargo
# come from apk via lib/packages.sh's ALPINE_BASE).
if [ "$OSNICK" != "alpine" ]; then
    if [ -x "$HERE/getrust.sh" ]; then
        echo "==> [redisjson] installing Rust via .install/getrust.sh"
        (cd "$ROOT" && bash "$HERE/getrust.sh" "$MODE")
    else
        echo "install_script.sh: getrust.sh missing at $HERE/getrust.sh" >&2
        exit 1
    fi
fi

echo "==> [redisjson] install_script.sh: done"
