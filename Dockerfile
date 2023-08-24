FROM rust:1.71 as builder
COPY . .
RUN cargo build --release

FROM debian:buster-slim
COPY --from=builder ./target/release/cargo-stylus ./target/release/cargo-stylus
CMD ["/target/release/cargo-stylus"]