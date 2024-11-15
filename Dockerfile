ARG TARGETPLATFORM=linux/amd64
FROM rust:1.80 AS builder
RUN apt-get update && apt-get install -y git
RUN rustup target add x86_64-unknown-linux-gnu
RUN git clone --branch v0.5.6 https://github.com/offchainlabs/cargo-stylus.git
WORKDIR /cargo-stylus
RUN cargo build --release --manifest-path main/Cargo.toml

FROM rust:1.80
COPY --from=builder /cargo-stylus/target/release/cargo-stylus /usr/local/bin/cargo-stylus
