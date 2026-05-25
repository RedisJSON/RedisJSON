#!/usr/bin/env bash
#
# Rocky Linux 9 extras: identical to alma9 (both are RHEL 9 rebuilds).
# See quirks/alma9.sh for rationale.
#
# Sourced by install_script.sh; receives MODE as $1.

set -eu
. "$(dirname "${BASH_SOURCE[0]:-$0}")/alma9.sh" "${1:-}"
