FROM rust:alpine AS builder


RUN apk add --no-cache musl-dev gcc

WORKDIR /app


COPY Cargo.toml ./
COPY src ./src
COPY static ./static


RUN cargo build --release


FROM alpine:latest
WORKDIR /app

RUN apk add --no-cache iproute2 iw wpa_supplicant \
    && (apk add --no-cache raspberrypi-utils-vcgencmd || true)

COPY --from=builder /app/target/release/pi-monitor .
COPY static ./static


EXPOSE 5000

CMD ["./pi-monitor"]
