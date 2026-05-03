#!/usr/bin/env bash
#
# Install build/test dependencies for RedisJSON.
#
# Two modes:
#
#   1) Abstract (preferred). Reads ../dependencies.yaml and resolves abstract
#      dep names (gcc, openssl_dev, ...) to concrete packages for the host's
#      package manager. Activated for every OSNICK listed under
#      `migrated_osnicks` in that YAML. After packages are installed, runs
#      .install/quirks/<osnick>.sh if it exists.
#
#   2) Legacy. Falls back to sourcing the existing .install/<distro>_<ver>.sh
#      script when the OSNICK is not yet migrated. This is what the old
#      install_script.sh always did, kept verbatim so non-migrated OSes
#      continue to work unchanged during the rollout.
#
# OS detection mirrors sbin/pack.sh: uses readies' `bin/platform --osnick`
# when available so the install nick matches the release-artefact nick, with
# a /etc/os-release fallback for environments that don't ship readies.
#
# Usage: install_script.sh [MODE]
#   MODE   "sudo" or "" (whether to wrap install commands with sudo)

set -eu

MODE="${1:-}"

HERE="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
DEPS_YAML="$ROOT/dependencies.yaml"

# ----------------------------------------------------------------------------
# Detect OSNICK (mirrors sbin/pack.sh)
# ----------------------------------------------------------------------------

OSNICK=""

if [ "$(uname -s)" = "Darwin" ]; then
    OSNICK="macos"
elif [ -x "$ROOT/deps/readies/bin/platform" ] && command -v python3 >/dev/null 2>&1; then
    OSNICK="$("$ROOT/deps/readies/bin/platform" --osnick 2>/dev/null || true)"
    # AlmaLinux is reported as `centosN`; override to alma<major> like pack.sh.
    if [ -r /etc/os-release ]; then
        # shellcheck disable=SC1091
        . /etc/os-release
        [ "${ID:-}" = "almalinux" ] && OSNICK="alma${VERSION_ID%%.*}"
    fi
    # Normalise to the nicknames our Dockerfile.<nick> files use.
    case "$OSNICK" in
        amzn2)        OSNICK="amazonlinux2"     ;;
        amzn2023)     OSNICK="amazonlinux2023"  ;;
        centos8|ol8)  OSNICK="rocky8"           ;;
        centos9)      OSNICK="rocky9"           ;;
        centos10)     OSNICK="rocky10"          ;;
    esac
fi

if [ -z "$OSNICK" ] && [ -r /etc/os-release ]; then
    # shellcheck disable=SC1091
    . /etc/os-release
    case "${ID:-}:${VERSION_ID:-}" in
        ubuntu:18.04)            OSNICK="bionic"          ;;
        ubuntu:20.04)            OSNICK="focal"           ;;
        ubuntu:22.04)            OSNICK="jammy"           ;;
        ubuntu:24.04)            OSNICK="noble"           ;;
        ubuntu:25.*|ubuntu:26.*) OSNICK="resolute"        ;;
        debian:11*)              OSNICK="bullseye"        ;;
        debian:12*)              OSNICK="bookworm"        ;;
        debian:13*|debian:trixie) OSNICK="trixie"         ;;
        alpine:*)                OSNICK="alpine"          ;;
        amzn:2)                  OSNICK="amazonlinux2"    ;;
        amzn:2023)               OSNICK="amazonlinux2023" ;;
        rocky:8*)                OSNICK="rocky8"          ;;
        rocky:9*)                OSNICK="rocky9"          ;;
        rocky:10*)               OSNICK="rocky10"         ;;
        almalinux:8*)            OSNICK="alma8"           ;;
        almalinux:9*)            OSNICK="alma9"           ;;
        almalinux:10*)           OSNICK="alma10"          ;;
        mariner:2*)              OSNICK="mariner2"        ;;
        azurelinux:3*)           OSNICK="azurelinux3"     ;;
    esac
fi

