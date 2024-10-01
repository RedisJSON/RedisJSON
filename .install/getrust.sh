#!/bin/bash

MODE=$1 # whether to install using sudo or not

# Download and install rustup
$MODE curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

# Source the cargo environment script to update the PATH
$MODE source "$HOME/.cargo/env"

# Update rustup and install nightly toolchain
$MODE rustup update
$MODE rustup update nightly

# for RedisJSON build with addess santizer
$MODE rustup component add --toolchain nightly rust-src

# Verify cargo installation
cargo --version

rustup show

profile_d=`get_profile_d`
if [[ -f $HOME/.cargo/env ]]; then
  $MODE cp $HOME/.cargo/env $profile_d/rust.sh
elif [[ -f /usr/local/cargo/env ]]; then
	$MODE cp /usr/local/cargo/env $profile_d/rust.sh
else
	eprint "rust: environment file not found"
	exit 1
fi
