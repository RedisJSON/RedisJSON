#!/bin/bash
set -x
TF_VERSION=${TF_VERSION:-"0.14.8"}

# Ensure terraform is available
TF_EXE_FILE_NAME=${TF_EXE_FILE_NAME:-$(which terraform)}
if [[ ! -z "${TF_EXE_FILE_NAME}" ]]; then
    echo "terraform not available. It is not specified explicitly and not found in \$PATH"
    echo "Downloading terraform..."
    wget https://releases.hashicorp.com/terraform/${TF_VERSION}/terraform_${TF_VERSION}_linux_amd64.zip
    unzip terraform_${TF_VERSION}_linux_amd64.zip
    mv terraform ${TF_EXE_FILE_NAME}
    chmod 755 ${TF_EXE_FILE_NAME}
    rm terraform_${TF_VERSION}_linux_amd64.zip
fi
echo "Checking terraform version..."
${TF_EXE_FILE_NAME} --version
