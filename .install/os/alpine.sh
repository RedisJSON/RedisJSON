#!/usr/bin/env bash
# Alpine Linux. ALPINE_BASE includes musl extras; extra apk wheels for pip.

. "$LIB/packages.sh"

alpine_default_install
apk_install py3-cryptography py3-numpy py3-psutil openblas-dev xsimd

# bindgen uses dlopen to load libclang.so. Alpine ships only versioned symlinks,
# typically under /usr/lib/llvmXX/lib/ rather than /usr/lib/ directly.
if [ ! -e /usr/lib/libclang.so ]; then
    _libclang=""
    for _c in \
        /usr/lib/libclang.so.21.1 \
        /usr/lib/llvm21/lib/libclang.so.21.1 \
        /usr/lib/libclang.so.18.1 \
        /usr/lib/llvm18/lib/libclang.so.18.1; do
        [ -e "$_c" ] && { _libclang="$_c"; break; }
    done
    if [ -n "$_libclang" ]; then
        $SUDO ln -sf "$_libclang" /usr/lib/libclang.so
    else
        echo "alpine.sh: WARNING: no libclang.so found; bindgen's dlopen will fail later" >&2
    fi
fi
