#!/usr/bin/env bash
# Amazon Linux 2023. dnf-based; clang-format as clang-tools-extra.

. "$LIB/packages.sh"

rhel_default_install
dnf_install clang-tools-extra
