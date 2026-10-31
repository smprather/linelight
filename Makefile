PREFIX ?= $(HOME)/.local
CARGO ?= cargo

.PHONY: all build test check install clean

all: build

build:
	$(CARGO) build --release

test:
	$(CARGO) test

check:
	$(CARGO) fmt -- --check
	$(CARGO) clippy --all-targets -- -D warnings
	$(CARGO) test
	$(CARGO) build --release
	./target/release/linelight check
	./tests/completion.sh

install: build
	PREFIX=$(PREFIX) ./install.sh

clean:
	$(CARGO) clean

deploy-test: build
	./tests/deploy.sh

completion-test: build
	./tests/completion.sh
