FROM --platform=linux/amd64 rust:1.80 as builder
RUN apt-get update && apt-get install -y git
RUN rustup target add x86_64-unknown-linux-gnu
RUN git clone https://github.com/offchainlabs/cargo-stylus.git
WORKDIR /cargo-stylus
RUN git checkout v0.5.2
RUN cargo build --release --manifest-path main/Cargo.toml
FROM --platform=linux/amd64 rust:1.80
COPY --from=builder /cargo-stylus/target/release/cargo-stylus /usr/local/bin/cargo-stylus