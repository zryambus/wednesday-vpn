FROM builder as builder
FROM alpine:latest

RUN apk add \
    wireguard-tools \
    libqrencode

COPY --from=builder /tmp/wg/target/release/telegram_bot /opt/telegram_bot
COPY config.yaml /opt/config.yaml

WORKDIR /opt
ENTRYPOINT ["/opt/telegram_bot"]