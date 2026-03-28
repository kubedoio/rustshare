# RustShare Backend - Cached Build
# 
# This Dockerfile assumes frontend is pre-built and mounted at runtime.
# It only builds the Rust backend.

FROM rust:alpine AS builder

WORKDIR /app

# Install build dependencies
RUN apk add --no-cache musl-dev openssl-dev

# Copy manifests first for better layer caching
COPY backend/Cargo.toml backend/Cargo.lock ./
COPY backend/crates ./crates/
COPY backend/server ./server/
COPY backend/migrations ./migrations/

# Build with cache mount for faster rebuilds
RUN --mount=type=cache,target=/app/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release --bin rustshare-server && \
    cp /app/target/release/rustshare-server /tmp/rustshare-server

# Copy the binary to a known location
FROM scratch AS binaries
COPY --from=builder /tmp/rustshare-server /rustshare-server

# Runtime image
FROM alpine:3.19

RUN apk add --no-cache libgcc openssl ca-certificates wget

COPY --from=binaries /rustshare-server /usr/local/bin/rustshare-server
COPY docker/wait-for-rustfs.sh /usr/local/bin/wait-for-rustfs.sh
RUN chmod +x /usr/local/bin/wait-for-rustfs.sh

# Frontend will be mounted at runtime
ENV FRONTEND_DIST_DIR=/app/frontend-build

ENTRYPOINT ["/usr/local/bin/wait-for-rustfs.sh"]
CMD ["rustshare-server"]
