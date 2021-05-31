all:
	$(MAKE) -C ./src all

test:
	$(MAKE) -C ./test all
.PHONY: test

benchmark:
	$(MAKE) -C ./src benchmark
.PHONY: benchmark

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

