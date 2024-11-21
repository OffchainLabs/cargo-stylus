CARGO_STYLUS_VERSION := $(shell cargo pkgid --manifest-path main/Cargo.toml | cut -d '@' -f 2)

.PHONY: build
build:
	cargo build

.PHONY: test
test:
	cargo test

.PHONY: bench
bench:
	cargo +nightly bench -F nightly

.PHONY: fmt
fmt:
	cargo fmt

.PHONY: lint
lint:
	cargo clippy --package cargo-stylus --package cargo-stylus-example

.PHONY: install
install: fmt lint
	cargo install --path main

.PHONY: docker
docker:
	docker build -t cargo-stylus-base:$(CARGO_STYLUS_VERSION) --build-arg CARGO_STYLUS_VERSION=$(CARGO_STYLUS_VERSION) .
