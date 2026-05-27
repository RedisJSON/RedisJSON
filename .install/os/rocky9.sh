#!/usr/bin/env bash
# Rocky Linux 9. Same as alma9.

. "$LIB/packages.sh"

el9_default_install
dnf_install clang-tools-extra
