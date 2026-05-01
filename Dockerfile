FROM rust:1.95-bookworm AS builder

WORKDIR /build
COPY . .

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/build/target \
    cargo build --release --bin app && \
    strip /build/target/release/app && \
    cp /build/target/release/app /build/app-bin

FROM debian:bookworm-slim AS runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/* && \
    useradd --system --uid 10001 --home-dir /app --shell /usr/sbin/nologin summer

WORKDIR /app

COPY --from=builder --chown=summer:summer /build/app-bin /app/app
COPY --chown=summer:summer config/ /app/config/
COPY --chown=summer:summer data/ip2region_v4.xdb /app/data/ip2region_v4.xdb

ENV SUMMER_ENV=prod \
    RUST_LOG=info \
    RUST_BACKTRACE=1

USER summer
EXPOSE 8080

ENTRYPOINT ["/app/app"]
