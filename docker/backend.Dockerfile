FROM node:20-alpine AS frontend-builder

WORKDIR /app/frontend

ARG VITE_API_URL=/api/v1
ARG VITE_WS_URL=/api/ws
ENV VITE_API_URL=$VITE_API_URL
ENV VITE_WS_URL=$VITE_WS_URL

COPY frontend/package*.json ./
RUN npm install --legacy-peer-deps

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

WORKDIR /srv/rustshare

COPY --from=builder /app/target/release/rustshare-server /usr/local/bin/
COPY --from=frontend-builder /app/frontend/build /srv/rustshare/frontend

ENV FRONTEND_DIST_DIR=/srv/rustshare/frontend

CMD ["rustshare-server"]
