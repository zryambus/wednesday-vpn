FROM rust:alpine as builder

RUN apk add musl-dev protoc

WORKDIR /tmp/wg
COPY Cargo.toml /tmp/wg/Cargo.toml
COPY src/lib.rs /tmp/wg/src/lib.rs
RUN cargo build --release --lib

COPY src/ /tmp/wg/src/
COPY proto/ /tmp/wg/proto/
COPY migrations /tmp/wg/migrations
COPY build.rs /tmp/wg/
COPY sqlx-data.json /tmp/wg/sqlx-data.json
RUN cargo build --release 
