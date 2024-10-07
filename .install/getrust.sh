#!/bin/bash

MODE=$1 # whether to install using sudo or not

# Download and install rustup
$MODE curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

# Source the cargo environment script to update the PATH
echo "source $HOME/.cargo/env" >> $HOME/.bashrc
source $HOME/.cargo/env

# Update rustup and install nightly toolchain
$MODE rustup update
$MODE rustup update nightly

# for RedisJSON build with addess santizer
$MODE rustup component add --toolchain nightly rust-src

# Verify cargo installation
cargo --version

rustup show
