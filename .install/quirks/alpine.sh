#!/usr/bin/env bash
#
# Alpine-only extras that don't fit the cross-distro abstract package map in
# ../../dependencies.yaml. Run after the standard `apk add` of the abstract
# system list.
#
# Why these are here and not in dependencies.yaml:
#   - musl/Alpine specific runtime/headers (musl-dev, linux-headers, gcompat,
#     libstdc++, libgcc, bsd-compat-headers): only meaningful on Alpine.
#   - py3-* prebuilt wheels (cryptography, numpy, psutil) avoid building the
#     C extensions against musl from source during pip install.
#   - openblas-dev / xsimd: math/SIMD libs used by some test deps.
#   - openssh, xz, py-virtualenv: kept verbatim from the legacy
#     .install/alpine.sh script for parity.
#
# Sourced by install_script.sh; receives MODE as $1.

set -eu

MODE="${1:-}"

$MODE apk add --no-cache \
    bash \
    musl-dev \
    linux-headers \
    libffi-dev \
    bsd-compat-headers \
    gcompat \
    libstdc++ \
    libgcc \
    openblas-dev \
    xsimd \
    xz \
    openssh \
    py3-cryptography \
    py3-numpy \
    py3-psutil \
    py-virtualenv
