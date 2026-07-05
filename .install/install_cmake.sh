#!/bin/bash
set -euo pipefail
version=3.25.1
processor=$(uname -m)
OS_TYPE=$(uname -s)
MODE="${1:-}" # whether to install using sudo or not (empty when already root)

# Skip when an adequate cmake is already on PATH (distro package or another
# module's bootstrap on the same host): re-running must be a no-op, and
# blindly re-installing would overwrite /usr/local under other checkouts.
have_ver="$(cmake --version 2>/dev/null | awk '/cmake version/ {print $3; exit}' || true)"
if [[ -n "$have_ver" ]] && printf '%s\n' "$version" "$have_ver" | sort -V -C 2>/dev/null; then
    echo "cmake $have_ver already installed (>= required $version) - skipping"
    exit 0
fi

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

    tmpdir=$(mktemp -d)
    wget -P "$tmpdir" https://github.com/Kitware/CMake/releases/download/v${version}/${filename}
    chmod u+x "$tmpdir/$filename"
    $MODE "$tmpdir/$filename" --skip-license --prefix=/usr/local --exclude-subdir
    rm -rf "$tmpdir"
    cmake --version
fi
