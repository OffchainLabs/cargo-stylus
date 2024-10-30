.PHONY: build
build:
	cargo build

.PHONY: test
test:
	cargo test

.PHONY: fmt
fmt:
	cargo fmt

.PHONY: lint
lint:
	cargo clippy --package cargo-stylus --package cargo-stylus-example

.PHONY: install
install: fmt lint
	cargo install --path main
