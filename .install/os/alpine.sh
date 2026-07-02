#!/usr/bin/env bash
# Alpine Linux. ALPINE_BASE includes musl extras; extra apk wheels for pip.

. "$LIB/packages.sh"

alpine_default_install
apk_install py3-cryptography py3-numpy py3-psutil openblas-dev xsimd

# bindgen uses dlopen to load libclang.so. Alpine ships only versioned symlinks.
if [ ! -e /usr/lib/libclang.so ]; then
    if [ -e /usr/lib/libclang.so.21.1 ]; then
        ln -sf libclang.so.21.1 /usr/lib/libclang.so
    elif [ -e /usr/lib/libclang.so.18.1 ]; then
        ln -sf libclang.so.18.1 /usr/lib/libclang.so
    fi
fi
