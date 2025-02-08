FROM rust:alpine as builder

RUN cargo new --bin smoltunes
WORKDIR /smoltunes

COPY Cargo.toml Cargo.lock ./
RUN apk add musl-dev
RUN cargo build --release --locked
RUN rm src/*.rs

COPY . .
RUN cargo build --release --locked

FROM alpine:latest

COPY --from=builder /smoltunes/target/release/smoltunes /smoltunes

USER 1000
ENTRYPOINT ["/smoltunes"]