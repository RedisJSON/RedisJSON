#!/usr/bin/env bash
# AlmaLinux 10. Base toolchain only; Dockerfile may run install_cmake.sh later.

. "$LIB/packages.sh"

rhel_default_install
dnf_install clang-tools-extra
