FROM node:20-bookworm-slim AS frontend-builder

WORKDIR /app/frontend

ARG VITE_API_URL=/api/v1
ARG VITE_WS_URL=/api/ws
ENV VITE_API_URL=$VITE_API_URL
ENV VITE_WS_URL=$VITE_WS_URL

COPY frontend/package*.json ./
RUN npm install --legacy-peer-deps \
    && ARCH="$(dpkg --print-architecture)" \
    && case "$ARCH" in \
        amd64) npm install --no-save "@rolldown/binding-linux-x64-gnu@1.0.0-rc.12" "lightningcss-linux-x64-gnu@1.32.0" ;; \
        arm64) npm install --no-save "@rolldown/binding-linux-arm64-gnu@1.0.0-rc.12" "lightningcss-linux-arm64-gnu@1.32.0" ;; \
        *) echo "Unsupported frontend builder architecture: ${ARCH}" >&2; exit 1 ;; \
    esac

COPY frontend ./
RUN npm run build

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
COPY --from=frontend-builder /app/frontend/build /app/frontend-build

ENV FRONTEND_DIST_DIR=/app/frontend-build

CMD ["rustshare-server"]
