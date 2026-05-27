#!/usr/bin/env bash
# Azure Linux 3 (tdnf). Same shape as mariner2 — Microsoft's distro lineage,
# tdnf-based, ships a smaller package set than dnf.

# shellcheck source=../lib/packages.sh
. "$LIB/packages.sh"

tdnf_default_install

# Install aws-cli for uploading artifacts to s3. Subshell at /tmp keeps the
# zip and extracted ./aws/ directory out of the caller's CWD (the script is
# sourced from install_script.sh with CWD at .install/).
(cd /tmp && curl "https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip" -o awscliv2.zip && unzip -q awscliv2.zip && ./aws/install)
