#!/bin/bash

# Download and install rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

# Source the cargo environment script to update the PATH
source "$HOME/.cargo/env"

# Update rustup and install nightly toolchain
rustup update
rustup update nightly

# for RedisJSON build with addess santizer
rustup component add rust-src --toolchain nightly

# Verify cargo installation
cargo --version