# BUILD redisfab/rejson:${VERSION}-${ARCH}-${OSNICK}

ARG REDIS_VER=6.2.2

# stretch|bionic|buster
ARG OSNICK=buster

# ARCH=x64|arm64v8|arm32v7
ARG ARCH=x64

ARG PACK=0
ARG TEST=0

#----------------------------------------------------------------------------------------------
FROM redisfab/redis:${REDIS_VER}-${ARCH}-${OSNICK} AS builder

ARG OSNICK
ARG OS
ARG ARCH
ARG REDIS_VER
ARG PACK
ARG TEST

RUN echo "Building for ${OSNICK} (${OS}) for ${ARCH} [with Redis ${REDIS_VER}]"

ADD ./ /build
WORKDIR /build

RUN ./deps/readies/bin/getpy3
RUN ./system-setup.py
RUN set -ex ;\
    if [ -e /usr/bin/apt-get ]; then \
        apt-get update -qq; \
        apt-get upgrade -yqq; \
        rm -rf /var/cache/apt; \
    fi
RUN if [ -e /usr/bin/yum ]; then \
        yum update -y; \
        rm -rf /var/cache/yum; \
    fi

RUN bash -l -c make

RUN set -ex ;\
    if [ "$TEST" = "1" ]; then bash -l -c "TEST= make test"; fi
RUN set -ex ;\
    mkdir -p bin/artifacts ;\
    if [ "$PACK" = "1" ]; then bash -l -c "make pack"; fi

#----------------------------------------------------------------------------------------------
FROM redisfab/redis:${REDIS_VER}-${ARCH}-${OSNICK}

ARG REDIS_VER

ENV LIBDIR /usr/lib/redis/modules
WORKDIR /data
RUN mkdir -p "$LIBDIR"

COPY --from=builder /build/bin/artifacts/ /var/opt/redislabs/artifacts
COPY --from=builder /build/target/release/rejson.so "$LIBDIR"

EXPOSE 6379
CMD ["redis-server", "--loadmodule", "/usr/lib/redis/modules/rejson.so"]
