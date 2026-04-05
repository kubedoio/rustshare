# =============================================================================
# Stage 1: Frontend Builder
# =============================================================================
FROM node:20-bookworm-slim AS frontend-builder

WORKDIR /app/frontend

ARG VITE_API_URL=/api/v1
ARG VITE_WS_URL=/api/ws
ENV VITE_API_URL=$VITE_API_URL
ENV VITE_WS_URL=$VITE_WS_URL

# Copy package files first for better layer caching
COPY frontend/package*.json ./

# Install dependencies (cached if package.json hasn't changed)
RUN npm install --legacy-peer-deps \
    && ARCH="$(dpkg --print-architecture)" \
    && case "$ARCH" in \
        amd64) npm install --no-save "@rolldown/binding-linux-x64-gnu@1.0.0-rc.12" "lightningcss-linux-x64-gnu@1.32.0" ;; \
        arm64) npm install --no-save "@rolldown/binding-linux-arm64-gnu@1.0.0-rc.12" "lightningcss-linux-arm64-gnu@1.32.0" ;; \
        *) echo "Unsupported frontend builder architecture: ${ARCH}" >&2; exit 1 ;; \
    esac

# Copy frontend source and build
COPY frontend ./
RUN npm run build

# =============================================================================
# Stage 2: Rust Builder
# We use a two-step approach without caching the target directory to ensure
# the binary is always built from the actual source code.
# =============================================================================
FROM rust:bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Cache buster - change this to invalidate build cache
ARG CACHE_BUSTER=1

# Copy all source code at once (no dummy file trick to avoid caching issues)
COPY Cargo.toml Cargo.lock ./
COPY rust-toolchain.toml ./
COPY backend ./backend/
COPY apps ./apps/
COPY crates ./crates/

# Build the application
ENV CARGO_NET_RETRY=10
RUN cargo build --release --bin rustshare-server

# Strip the binary for smaller size
RUN strip target/release/rustshare-server

# =============================================================================
# Stage 3: Runtime Image
# =============================================================================
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates libssl3 wget \
    && rm -rf /var/lib/apt/lists/*

# Copy binary and frontend build
COPY --from=builder /app/target/release/rustshare-server /usr/local/bin/
COPY --from=frontend-builder /app/frontend/build /app/frontend-build

ENV FRONTEND_DIST_DIR=/app/frontend-build

# Use non-root user for security
RUN useradd -m -s /bin/sh appuser \
    && chown -R appuser:appuser /app
USER appuser

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:8080/api/v1/health || exit 1

CMD ["rustshare-server"]
