#!/bin/bash
set -e
OS_TYPE=$(uname -s)
MODE=$1 # whether to install using sudo or not

activate_venv() {
	echo "copy ativation script to shell config"
	if [[ $OS_TYPE == Darwin ]]; then
		echo "source venv/bin/activate" >> ~/.bashrc
		echo "source venv/bin/activate" >> ~/.zshrc
	else
		echo "source $PWD/venv/bin/activate" >> ~/.bash_profile
	fi
}
python3 -m venv venv
activate_venv
source venv/bin/activate

pip3 install --upgrade pip
pip3 install -q --upgrade setuptools
echo "pip version: $(pip3 --version)"
echo "pip path: $(which pip3)"

pip3 install -q -r tests/pytest/requirements.txt
# These packages are needed to build the package
pip3 install -q -r .install/build_package_requirements.txt

# List installed packages
pip3 list
