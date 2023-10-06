#!/bin/bash

tdnf install -q -y build-essential git wget ca-certificates tar openssl-devel \
    cmake python3 python3-pip rust clang which

pip install --upgrade setuptools
pip install --upgrade pip
pip install -q -r tests/pytest/requirements.txt

# These packages are needed to build the package
pip install -q addict toml jinja2 ramp-packer

# Install aws-cli for uploading artifacts to s3
curdir="$PWD"
cd /tmp/
curl "https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip" -o "awscliv2.zip"
unzip awscliv2.zip
./aws/install
cd $curdir