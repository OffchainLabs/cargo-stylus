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

cargo test

if [ "$(uname -s)" != "Darwin" ]; then
    # The MacOS CI doesn't support Docker because of licensing issues, so only run them on Linux.
    # Also, run the docker tests on a single thread to avoid concurrency issues.
    cargo test -F docker-test -- --test-threads 1 docker
fi
