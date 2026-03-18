#!/bin/bash

# Quick Fix Script for Common RustShare Issues
# Run this if you see "Bad Gateway" or connection errors

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
  -e AWS_ACCESS_KEY_ID=rustfsadmin \
  -e AWS_SECRET_ACCESS_KEY=rustfsadmin \
  amazon/aws-cli \
  --endpoint-url http://rustfs:9000 \
  s3 mb s3://rustshare-files 2>&1 | grep -v "BucketAlreadyOwnedByYou" || true

echo ""
echo "4. Testing services..."
echo -n "  - Backend health: "
curl -s http://localhost:8080/health | jq -r '.status' 2>/dev/null || echo "FAILED"

echo -n "  - Login API: "
LOGIN=$(curl -s -X POST http://localhost/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email": "admin@localhost", "password": "admin123"}' 2>/dev/null)
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
echo "Login: admin@localhost / admin123"
echo ""
echo "If issues persist, run: ./test-deployment.sh"
