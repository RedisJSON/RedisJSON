#!/usr/bin/env sh

export CWD=$(dirname `which "${0}"`)
export CLANG_VERSION=18
export DEBIAN_FRONTEND=noninteractive
MODE=$1 # whether to install using sudo or not

wget https://apt.llvm.org/llvm.sh -O llvm.sh

chmod u+x llvm.sh

# expected to fail:
$MODE ./llvm.sh $CLANG_VERSION

$MODE apt-get install python3-lldb-18 --yes --force-yes

$MODE ./llvm.sh $CLANG_VERSION

$MODE $CWD/update_clang_alternatives.sh $CLANG_VERSION 1
