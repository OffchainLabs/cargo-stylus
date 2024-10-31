ARG BUILD_PLATFORM=linux/amd64
ARG RUST_VERSION=1.80
ARG CARGO_STYLUS_VERSION

FROM --platform=${BUILD_PLATFORM} rust:${RUST_VERSION} AS builder
RUN apt-get update && apt-get install -y git
RUN rustup target add x86_64-unknown-linux-gnu
ARG CARGO_STYLUS_VERSION
RUN test -n "$CARGO_STYLUS_VERSION"
RUN git clone --branch v$CARGO_STYLUS_VERSION https://github.com/offchainlabs/cargo-stylus.git
WORKDIR /cargo-stylus
RUN cargo build --release --manifest-path main/Cargo.toml

FROM --platform=${BUILD_PLATFORM} rust:${RUST_VERSION} AS cargo-stylus-base
COPY --from=builder /cargo-stylus/target/release/cargo-stylus /usr/local/bin/cargo-stylus
