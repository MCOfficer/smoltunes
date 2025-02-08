FROM rust:alpine AS builder

RUN cargo new --bin smoltunes
WORKDIR /smoltunes

COPY Cargo.toml Cargo.lock ./
RUN apk add musl-dev
RUN cargo build --release
RUN rm src/*.rs

COPY . .
RUN touch src/main.rs
RUN cargo build --release

FROM alpine:latest

COPY --from=builder /smoltunes/target/release/smoltunes /smoltunes

USER 1000
ENTRYPOINT ["/smoltunes"]