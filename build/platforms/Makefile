
ROOT=../..
MK.pyver:=3
include $(ROOT)/deps/readies/mk/main

DOCKERFILE_STEM=$(ROOT)/build/platforms/Dockerfile

# OSNICK=bullseye|bionic|xenial|centos7|centos8
OSNICK ?= bullseye

REPO=rejson

REDIS_VER=6.2.5
REDISEARCH_VER=master

INT_BRANCHES=2.0 1.2 1.1 1.0

ART_DIR=$(ROOT)/bin/artifacts
ART_INT_DIR=/var/opt/redislabs/artifacts

include $(MK)/docker.native
