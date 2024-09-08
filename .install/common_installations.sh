#!/bin/bash
set -e
OS_TYPE=$(uname -s)
MODE=$1 # whether to install using sudo or not

pip3 install --upgrade pip
pip3 install -q --upgrade setuptools
echo "pip version: $(pip3 --version)"
echo "pip path: $(which pip3)"

pip3 install -q -v --no-build-isolation --no-cache-dir -r ./tests/pytest/requirements.txt

# List installed packages
pip3 list