if [ -z "$OSNICK" ]; then
    echo "install_script.sh: cannot determine OSNICK (uname=$(uname -s))" >&2
    exit 1
fi

echo "OSNICK=$OSNICK"

# ----------------------------------------------------------------------------
# Detect package manager
# ----------------------------------------------------------------------------

if   [ "$OSNICK" = "macos" ];                  then PM="brew"
elif command -v apt-get >/dev/null 2>&1;       then PM="apt"
elif command -v dnf     >/dev/null 2>&1;       then PM="dnf"
elif command -v tdnf    >/dev/null 2>&1;       then PM="tdnf"
elif command -v yum     >/dev/null 2>&1;       then PM="yum"
elif command -v apk     >/dev/null 2>&1;       then PM="apk"
else
    echo "install_script.sh: no supported package manager found" >&2
    exit 1
fi

echo "PM=$PM"

# ----------------------------------------------------------------------------
# Bootstrap awk
# ----------------------------------------------------------------------------
#
# We use awk to parse dependencies.yaml. Most base images ship one (busybox
# awk on alpine, mawk on debian/ubuntu, gawk on RHEL-likes), but a few
# minimal images (CBL-Mariner 2 base, AzureLinux 3 base) don't, so install
# it here before is_migrated runs. Without this we'd silently fall through
# to the legacy path and confuse later steps.

if ! command -v awk >/dev/null 2>&1; then
    case "$PM" in
        apt)  $MODE apt-get update -qq && $MODE apt-get install -yqq --no-install-recommends gawk ;;
        dnf)  $MODE dnf -y install gawk ;;
        yum)  $MODE yum -y install gawk ;;
        tdnf) $MODE tdnf -y install gawk ;;
        apk)  $MODE apk add --no-cache gawk ;;
        brew) $MODE brew install gawk ;;
    esac
fi

# ----------------------------------------------------------------------------
# Decide between legacy and abstract paths
# ----------------------------------------------------------------------------

# Read `migrated_osnicks` from dependencies.yaml. If the YAML is missing
# (because someone copied this script into a tree without it), or this OSNICK
# isn't in the migrated list, fall back to the legacy <distro>_<ver>.sh flow.

