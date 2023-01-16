FROM builder as builder
FROM alpine:latest

RUN apk add \
    wireguard-tools

COPY run.bash /opt/run.bash
COPY --from=builder /tmp/wg/target/release/wireguard_control /opt/wireguard_control
COPY config.yaml /opt/config.yaml

RUN echo "net.ipv4.ip_forward=1" >> /etc/sysctl.conf

EXPOSE 51820
EXPOSE 8080

WORKDIR /opt
ENTRYPOINT ["/opt/wireguard_control"]