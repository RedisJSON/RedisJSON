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
    TOOLCHAIN=$(grep -E '^\s*channel\s*=' rust-toolchain.toml | sed 's/.*=\s*"\([^"]*\)".*/\1/' | tr -d ' ')
    if [ -n "$TOOLCHAIN" ]; then
        $MODE rustup toolchain install "$TOOLCHAIN"
    else
        $MODE rustup update nightly
    fi
else
    $MODE rustup update nightly
fi

# Install required components for the active toolchain
$MODE rustup component add rust-src
$MODE rustup component add rustfmt
$MODE rustup component add clippy

# Symlink the toolchain into /usr/local/bin so it's on every shell's PATH —
# not just bash-login shells that source ~/.cargo/env. This is what makes
# `make build` work in the same shell session that just ran `make bootstrap`
# (and also covers direct `make -C modules/<name>` calls, CI runners, and
# any future script that shells out to cargo/rustc). $MODE handles
# sudo-vs-no-sudo identically to the rest of the script. Best-effort: if
# /usr/local/bin isn't writable the symlink silently no-ops; later login
# shells still pick up cargo via the ~/.cargo/env hook above, but the
# current shell would then not have cargo on PATH.
for bin in rustc cargo rustup rustfmt cargo-fmt clippy-driver cargo-clippy; do
    if [ -e "$HOME/.cargo/bin/$bin" ]; then
        $MODE ln -sf "$HOME/.cargo/bin/$bin" "/usr/local/bin/$bin" 2>/dev/null || true
    fi
done

# Verify cargo installation
cargo --version

rustup show
