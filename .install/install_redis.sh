#!/bin/bash
set -e

if [ -z "${REDIS_REF}" ]; then
    echo "Error: REDIS_REF environment variable is required"
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
