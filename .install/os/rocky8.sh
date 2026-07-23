#!/usr/bin/env bash
# Rocky Linux 8. Same as alma8.

. "$LIB/packages.sh"

el8_default_install
dnf_install clang-tools-extra

# Backport `dataclasses` (stdlib in py 3.7+) for the system python3, which on
# EL8 is 3.6. NOTE: setup-python.sh creates the venv against python3.11 and
# all pip/uv work goes through that, so nothing in this PR's call graph
# obviously consumes this shim. It exists as a safety net for any historical
# readies-driven build script that still imports against bare `python3`. To
# verify it's dead code: run `make bootstrap && make build && make test` on a
# clean EL8 image with this block removed — if everything passes, drop it.
# `|| true` keeps a missing/blocked pip from breaking the rest of the bootstrap.
if command -v python3 >/dev/null 2>&1 && ! python3 -c 'import dataclasses' 2>/dev/null; then
    _sh 'python3 -m pip install --disable-pip-version-check -q "dataclasses>=0.8,<1" || true'
fi
