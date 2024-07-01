cargo fmt
cargo clippy --package cargo-stylus --package cargo-stylus-cgen --package cargo-stylus-check
cargo install --path main
cargo install --path cgen
cargo install --path check