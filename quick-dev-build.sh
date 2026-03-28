#!/bin/bash
set -e

echo "=== RustShare Quick Development Build ==="
echo ""

# Build frontend
echo "Building frontend..."
cd frontend
npm install --legacy-peer-deps 2>/dev/null || true
npm run build
cd ..

echo ""
echo "Building backend Docker image..."
docker-compose -f docker-compose.dev-optimized.yml build backend

echo ""
echo "Starting services..."
docker-compose -f docker-compose.dev-optimized.yml up -d

echo ""
echo "=== Done! ==="
echo "Application should be available at http://localhost"
echo ""
echo "To view logs:"
echo "  docker-compose -f docker-compose.dev-optimized.yml logs -f"
echo ""
echo "To rebuild only frontend (fast):"
echo "  cd frontend && npm run build"
echo "  docker-compose -f docker-compose.dev-optimized.yml restart nginx backend"
