#!/bin/bash
set -euo pipefail

# Install the Rust toolchain pinned by rust-toolchain.toml via rustup.
#
# Contract (keeps bootstraps of co-located modules from colliding):
#   - rustup owns ~/.rustup and ~/.cargo only; nothing is written to
#     /usr/local/bin, /usr/bin, or shell profiles.
#   - No default toolchain is set and `rustup update` is never run, so this
#     script cannot change which Rust version any other checkout resolves.
#     Version selection happens per-directory through rust-toolchain.toml
#     (cargo/rustc in ~/.cargo/bin are rustup proxies that read it).
#   - The Makefile prepends $(CARGO_HOME)/bin to PATH, so `make bootstrap`
#     followed by `make build` works in the same shell without profile edits.
#
# Idempotent: re-running skips anything already installed.
#
# Called by install_script.sh with cwd = repo root. The historical MODE/sudo
# argument is accepted but unused: everything here is user-scoped.

export PATH="$HOME/.cargo/bin:$PATH"

# Install rustup if missing. --default-toolchain none: the pinned toolchain is
# installed explicitly below; installing `stable` here would just waste time
# and disk. Also covers hosts that have a distro cargo but no rustup — the
# rustup proxies are required for rust-toolchain.toml resolution.
if ! command -v rustup &>/dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain none
fi

TOOLCHAIN=$(sed -n 's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' rust-toolchain.toml | head -1)
if [ -z "$TOOLCHAIN" ]; then
    echo "getrust.sh: cannot read pinned channel from $(pwd)/rust-toolchain.toml" >&2
    exit 1
fi

rustup toolchain install "$TOOLCHAIN"
rustup component add --toolchain "$TOOLCHAIN" rust-src rustfmt clippy

# Verify the full chain: proxy on PATH + rust-toolchain.toml resolution in
# this directory + the toolchain we just installed.
cargo --version
rustup show
