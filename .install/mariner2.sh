#!/bin/bash

tdnf install -q -y build-essential git wget ca-certificates tar openssl-devel \
    cmake python3 python3-pip rust clang which

pip install --upgrade setuptools
pip install --upgrade pip
pip install -r tests/pytest/requirements.txt

# These packages are needed to build the package
pip install addict toml jinja2 ramp-packer