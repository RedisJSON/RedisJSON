#!/usr/bin/env bash
# Azure Linux 3 (tdnf). Same shape as mariner2 — Microsoft's distro lineage,
# tdnf-based, ships a smaller package set than dnf.

. "$LIB/packages.sh"

tdnf_default_install

install_aws_cli
