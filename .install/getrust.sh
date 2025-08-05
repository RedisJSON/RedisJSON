#!/bin/bash

MODE=$1 # whether to install using sudo or not

# Download and install rustup
$MODE curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

# Source the cargo environment script to update the PATH
echo "source $HOME/.cargo/env" >> $HOME/.bashrc
source $HOME/.cargo/env

# Update rustup
$MODE rustup update

# Install the toolchain specified in rust-toolchain.toml (if present)
if [ -f "rust-toolchain.toml" ]; then
    echo "Found rust-toolchain.toml, reading specified toolchain..."
    # Extract the channel from rust-toolchain.toml
    TOOLCHAIN=$(grep -E '^\s*channel\s*=' rust-toolchain.toml | sed 's/.*=\s*"\([^"]*\)".*/\1/' | tr -d ' ')
    if [ -n "$TOOLCHAIN" ]; then
        echo "Installing toolchain: $TOOLCHAIN"
        $MODE rustup toolchain install "$TOOLCHAIN"
    else
        echo "Could not parse toolchain from rust-toolchain.toml, falling back to latest nightly"
        $MODE rustup update nightly
    fi
else
    echo "No rust-toolchain.toml found, installing latest nightly..."
    $MODE rustup update nightly
fi

# Install rust-src component for the active toolchain (for RedisJSON build with address sanitizer)
$MODE rustup component add rust-src

# Verify cargo installation
cargo --version

rustup show
