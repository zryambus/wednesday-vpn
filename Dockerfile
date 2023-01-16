FROM rust:alpine as builder

RUN apk add musl-dev protoc

WORKDIR /tmp/wg
COPY Cargo.toml /tmp/wg/Cargo.toml
COPY src/lib.rs /tmp/wg/src/lib.rs
RUN cargo build --release --lib

COPY src/ /tmp/wg/src/
COPY proto/ /tmp/wg/proto/
COPY build.rs /tmp/wg/
RUN cargo build --release 
