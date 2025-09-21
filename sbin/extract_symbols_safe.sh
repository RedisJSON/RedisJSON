#!/bin/bash

# Safe symbol extraction script that handles Alpine ARM64 issues
TARGET="$1"

# Check if we're on Alpine ARM64 and skip symbol extraction if so
if [ -f "/etc/alpine-release" ] && [ "$(uname -m)" = "aarch64" ]; then
    echo "Skipping debug symbol extraction on Alpine ARM64 to avoid musl/objcopy issues"
    exit 0
fi

# If not Alpine ARM64, exit with error code 1 so the Makefile will run extract_symbols
echo "Not Alpine ARM64 - will extract debug symbols"
exit 1
