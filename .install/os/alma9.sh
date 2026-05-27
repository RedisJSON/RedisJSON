#!/usr/bin/env bash
# AlmaLinux 9. EL9 + gcc-toolset-13.

. "$LIB/packages.sh"

el9_default_install
dnf_install clang-tools-extra
