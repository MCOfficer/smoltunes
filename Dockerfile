FROM rust:alpine AS builder

RUN cargo new --bin smoltunes
WORKDIR /smoltunes

COPY Cargo.toml Cargo.lock ./
RUN apk add musl-dev
RUN cargo build --release --locked
RUN rm src/*.rs

COPY . .
RUN touch src/main.rs
RUN cargo build --release

FROM alpine:latest

COPY --from=builder /smoltunes/target/release/smoltunes /smoltunes

# Suppress error backtraces (required for poise_error, or the embed are too large for discord)
ENV RUST_LIB_BACKTRACE=0
# ... but enable them for panics
ENV RUST_BACKTRACE=1

USER 1000
ENTRYPOINT ["/smoltunes"]