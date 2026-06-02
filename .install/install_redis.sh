#!/bin/bash
set -e

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)"

# If REDIS_REF was not provided explicitly, derive it from the manifest's
# compatible_redis_version (pack/ramp.yml) via the shared reader that ships in
# this same directory (.install/get-redis-ref.sh). Both this script and
# pack/ramp.yml are copied into the Docker build context, so the reader works
# here exactly as it does in CI.
if [ -z "${REDIS_REF}" ]; then
    REDIS_REF="$(bash "$HERE/get-redis-ref.sh")"
fi

if [ -z "${REDIS_REF}" ]; then
    echo "Error: REDIS_REF is not set and could not be derived from pack/ramp.yml"
    exit 1
fi

echo "Installing Redis from ref: ${REDIS_REF}"

# SANITIZER can be passed to build Redis with sanitizer support (e.g., SANITIZER=address)
if [ -n "${SANITIZER}" ]; then
    echo "Building Redis with SANITIZER=${SANITIZER}"
fi

git clone https://github.com/redis/redis.git 
cd redis
git fetch origin ${REDIS_REF}
git checkout ${REDIS_REF}
git submodule update --init --recursive
make SANITIZER=${SANITIZER:-} -j$(nproc)
make install
cd ..

echo "Redis installed successfully"
redis-server --version
