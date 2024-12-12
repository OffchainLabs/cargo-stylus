ARG BUILD_PLATFORM=linux/amd64
ARG RUST_VERSION=1.81
FROM --platform=${BUILD_PLATFORM} rust:${RUST_VERSION} AS builder

RUN rustup target add x86_64-unknown-linux-gnu

# Copy the entire workspace
COPY . /cargo-stylus/
WORKDIR /cargo-stylus

# Build the project using the workspace member
RUN cargo build --release --manifest-path main/Cargo.toml

FROM --platform=${BUILD_PLATFORM} rust:${RUST_VERSION} AS cargo-stylus-base
COPY --from=builder /cargo-stylus/target/release/cargo-stylus /usr/local/bin/cargo-stylus
