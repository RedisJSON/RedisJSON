#!/usr/bin/env bash
# CBL-Mariner 2.0 (tdnf).

. "$LIB/packages.sh"

tdnf_default_install

# Install aws-cli for uploading artifacts to s3
install_aws_cli
