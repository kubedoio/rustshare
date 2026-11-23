FROM rust:1.97.1-bookworm AS builder

WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY backend ./backend/
COPY apps ./apps/
COPY crates ./crates/
COPY config/buzz-compatibility.env ./config/buzz-compatibility.env
COPY .sqlx ./.sqlx/

ENV SQLX_OFFLINE=true \
    CARGO_NET_RETRY=10

RUN cargo build --release --locked -p rustshare-server --bin elembra-memory-worker

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/elembra-memory-worker /usr/local/bin/elembra-memory-worker

USER 65532:65532
EXPOSE 8081
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD ["curl", "--fail", "--silent", "--show-error", "--max-time", "3", "http://127.0.0.1:8081/ready"]
ENTRYPOINT ["/usr/local/bin/elembra-memory-worker"]
