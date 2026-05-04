#!/usr/bin/env bash
#
# Rocky Linux 8 extras: identical to alma8 (both are RHEL 8 rebuilds).
# See quirks/alma8.sh for rationale.
#
# Sourced by install_script.sh; receives MODE as $1.

set -eu
. "$(dirname "${BASH_SOURCE[0]:-$0}")/alma8.sh" "${1:-}"
