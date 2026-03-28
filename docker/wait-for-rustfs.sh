#!/bin/sh
# Wait for RustFS S3 service to be ready

set -e

RUSTFS_HOST="${RUSTFS_HOST:-rustfs}"
RUSTFS_PORT="${RUSTFS_PORT:-9000}"
MAX_RETRIES="${MAX_RETRIES:-30}"
RETRY_DELAY="${RETRY_DELAY:-2}"

echo "Waiting for RustFS at ${RUSTFS_HOST}:${RUSTFS_PORT}..."

retry=0
while [ $retry -lt $MAX_RETRIES ]; do
    # Try to connect to RustFS health endpoint
    if wget --quiet --timeout=5 --tries=1 --spider "http://${RUSTFS_HOST}:${RUSTFS_PORT}/health" 2>/dev/null; then
        echo "RustFS health endpoint is responding"
        # Additional wait for S3 service to be fully initialized
        echo "Waiting additional 3 seconds for S3 service to be ready..."
        sleep 3
        echo "RustFS is ready!"
        exec "$@"
    fi
    
    retry=$((retry + 1))
    echo "RustFS not ready yet (attempt $retry/$MAX_RETRIES), retrying in ${RETRY_DELAY}s..."
    sleep $RETRY_DELAY
done

echo "ERROR: RustFS did not become ready within $MAX_RETRIES attempts"
exit 1
