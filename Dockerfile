
#---------------------------------------------------------------------------------------------- 
FROM rust:latest as builder

ENV LIBDIR /usr/lib/redis/modules

ADD . /build
WORKDIR /build

RUN ./deps/readies/bin/getpy3
RUN ./system-setup.py

RUN set -ex ;\
    cargo build --release ;\
    mv target/release/librejson.so target/release/rejson.so

#---------------------------------------------------------------------------------------------- 
FROM redis:latest

ENV LIBDIR /usr/lib/redis/modules
WORKDIR /data

RUN mkdir -p "$LIBDIR"
COPY --from=builder /build/target/release/rejson.so "$LIBDIR"

CMD ["redis-server", "--loadmodule", "/usr/lib/redis/modules/rejson.so"]
