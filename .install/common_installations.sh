#!/bin/bash
set -e
OS_TYPE=$(uname -s)
MODE=$1 # whether to install using sudo or not

pip install --upgrade pip
pip install -q --upgrade setuptools
echo "pip version: $(pip3 --version)"
echo "pip path: $(which pip3)"

pip install -q -r tests/pytest/requirements.txt

# List installed packages
pip list
