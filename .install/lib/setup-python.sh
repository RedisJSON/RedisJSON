#!/usr/bin/env bash
# Provision uv + a project-local venv + pip dependencies for redisjson.
#
# Sourced by install_script.sh after the OS package install. Reads $ROOT
# (set by install_script.sh) and $HERE (path to .install/). Writes
# $ROOT/venv/.
#
# Replaces venv bootstrap that used to run from the Makefile after
# install_script.sh: all pip work lives here so `make bootstrap` is just
# install_script.sh + done.

# uv installs to ~/.local/bin (or ~/.cargo/bin), which is not on PATH in the
# non-login bootstrap subshell — detect it there too, not just via PATH.
_have_uv() { command -v uv >/dev/null 2>&1 || [ -x "$HOME/.local/bin/uv" ] || [ -x "$HOME/.cargo/bin/uv" ]; }

# list / dry-run are read-only dependency reports — they must run in EVERY
# environment, so handle them BEFORE the ROOT/HERE asserts below.
if [ "${CHECK_DEPS:-0}" = 1 ]; then
    # uv presence, routed through OPTIONAL_PKGS like any other dep.
    if _have_uv; then _uv=ok; else _uv=missing; fi
    if _is_optional uv; then
        [ "$_uv" = ok ] && DEPS_OPT_OK="$DEPS_OPT_OK uv" || DEPS_OPT_MISSING="$DEPS_OPT_MISSING uv"
    else
        [ "$_uv" = ok ] && DEPS_OK="$DEPS_OK uv" || DEPS_MISSING="$DEPS_MISSING uv"
    fi
    return 0 2>/dev/null || exit 0
fi

if [ "${DRY_RUN:-0}" = 1 ]; then
    # Print the exact uv + venv + pip sequence bootstrap would run (nothing is
    # executed) so dry-run output is a copy-pasteable script. Guarded on what's
    # already present so a provisioned host prints only the gaps: uv install
    # only if uv is missing; venv + pip only if the venv doesn't exist yet
    # (mirrors the real `[ ! -d venv ]` guard below).
    if [ ! -d "$ROOT/venv" ]; then
        _have_uv || _dry_line "curl -LsSf https://astral.sh/uv/install.sh | sh"
        # uv installs to ~/.local/bin (or ~/.cargo/bin), which may not be on
        # PATH — export it so the uv commands below resolve when pasted, exactly
        # as bootstrap does after installing uv.
        _dry_line 'export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"'
        _dry_line "uv venv \"$ROOT/venv\" --python \"${SETUP_PYTHON_VERSION:-3.12}\""
        _dry_line "uv pip install --python \"$ROOT/venv/bin/python\" --upgrade pip wheel \"setuptools<81\""
        _dry_line "uv pip install --python \"$ROOT/venv/bin/python\" -r \"$HERE/build_package_requirements.txt\""
        [ -f "$ROOT/tests/pytest/requirements.txt" ] && _dry_line "uv pip install --python \"$ROOT/venv/bin/python\" -r \"$ROOT/tests/pytest/requirements.txt\""
    fi
    return 0 2>/dev/null || exit 0
fi

# Required by callers — set by install_script.sh. Fail fast if absent rather
# than producing a confusing `uv venv ""` failure later.
: "${ROOT:?setup-python.sh: ROOT not set (must be sourced by install_script.sh)}"
: "${HERE:?setup-python.sh: HERE not set (must be sourced by install_script.sh)}"

if ! command -v uv >/dev/null 2>&1; then
    echo "==> [redisjson] installing uv"
    curl -LsSf https://astral.sh/uv/install.sh | sh
    export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"
fi

if ! command -v uv >/dev/null 2>&1; then
    echo "setup-python.sh: ERROR: uv installation failed; cannot create venv" >&2
    # Hard-fail here so the actual root cause shows up at the top of the
    # failed CI step. Returning success would surface a misleading "venv
    # missing" error from run-tests/action.yml far below in the log.
    return 1 2>/dev/null || exit 1
fi

# A stale or partial venv (e.g. a previous `make bootstrap` aborted halfway,
# or the developer ran `python3 -m venv` against a now-missing python) shows
# up as `$ROOT/venv` existing but `bin/python` not being executable. Wipe
# and recreate so we don't trip the executable check below.
if [ -d "$ROOT/venv" ] && [ ! -x "$ROOT/venv/bin/python" ]; then
    echo "==> [redisjson] $ROOT/venv looks broken (no bin/python); recreating"
    rm -rf "$ROOT/venv"
fi

if [ ! -d "$ROOT/venv" ]; then
    uv venv "$ROOT/venv" --python "${SETUP_PYTHON_VERSION:-3.12}"
fi

if [ ! -x "$ROOT/venv/bin/python" ]; then
    echo "setup-python.sh: missing $ROOT/venv/bin/python (uv venv step failed?)" >&2
    exit 1
fi

# All pip work goes through `uv pip --python <venv>` (never --system, never
# under sudo). Sourcing under sudo would otherwise resolve uv against /usr's
# python3 (3.6 on EL8) and break rltest.
uv_pip() {
    uv pip install --python "$ROOT/venv/bin/python" "$@"
}

# setuptools<81: setuptools 81 (2025-07) removed the bundled distutils
# shim plus several legacy `setup.py` install paths. Some upstream test deps
# in build_package_requirements.txt / tests/pytest/requirements.txt still
# rely on them (gevent / RLTest are the historical laggards). To check if
# the pin can be relaxed: temporarily drop the upper bound and run
# `make bootstrap` clean — if everything still installs, the constraint
# is obsolete.
uv_pip --upgrade pip wheel "setuptools<81"
uv_pip -r "$HERE/build_package_requirements.txt"

if [ -f "$ROOT/tests/pytest/requirements.txt" ]; then
    (cd "$ROOT" && uv_pip -r tests/pytest/requirements.txt)
fi
