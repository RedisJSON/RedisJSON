BUILD = cargo build --all --all-targets
BUILD_RELEASE = ${BUILD}
ifndef DEBUG
	BUILD_RELEASE += --release
endif

all:
	$(BUILD_RELEASE)

test: build_debug 
	python test/pytest/test.py
.PHONY: test

build_debug:
	$(BUILD)
	cp ./target/debug/librejson.so ./target/debug/rejson.so

docker:
	docker pull ubuntu:latest
	docker pull ubuntu:xenial
	docker build . -t rejson:latest
.PHONY: docker

docker_dist:
	docker build --rm -f Dockerfile_Dist . -t redislabs/rejson

docker_push: docker_dist
	docker push redislabs/rejson:latest

package:
	$(MAKE) -C ./src package

builddocs:
	mkdocs build

localdocs: builddocs
	mkdocs serve

deploydocs: builddocs
	mkdocs gh-deploy

clean:
	find ./ -name "*.[oa]" -exec rm {} \; -print
	find ./ -name "*.so" -exec rm {} \; -print
	find ./ -name "*.out" -exec rm {} \; -print
	rm -rf ./build

