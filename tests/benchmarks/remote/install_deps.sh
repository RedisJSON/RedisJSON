#!/bin/bash
set -x 

# Ensure terraform is available
TF_EXE_FILE_NAME=${TF_EXE_FILE_NAME:-$(which terraform)}
if [[ ! -z "${TF_EXE_FILE_NAME}" ]]; then
    echo "terraform not available. It is not specified explicitly and not found in \$PATH"
    echo "Downloading terraform..."
    wget https://releases.hashicorp.com/terraform/0.13.5/terraform_0.13.5_linux_amd64.zip
    unzip terraform_0.13.5_linux_amd64.zip
    mv terraform ${TF_EXE_FILE_NAME}
    chmod 755 ${TF_EXE_FILE_NAME}
    rm terraform_0.13.5_linux_amd64.zip
fi
echo "Checking terraform version..."
${TF_EXE_FILE_NAME} --version
