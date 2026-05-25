# =============================================================================
# Stage 1: Frontend Builder
# =============================================================================
FROM node:26-bookworm-slim AS frontend-builder

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
FROM rust:1.95-bookworm AS builder

ARG TARGETPLATFORM
ARG USE_PRECOMPILED=false
WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get upgrade -y && apt-get install -y pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Cache buster - change this to invalidate build cache
ARG CACHE_BUSTER=1

# Optional copy of precompiled binaries (glob pattern makes it optional)
COPY rustshare-server-x86_64-unknown-linux-gnu* ./
COPY rustshare-server-aarch64-unknown-linux-gnu* ./

# Copy all source code at once (no dummy file trick to avoid caching issues)
COPY Cargo.toml Cargo.lock ./
COPY rust-toolchain.toml ./
COPY backend ./backend/
COPY apps ./apps/
COPY crates ./crates/

# Build the application or select precompiled binaries
ENV CARGO_NET_RETRY=10
RUN mkdir -p target/release \
    && if [ "$USE_PRECOMPILED" = "true" ]; then \
        case "$TARGETPLATFORM" in \
            "linux/amd64") cp rustshare-server-x86_64-unknown-linux-gnu target/release/rustshare-server ;; \
            "linux/arm64") cp rustshare-server-aarch64-unknown-linux-gnu target/release/rustshare-server ;; \
            *) echo "Unsupported target platform for precompiled: $TARGETPLATFORM" >&2; exit 1 ;; \
        esac; \
    else \
        cargo build --release --bin rustshare-server; \
    fi \
    && strip target/release/rustshare-server

# =============================================================================
# Stage 3: Runtime Image
# =============================================================================
FROM debian:bookworm-slim

RUN apt-get update && apt-get upgrade -y && apt-get install -y ca-certificates libssl3 wget \
    && rm -rf /var/lib/apt/lists/*

# Copy binary and frontend build
COPY --from=builder /app/target/release/rustshare-server /usr/local/bin/
COPY --from=frontend-builder /app/frontend/build /app/frontend-build

ENV FRONTEND_DIST_DIR=/app/frontend-build

ARG VERSION=dev
ARG REVISION=unknown

LABEL org.opencontainers.image.title="RustShare Backend"
LABEL org.opencontainers.image.description="Self-hosted file sharing platform backend"
LABEL org.opencontainers.image.url="https://github.com/kubedoio/rustshare"
LABEL org.opencontainers.image.source="https://github.com/kubedoio/rustshare"
LABEL org.opencontainers.image.licenses="Apache-2.0"
LABEL org.opencontainers.image.version="${VERSION}"
LABEL org.opencontainers.image.revision="${REVISION}"

# Use non-root user for security
RUN useradd -m -s /bin/sh appuser \
    && chown -R appuser:appuser /app
USER appuser

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:8080/health || exit 1

CMD ["rustshare-server"]
