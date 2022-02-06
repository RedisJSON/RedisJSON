
ROOT=../..
MK.pyver:=3
include $(ROOT)/deps/readies/mk/main

REPO=rejson

REDIS_VERSION=6.2.6

OSNICK.official=bullseye

INT_BRANCHES=2.0 1.2 1.0
LATEST_BRANCH=2.0
PREVIEW_BRANCH=2.0

ART_DIR=$(ROOT)/bin/artifacts

REDISEARCH_VERSION=master
DOCKER_VARS += REDISEARCH_VERSION

include $(MK)/module.docker.rules
