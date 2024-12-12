#!/bin/sh -e

cargo fmt
cargo clippy --package cargo-stylus --package cargo-stylus-example
cargo install --path main
