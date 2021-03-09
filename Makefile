
MODULE_NAME=rejson.so

ifeq ($(DEBUG),1)
TARGET_DIR=target/debug
else
CARGO_FLAGS += --release
TARGET_DIR=target/release
endif

TARGET=$(TARGET_DIR)/$(MODULE_NAME)

#----------------------------------------------------------------------------------------------

all: build

#----------------------------------------------------------------------------------------------

lint:
	cargo fmt -- --check

#----------------------------------------------------------------------------------------------

build:
	cargo build --all --all-targets $(CARGO_FLAGS)
	cp $(TARGET_DIR)/librejson.so $(TARGET)

clean:
ifneq ($(ALL),1)
	cargo clean
else
	rm -rf target
endif

#----------------------------------------------------------------------------------------------

test: pytest

pytest:
	MODULE=$(abspath $(TARGET)) ./tests/pytest/tests.sh

cargo_test:
	cargo test --features test --all

.PHONY: pytest cargo_test

#----------------------------------------------------------------------------------------------

package:
	$(MAKE) -C ./src package

.PHONY: package

#----------------------------------------------------------------------------------------------

docker:
	docker build --pull -t rejson:latest .

docker_push:
	docker push redislabs/rejson:latest

.PHONY: docker docker_push

#----------------------------------------------------------------------------------------------

builddocs:
	mkdocs build

localdocs: builddocs
	mkdocs serve

deploydocs: builddocs
	mkdocs gh-deploy

.PHONY: builddocs localdocs deploydocs
