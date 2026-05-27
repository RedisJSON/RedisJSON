#!/usr/bin/env bash
# Alpine Linux. ALPINE_BASE includes musl extras; extra apk wheels for pip.

. "$LIB/packages.sh"

alpine_default_install
apk_install py3-cryptography py3-numpy py3-psutil openblas-dev xsimd
