#!/bin/bash

set -euo pipefail

export RUSTFLAGS="-D warnings"
export RUSTFMT_CI=1

# Print version information
rustc -Vv
cargo -V

# Build and test main crate
if [ "$CFG_RELEASE_CHANNEL" == "nightly" ]; then
    cargo build --locked --all-features
else
    cargo build --locked
fi

UNAME=$(uname -s)
if [ "$UNAME" == "Darwin" ]; then
    # Disable docker tests on MacOS CI
    cargo test --no-default-features
else
    cargo test
fi
