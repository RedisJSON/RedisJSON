FROM ubuntu:xenial
LABEL Description="This image is used to Redis with ReJSON under valgrind" Vendor="Redis Labs" Version="1.0"

RUN apt-get -y update && \
    apt-get -y upgrade && \
    apt-get -y install \
        apt-utils \
        build-essential \
        wget \
        zip \
        valgrind

RUN mkdir /build
WORKDIR /build

RUN wget https://github.com/antirez/redis/archive/unstable.zip && \
    unzip unstable.zip && \
    rm unstable.zip && \
    mv redis-unstable redis

WORKDIR /build/redis
RUN make distclean
RUN make valgrind

WORKDIR /build/rejson
COPY ./deps deps/
COPY ./src src/
COPY ./test test/
COPY ./Makefile ./
ENV DEBUG 1
# RUN make all

EXPOSE 6379
WORKDIR /build
CMD ["bash"]
# CMD ["valgrind", "--tool=memcheck", "--leak-check=full", "--track-origins=yes", "--show-reachable=no", "--show-possibly-lost=no", "--suppressions=redis/src/valgrind.sup", "redis/src/redis-server", "--protected-mode", "no", "--loadmodule", "/build/rejson/src/rejson.so"]