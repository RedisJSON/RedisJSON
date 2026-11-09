#!/usr/bin/env bash
# Preserve the workflow's Docker arguments while adding isolated canaries.
set -euo pipefail
mode=${1:?Expected run, build, or buildx}
shift
case "$mode" in
    run|build) command=(docker "$mode") ;;
    buildx) command=(docker buildx build) ;;
    *) echo "Unsupported Docker mode: $mode" >&2; exit 2 ;;
esac
trap 'if [[ -n "${staging:-}" ]]; then rm -rf "$staging"; fi' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
staging=$(python3 "$(dirname "${BASH_SOURCE[0]}")/tracebit-docker.py" "$mode" "$@")
arguments=()
while IFS= read -r -d '' argument; do
    arguments+=("$argument")
done < "$staging/arguments"
# This is a Docker CLI wrapper, not a shell-command interpreter. Every argument
# remains a separate array item; metacharacters are never evaluated.
"${command[@]}" "${arguments[@]}"
