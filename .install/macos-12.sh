#!/bin/bash

echo "::error $(make --version)"

export HOMEBREW_NO_AUTO_UPDATE=1
BREW_PREFIX=$(brew --prefix)
GNUBIN=$BREW_PREFIX/opt/make/libexec/gnubin
LLVM=$BREW_PREFIX/opt/llvm@16/bin
COREUTILS=$BREW_PREFIX/opt/coreutils/libexec/gnubin

brew update
brew install coreutils
brew install make
brew install llvm@16

echo "export PATH=$COREUTILS:$LLVM:$GNUBIN:$PATH" >> ~/.bashrc
echo "export PATH=$COREUTILS:$LLVM:$GNUBIN:$PATH" >> ~/.zshrc
source ~/.bashrc
source ~/.zshrc

brew install openssl

version=3.25.1
processor=$(uname -m)
OS_TYPE=$(uname -s)
MODE=$1 # whether to install using sudo or not

if [[ $OS_TYPE = 'Darwin' ]]
then
    brew install cmake
else
    if [[ $processor = 'x86_64' ]]
    then
        filename=cmake-${version}-linux-x86_64.sh
    else
        filename=cmake-${version}-linux-aarch64.sh
    fi

    wget https://github.com/Kitware/CMake/releases/download/v${version}/${filename}
    chmod u+x ./${filename}
    $MODE ./${filename} --skip-license --prefix=/usr/local --exclude-subdir
    cmake --version
fi

echo "::error $(make --version)"