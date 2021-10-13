REDIS_VERSION=6.2.5
PRODUCT=rejson

OSNICK ?= bullseye
OSNICK_OFFICIAL=bullseye

# if there's a need to manually set the version, override.
# by default this uses ${VERSION} but in a private branch testing with
# VERSION=master, VERSION=2.0 is requireed.
REDISEARCH_VERSION=${VERSION}
export REDISEARCH_VERSION

ROOT=../../
READIES=${ROOT}/deps/readies
include ${READIES}/mk/docker.rules
