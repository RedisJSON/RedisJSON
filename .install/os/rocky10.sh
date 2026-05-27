#!/usr/bin/env bash
# Rocky Linux 10. Same as alma10.

. "$LIB/packages.sh"

rhel_default_install
dnf_install clang-tools-extra
