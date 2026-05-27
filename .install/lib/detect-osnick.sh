#!/usr/bin/env bash
# Map the host to a canonical OSNICK that names a file in ../os/.
#
# Sourced by install_script.sh. Provides one function: detect_osnick.
# Result is printed on stdout; empty string on unrecognised hosts.
#
# Caller may pre-set $OSNICK in the environment (e.g. Docker ARG/ENV) to
# bypass detection entirely; we still normalise rhel*->rocky* so callers
# don't have to remember which file we picked.

detect_osnick() {
    local osnick="${OSNICK:-}"

    if [ -n "$osnick" ]; then
        case "$osnick" in
            rhel8)  osnick=rocky8  ;;
            rhel9)  osnick=rocky9  ;;
            rhel10) osnick=rocky10 ;;
        esac
        printf '%s\n' "$osnick"
        return 0
    fi

    if [ "$(uname -s)" = "Darwin" ]; then
        printf 'macos\n'
        return 0
    fi

    if [ -r /etc/os-release ]; then
        # shellcheck disable=SC1091
        . /etc/os-release
        case "${ID:-}:${VERSION_ID:-}" in
            ubuntu:18.04)             printf 'bionic\n'          ;;
            ubuntu:20.04)             printf 'focal\n'           ;;
            ubuntu:22.04)             printf 'jammy\n'           ;;
            ubuntu:24.04)             printf 'noble\n'           ;;
            ubuntu:26.*)              printf 'resolute\n'        ;;
            debian:11*)               printf 'bullseye\n'        ;;
            debian:12*)               printf 'bookworm\n'        ;;
            debian:13*)               printf 'trixie\n'          ;;
            alpine:*)                 printf 'alpine\n'          ;;
            amzn:2)                   printf 'amazonlinux2\n'    ;;
            amzn:2023)                printf 'amazonlinux2023\n' ;;
            rocky:8*)                 printf 'rocky8\n'          ;;
            rocky:9*)                 printf 'rocky9\n'          ;;
            rocky:10*)                printf 'rocky10\n'         ;;
            rhel:8*|redhat:8*)        printf 'rocky8\n'          ;;
            rhel:9*|redhat:9*)        printf 'rocky9\n'          ;;
            rhel:10*|redhat:10*)      printf 'rocky10\n'         ;;
            almalinux:8*)             printf 'alma8\n'           ;;
            almalinux:9*)             printf 'alma9\n'           ;;
            almalinux:10*)            printf 'alma10\n'          ;;
            mariner:2*)               printf 'mariner2\n'        ;;
            azurelinux:3*)            printf 'azurelinux3\n'     ;;
            *)                        printf '\n'                ;;
        esac
        return 0
    fi

    printf '\n'
}
