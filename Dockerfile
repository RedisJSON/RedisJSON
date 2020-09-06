FROM rust:latest as builder

ENV LIBDIR /usr/lib/redis/modules

ADD . /REJSON
WORKDIR /REJSON

# Set up a build environment
RUN set -ex ;\
	mkdir -p deps ;\
	cd deps ;\
	git clone https://github.com/RedisLabsModules/readies.git
RUN PIP=1 FORCE=1 ./deps/readies/bin/getpy2
RUN ./system-setup.py

# Build the source
RUN set -ex ;\
    cargo build --release ;\
    mv target/release/librejson.so target/release/rejson.so

#---------------------------------------------------------------------------------------------- 
# Package the runner
FROM redis:latest

ENV LIBDIR /usr/lib/redis/modules
WORKDIR /data
RUN mkdir -p "$LIBDIR"
COPY --from=builder /REJSON/target/release/rejson.so "$LIBDIR"

CMD ["redis-server", "--loadmodule", "/usr/lib/redis/modules/rejson.so"]
