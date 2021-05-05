
ROOT=../..
MK.pyver:=3
include $(ROOT)/deps/readies/mk/main

# OSNICK=buster|stretch|bionic|xenial|centos7
OSNICK ?= buster

REPO=rejson

REDIS_VER=6.2.1

INT_BRANCHES=1.2 1.1 1.0

ART_DIR=$(ROOT)/bin/artifacts
ART_INT_DIR=/var/opt/redislabs/artifacts

include $(MK)/docker.native
