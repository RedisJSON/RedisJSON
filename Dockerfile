FROM rust:latest as builder

ENV LIBDIR /usr/lib/redis/modules
ENV DEPS "python python-setuptools python-pip wget unzip build-essential clang-6.0 cmake"

# Set up a build environment
RUN set -ex;\
    deps="$DEPS";\
    apt-get update; \
    apt-get install -y --no-install-recommends $deps;\
    pip install rmtest

# Build the source
ADD . /REJSON
WORKDIR /REJSON
RUN set -ex;\
    cargo build --release;\
    mv target/release/libredisjson.so target/release/rejson.so

# Package the runner
FROM redis:latest
ENV LIBDIR /usr/lib/redis/modules
WORKDIR /data
RUN set -ex;\
    mkdir -p "$LIBDIR";
COPY --from=builder /REJSON/target/release/rejson.so "$LIBDIR"

CMD ["redis-server", "--loadmodule", "/usr/lib/redis/modules/rejson.so"]
