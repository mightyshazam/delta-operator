FROM rust:1.71-bookworm as builder
ARG VERSION
WORKDIR /build
RUN if [[ "$TARGETARCH" = "arm" ]] ; then echo -n "aarch64" > data  ; else echo -n "x86_64" > data ; fi
RUN wget https://github.com/mightyshazam/delta-operator/releases/download/${VERSION}/delta-operator-`cat data`-unknown-linux-musl.tar.gz \
    && tar -xvf delta-operator-`cat data`-unknown-linux-musl.tar.gz

FROM debian:bookworm-slim
WORKDIR /app
RUN apt update \
    && apt install -y openssl ca-certificates \
    && apt clean \
    && rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*

COPY --from=builder /build/delta-operator-worker .

ENTRYPOINT ["/app/delta-operator-worker"]