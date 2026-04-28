#!/bin/bash

# Quick Fix Script for Common RustShare Issues
# Run this if you see "Bad Gateway" or connection errors

# Require credentials via environment variables — no hardcoded defaults
if [ -z "${ADMIN_EMAIL:-}" ] || [ -z "${ADMIN_PASSWORD:-}" ]; then
    echo "Error: ADMIN_EMAIL and ADMIN_PASSWORD must be set as environment variables." >&2
    exit 1
fi

if [ -z "${AWS_ACCESS_KEY_ID:-}" ] || [ -z "${AWS_SECRET_ACCESS_KEY:-}" ]; then
    echo "Error: AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY must be set as environment variables." >&2
    exit 1
fi

echo "🔧 RustShare Quick Fix"
echo "====================="
echo ""

echo "1. Restarting all services..."
docker-compose restart

echo ""
echo "2. Waiting for services to start..."
sleep 5

echo ""
echo "3. Creating MinIO bucket if needed..."
docker run --rm --network rustshare_default \
  -e AWS_ACCESS_KEY_ID="$AWS_ACCESS_KEY_ID" \
  -e AWS_SECRET_ACCESS_KEY="$AWS_SECRET_ACCESS_KEY" \
  amazon/aws-cli \
  --endpoint-url http://rustfs:9000 \
  s3 mb s3://rustshare-files 2>&1 | grep -v "BucketAlreadyOwnedByYou" || true

echo ""
echo "4. Testing services..."
echo -n "  - Backend health: "
curl -s http://localhost:8080/health | jq -r '.status' 2>/dev/null || echo "FAILED"

echo -n "  - Login API: "
LOGIN=$(curl -s -X POST http://localhost/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d "{\"email\": \"$ADMIN_EMAIL\", \"password\": \"$ADMIN_PASSWORD\"}" 2>/dev/null)
if echo "$LOGIN" | jq -e '.token' > /dev/null 2>&1; then
    echo "OK"
else
    echo "FAILED"
fi

echo -n "  - Frontend: "
curl -s http://localhost/ -o /dev/null && echo "OK" || echo "FAILED"

echo ""
echo "✅ Quick fix complete!"
echo ""
echo "Access RustShare at: http://localhost"
echo "Login credentials are provided via ADMIN_EMAIL and ADMIN_PASSWORD environment variables."
echo ""
echo "If issues persist, run: ./test-deployment.sh"
