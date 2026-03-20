FROM rust:alpine AS builder

WORKDIR /app

# Install build dependencies
RUN apk add --no-cache musl-dev openssl-dev

# Copy manifests
COPY backend/Cargo.toml backend/Cargo.lock ./

# Copy source
COPY backend/crates ./crates/
COPY backend/server ./server/
COPY backend/migrations ./migrations/

# Build
RUN cargo build --release --bin rustshare-server

# Runtime image
FROM alpine:3.19

RUN apk add --no-cache libgcc openssl ca-certificates

COPY --from=builder /app/target/release/rustshare-server /usr/local/bin/

CMD ["rustshare-server"]
