#!/bin/bash

# Safe symbol extraction script that handles Alpine ARM64 issues
TARGET="$1"

# Check if we're building for Alpine ARM64 (either native or in container)
# Also check if objcopy is available to avoid command not found errors
IS_ALPINE_ARM64=false

if [ -f "/etc/alpine-release" ] && [ "$(uname -m)" = "aarch64" ]; then
    IS_ALPINE_ARM64=true
elif ! command -v objcopy >/dev/null 2>&1; then
    # If objcopy is not available, we're likely in a CI environment building for Alpine
    echo "objcopy not available - likely building for Alpine in CI environment"
    IS_ALPINE_ARM64=true
fi

if [ "$IS_ALPINE_ARM64" = true ]; then
    echo "Skipping debug symbol extraction on Alpine ARM64 to avoid musl/objcopy issues"
    exit 0
fi

# If not Alpine ARM64 and objcopy is available, exit with error code 1 so the Makefile will run extract_symbols
echo "Not Alpine ARM64 and objcopy available - will extract debug symbols"
exit 1
