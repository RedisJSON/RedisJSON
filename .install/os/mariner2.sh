#!/usr/bin/env bash
# CBL-Mariner 2.0 (tdnf).

. "$LIB/packages.sh"

tdnf_default_install

# Install aws-cli for uploading artifacts to s3. Subshell at /tmp keeps the
# zip and extracted ./aws/ directory out of the caller's CWD (the script is
# sourced from install_script.sh with CWD at .install/).
ARCH=$(uname -m)
[[ "$ARCH" == "aarch64" ]] && ARCH="aarch64" || ARCH="x86_64"
(cd /tmp && curl "https://awscli.amazonaws.com/awscli-exe-linux-${ARCH}.zip" -o awscliv2.zip && unzip -qo awscliv2.zip && ./aws/install --update)
