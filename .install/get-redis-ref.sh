#!/usr/bin/env bash
# Print the Redis git ref that redisjson is built/tested against.
#
# The ref is derived from the `compatible_redis_version` field of the RAMP
# manifest (pack/ramp.yml), so the version lives in a single place and is not
# duplicated across Dockerfiles and CI workflows:
#   * the dev/unreleased placeholder 99.99 (or 99.99.99) -> "unstable" branch
#   * any real version (e.g. "8.8") is used as the git ref verbatim
# The value may be quoted or unquoted; an inline '#' comment, if any, is stripped.
#
# It lives under .install/ (not sbin/) so it is copied into the Docker build
# context together with the rest of .install/, letting install_redis.sh reuse
# the very same reader instead of re-implementing the parse.
#
# Usage:
#   .install/get-redis-ref.sh        # prints the ref, e.g. "unstable" or "8.8"
#
# In a GitHub Actions step:
#   echo "redis-ref=$(.install/get-redis-ref.sh)" >> "$GITHUB_OUTPUT"

set -euo pipefail

PROGNAME="${BASH_SOURCE[0]}"
HERE="$(cd "$(dirname "$PROGNAME")" &>/dev/null && pwd)"
ROOT="$(cd "$HERE/.." &>/dev/null && pwd)"
RAMP_FILE="$ROOT/pack/ramp.yml"

if [[ ! -f "$RAMP_FILE" ]]; then
	echo "Error: RAMP manifest not found at $RAMP_FILE" >&2
	exit 1
fi

# Value of the top-level `compatible_redis_version:` key, with inline comment,
# surrounding whitespace and quotes stripped.
COMPAT_VERSION="$(sed -nE 's/^compatible_redis_version:[[:space:]]*(.*)$/\1/p' "$RAMP_FILE" | head -n1 \
	| sed -E 's/[[:space:]]*#.*$//; s/^[[:space:]]+//; s/[[:space:]]+$//; s/^"(.*)"$/\1/; s/^'\''(.*)'\''$/\1/')"

if [[ -z "$COMPAT_VERSION" ]]; then
	echo "Error: 'compatible_redis_version' is not defined in $RAMP_FILE" >&2
	exit 1
fi

# The dev/unreleased placeholder (99.99 or 99.99.99) means we track the
# 'unstable' branch; a real version is used as the git ref directly.
case "$COMPAT_VERSION" in
	99.99 | 99.99.99) REDIS_REF="unstable" ;;
	*)                REDIS_REF="$COMPAT_VERSION" ;;
esac

echo "$REDIS_REF"