is_migrated() {
    [ -f "$DEPS_YAML" ] || return 1
    awk -v want="$1" '
        /^migrated_osnicks:/ { in_section=1; next }
        in_section {
            # A line that does NOT start with whitespace is a new top-level
            # key, so we leave the section (unless it is a comment).
            if (/^[^ \t]/) {
                if ($0 !~ /^#/) in_section=0
                next
            }
            if (match($0, /^[ \t]*-[ \t]*/)) {
                v = substr($0, RLENGTH + 1)
                sub(/[ \t#].*$/, "", v)
                if (v == want) { found=1; exit }
            }
        }
        END { exit (found ? 0 : 1) }
    ' "$DEPS_YAML"
}

if ! is_migrated "$OSNICK"; then
    # Legacy path: same behaviour install_script.sh has had since forever.
    # Reproduces the lower-cased "<distro_name>_<version>" filename scheme
    # from before this refactor.
    if [ "$(uname -s)" = "Darwin" ]; then
        legacy_os="macos"
    else
        legacy_version=$(grep '^VERSION_ID=' /etc/os-release | sed 's/"//g')
        legacy_version=${legacy_version#"VERSION_ID="}
        legacy_name=$(grep '^NAME=' /etc/os-release | sed 's/"//g')
        legacy_name=${legacy_name#"NAME="}
        # Rocky uses major version only (matches pre-refactor behaviour).
        case "$legacy_name" in "Rocky Linux") legacy_version=${legacy_version%.*} ;; esac
        legacy_os=$(echo "$legacy_name" | tr '[:upper:]' '[:lower:]')_${legacy_version}
        legacy_os=$(echo "$legacy_os" | sed 's/[/ ]/_/g')
    fi
    echo "install_script.sh: using legacy installer .install/${legacy_os}.sh"
    # shellcheck disable=SC1090
    . "$HERE/${legacy_os}.sh" "$MODE"
    git config --global --add safe.directory '*' || true
    exit 0
fi

# ----------------------------------------------------------------------------
# Abstract path
# ----------------------------------------------------------------------------

echo "install_script.sh: installing abstract deps for $OSNICK via $PM"

# Pure-awk YAML extraction. Handles the (limited) shape of dependencies.yaml:
#
#   system:
#     - foo
#     - bar
#   python:
#     - path/req.txt
#   package_map:
#     <abstract>:
#       <pm>: [pkg1, pkg2]
#       <pm>: skip
#       <pm>: []

# extract_flat_list <yaml> <key>  -> prints one entry per line.
extract_flat_list() {
    awk -v key="$1" '
        $0 ~ "^"key":" { in_section=1; next }
        in_section {
            # A line that does NOT start with whitespace is a new top-level
            # key, so we leave the section (unless it is a comment).
            if (/^[^ \t]/) {
                if ($0 !~ /^#/) in_section=0
                next
            }
            if (/^[ \t]*#/)              { next }
            if (match($0, /^[ \t]*-[ \t]*/)) {
                v = substr($0, RLENGTH + 1)
                sub(/[ \t]*#.*$/, "", v)
                gsub(/^[ \t]+|[ \t]+$/, "", v)
                if (v != "") print v
            }
        }
    ' "$2"
}

# resolve_abstract <abstract> <pm> <yaml>  -> prints concrete packages,
# space-separated. Empty if no entry / explicit skip.
resolve_abstract() {
    awk -v abs="$1" -v pm="$2" '
        BEGIN { in_map=0; in_abs=0 }
        /^package_map:/ { in_map=1; next }
        in_map {
            # Top-level key (no leading whitespace) closes the section unless
            # it is a comment.
            if (/^[^ \t]/) {
                if ($0 !~ /^#/) { in_map=0; in_abs=0 }
                next
            }
            # Abstract entry: 2 spaces indent, "<name>:"
            if (match($0, /^  [a-zA-Z0-9_]+:/)) {
                name = $0
                gsub(/^[ \t]+|[ \t:]+$/, "", name)
                in_abs = (name == abs)
                next
            }
            if (in_abs) {
                # Per-PM line: 4 spaces indent, "<pm>: <value>"
                if (match($0, /^    [a-zA-Z0-9_+]+:/)) {
                    line = $0
                    sub(/^    /, "", line)
                    split(line, parts, ":")
                    cur_pm = parts[1]
                    val = substr(line, length(parts[1]) + 2)
                    # Strip trailing "  # comment".
                    sub(/[ \t]+#.*$/, "", val)
                    gsub(/^[ \t]+|[ \t]+$/, "", val)
                    if (cur_pm != pm) next
                    if (val == "skip" || val == "[]" || val == "") { exit }
                    # Strip [ ], split on commas.
                    gsub(/^\[|\]$/, "", val)
                    n = split(val, items, ",")
                    out = ""
                    for (i=1; i<=n; i++) {
                        gsub(/^[ \t]+|[ \t]+$/, "", items[i])
                        if (items[i] != "") out = out (out ? " " : "") items[i]
                    }
                    print out
                    exit
                }
            }
        }
    ' "$3"
}

# Snapshot of every package the host's package database currently considers
# installed, one name per line. Built once up-front so per-package lookups
# are local grep calls rather than ~17 forks of `brew list` / `dpkg-query`
# / `rpm -q` (~10s saved on macOS, ~3s on apt). Empty for tdnf because we
# can't trust the metadata cheaply on minimal Mariner/AzureLinux images;
# tdnf already handles "already installed" gracefully on its own.
INSTALLED_PKGS_FILE="$(mktemp)"
trap 'rm -f "$INSTALLED_PKGS_FILE"' EXIT
case "$PM" in
    apt)         dpkg-query -W -f='${Package}\n'      2>/dev/null > "$INSTALLED_PKGS_FILE" || true ;;
    dnf|yum)     rpm -qa --queryformat '%{NAME}\n'    2>/dev/null > "$INSTALLED_PKGS_FILE" || true ;;
    apk)         apk info                             2>/dev/null > "$INSTALLED_PKGS_FILE" || true ;;
    brew)        brew list --formula -1               2>/dev/null > "$INSTALLED_PKGS_FILE" || true ;;
esac

# is_pkg_installed <pkg>  -> 0 if already present, non-zero otherwise.
is_pkg_installed() {
    grep -Fxq "$1" "$INSTALLED_PKGS_FILE"
}

# Resolve every abstract dep -> concrete list -> drop already-installed.
all_pkgs=""
missing_pkgs=""
while IFS= read -r abs; do
    [ -z "$abs" ] && continue
    pkgs=$(resolve_abstract "$abs" "$PM" "$DEPS_YAML")
    [ -z "$pkgs" ] && continue
    for p in $pkgs; do
        all_pkgs="$all_pkgs $p"
        # tdnf path skips probing — it short-circuits already-installed
        # packages itself (and Mariner's rpm db sometimes isn't populated
        # until after tdnf makecache).
        if [ "$PM" = "tdnf" ] || ! is_pkg_installed "$p"; then
            missing_pkgs="$missing_pkgs $p"
        fi
    done
done <<EOF_DEPS
$(extract_flat_list system "$DEPS_YAML")
EOF_DEPS

if [ -z "${missing_pkgs# }" ]; then
    echo "install_script.sh: all $(echo $all_pkgs | wc -w | tr -d ' ') deps already installed; nothing to do"
else
    echo "install_script.sh: installing missing:$missing_pkgs"

    # Refresh package-manager metadata only now that we know we'll install.
    case "$PM" in
        apt)
            export DEBIAN_FRONTEND=noninteractive
            $MODE apt-get update -qq
            ;;
        dnf|yum|tdnf)
            $MODE $PM -y makecache 2>/dev/null || true
            ;;
        apk)
            $MODE apk update -q
            ;;
    esac

    case "$PM" in
        apt)  $MODE apt-get install -yqq --no-install-recommends $missing_pkgs ;;
        dnf)  $MODE dnf -y install --allowerasing $missing_pkgs ;;
        yum)  $MODE yum -y install $missing_pkgs ;;
        apk)  $MODE apk add --no-cache $missing_pkgs ;;
        brew) $MODE brew install $missing_pkgs ;;
        tdnf)
            # tdnf has no --skip-broken and aborts the whole transaction if
            # any one package is missing from the repos (CBL-Mariner /
            # AzureLinux ship a small subset compared to apt/dnf). Install
            # one at a time so an individual missing package is just a
            # warning, not a build failure.
            for pkg in $missing_pkgs; do
                if ! $MODE tdnf -y install "$pkg" >/dev/null 2>&1; then
                    echo "  (tdnf: skipped unavailable package '$pkg')"
                fi
            done
            ;;
    esac
fi

# OS-specific extras that don't fit the abstract->concrete mapping
# (e.g. AmazonLinux2 needs openssl11 + devtoolset, Mariner2 needs awscli, ...).
QUIRK="$HERE/quirks/${OSNICK}.sh"
if [ -f "$QUIRK" ]; then
    echo "install_script.sh: running quirk $QUIRK"
    # shellcheck disable=SC1090
    . "$QUIRK" "$MODE"
fi

git config --global --add safe.directory '*' || true

# Note: Python requirement files are listed under `python:` in
# dependencies.yaml but are installed by `make setup` (via
# .install/common_installations.sh) inside its venv, not from here. This keeps
# install_script.sh focused on system packages and lets Docker images that
# manage their own Python environment (uv, venv) skip the duplicate work.

echo "install_script.sh: done"
