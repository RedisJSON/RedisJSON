#!/bin/bash

REDIS_VERSION=7.2.1

curdir="$PWD"
cd /tmp/
# Download redis source from github archive and extract it.
wget https://github.com/redis/redis/archive/${REDIS_VERSION}.tar.gz
tar -xvzf ${REDIS_VERSION}.tar.gz && cd ./redis-${REDIS_VERSION}

#patch
sed -ri 's/(createEnum.*enable-protected-configs.*PROTECTED_ACTION_ALLOWED)_NO/\1_YES/g' ./src/config.c
sed -ri 's/(createEnum.*enable-debug-command.*PROTECTED_ACTION_ALLOWED)_NO/\1_YES/g' ./src/config.c
sed -ri 's/(createEnum.*enable-module-command.*PROTECTED_ACTION_ALLOWED)_NO/\1_YES/g' ./src/config.c

# Build redis from source and install it.
make && make install
cd $curdir
