
ROOT=../..
MK.pyver:=3
include $(ROOT)/deps/readies/mk/main

REDIS_VERSION=6.2.5
PRODUCT=rejson

OSNICK ?= bullseye
OSNICK_OFFICIAL=bullseye

# if there's a need to manually set the version, override.
# by default this uses ${VERSION} but in a private branch testing with
# VERSION=master, VERSION=2.0 is requireed.
export REDISEARCH_VERSION=$(VERSION)
export ART_INT_DIR=/usr/lib/redis/modules/

include $(MK)/docker.rules
