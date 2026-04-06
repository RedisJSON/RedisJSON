#!/usr/bin/env bash
# Build and smoke-test all Redis 8.8 CI Docker images (local validation).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
REDIS_REF="${REDIS_REF:-7.4.2}"
TAG_PREFIX="${TAG_PREFIX:-redisjson-ci-verify}"
FAILED=()

images=(
  jammy noble resolute
  rocky8 rocky9 rocky10
  alma8 alma9 alma10
  bookworm trixie alpine
)

run_in_container() {
  local name="$1"
  docker run --rm \
    -v "${ROOT}:/workspace" \
    -w /workspace \
    "${TAG_PREFIX}:${name}" \
    bash -lc '
      set -e
      export PATH="/root/.cargo/bin:/usr/local/bin:${PATH}"
      for f in /etc/profile.d/gcc-toolset-11.sh /etc/profile.d/gcc-toolset-13.sh; do
        [ -r "$f" ] && . "$f" || true
      done
      cargo --version
      redis-server --version
      cargo test -p json_path -q
    '
}

for img in "${images[@]}"; do
  df="Dockerfile.${img}"
  if [[ ! -f "$df" ]]; then
    echo "SKIP $img (missing $df)"
    continue
  fi
  echo "========== BUILD $img =========="
  if ! docker build -f "$df" -t "${TAG_PREFIX}:${img}" --build-arg "REDIS_REF=${REDIS_REF}" . ; then
    echo "BUILD FAILED: $img"
    FAILED+=("$img:build")
    continue
  fi
  echo "========== TEST $img =========="
  if ! run_in_container "$img"; then
    echo "TEST FAILED: $img"
    FAILED+=("$img:test")
  else
    echo "OK $img"
  fi
done

if ((${#FAILED[@]})); then
  echo "Failures: ${FAILED[*]}"
  exit 1
fi
echo "All images built and json_path tests passed."
