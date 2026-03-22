#!/bin/bash

# RustShare Deployment Test Script
# Tests the complete deployment to catch issues before manual testing

# Don't exit on error - we want to run all tests
set +e

echo "=================================="
echo "RustShare Deployment Test Suite"
echo "=================================="
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

FAILED_TESTS=0
PASSED_TESTS=0

# Helper functions
test_passed() {
    echo -e "${GREEN}✓ PASS${NC}: $1"
    ((PASSED_TESTS++))
}

test_failed() {
    echo -e "${RED}✗ FAIL${NC}: $1"
    echo "  Details: $2"
    ((FAILED_TESTS++))
}

test_warning() {
    echo -e "${YELLOW}⚠ WARN${NC}: $1"
}

# Wait for service to be ready
wait_for_service() {
    local url=$1
    local max_attempts=30
    local attempt=0

    echo "Waiting for $url to be ready..."
    while [ $attempt -lt $max_attempts ]; do
        if curl -s -o /dev/null -w "%{http_code}" "$url" | grep -q "200\|404\|500"; then
            return 0
        fi
        sleep 1
        ((attempt++))
    done
    return 1
}

echo "===== Service Health Checks ====="
echo ""

# Test 1: Check if all containers are running
echo "Test 1: Checking if all containers are running..."
EXPECTED_CONTAINERS=("rustshare-postgres-1" "rustshare-rustfs-1" "rustshare-backend-1" "rustshare-frontend-1" "rustshare-nginx-1")
ALL_RUNNING=true

for container in "${EXPECTED_CONTAINERS[@]}"; do
    if docker ps --format '{{.Names}}' | grep -q "^${container}$"; then
        echo "  ✓ $container is running"
    else
        ALL_RUNNING=false
        echo "  ✗ $container is NOT running"
    fi
done

if [ "$ALL_RUNNING" = true ]; then
    test_passed "All containers are running"
else
    test_failed "Some containers are not running" "Check docker-compose logs"
fi

# Test 2: Database health
echo ""
echo "Test 2: Checking database health..."
if docker exec rustshare-postgres-1 pg_isready -U rustshare > /dev/null 2>&1; then
    test_passed "PostgreSQL is healthy"
else
    test_failed "PostgreSQL is not healthy" "Check database logs"
fi

# Test 3: MinIO health
echo ""
echo "Test 3: Checking object storage health..."
if curl -s http://localhost:9000/minio/health/live > /dev/null 2>&1; then
    test_passed "MinIO (RustFS) is healthy"
else
    test_failed "MinIO is not healthy" "Check MinIO logs"
fi

echo ""
echo "===== Frontend Tests ====="
echo ""

# Wait for frontend to be ready
wait_for_service "http://localhost:3000"

# Test 4: Root page redirects
echo "Test 4: Checking root page redirect..."
ROOT_CONTENT=$(curl -s http://localhost/)
if echo "$ROOT_CONTENT" | grep -q "loading loading-spinner"; then
    test_passed "Root page shows loading spinner (redirect logic present)"
else
    if echo "$ROOT_CONTENT" | grep -q "Welcome to SvelteKit"; then
        test_failed "Root page shows default SvelteKit message" "Redirect logic not implemented"
    else
        test_warning "Root page content unexpected"
    fi
fi

# Test 5: Login page loads
echo ""
echo "Test 5: Checking login page..."
LOGIN_PAGE=$(curl -s http://localhost/login)
if echo "$LOGIN_PAGE" | grep -q "RustShare"; then
    test_passed "Login page loads with RustShare branding"
else
    test_failed "Login page doesn't load correctly" "Missing RustShare branding"
fi

# Test 6: Login page has form fields
if echo "$LOGIN_PAGE" | grep -q "email" && echo "$LOGIN_PAGE" | grep -q "password"; then
    test_passed "Login form has email and password fields"
else
    test_failed "Login form is missing fields" "Form structure incomplete"
fi

echo ""
echo "===== Backend API Tests ====="
echo ""

# Wait for backend to be ready
wait_for_service "http://localhost:8080/health"

# Test 7: Backend health endpoint
echo "Test 7: Checking backend health..."
HEALTH_RESPONSE=$(curl -s http://localhost:8080/health)
if echo "$HEALTH_RESPONSE" | grep -q "ok"; then
    test_passed "Backend health endpoint responds"
else
    test_failed "Backend health endpoint not responding" "Check backend logs"
fi

# Test 8: Login API functionality
echo ""
echo "Test 8: Testing login API..."
LOGIN_RESPONSE=$(curl -s -X POST http://localhost/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email": "admin@localhost", "password": "admin123"}')

if echo "$LOGIN_RESPONSE" | jq -e '.token' > /dev/null 2>&1; then
    test_passed "Login API returns JWT token"
    TOKEN=$(echo "$LOGIN_RESPONSE" | jq -r '.token')
else
    test_failed "Login API doesn't return token" "Response: $LOGIN_RESPONSE"
    TOKEN=""
fi

# Test 9: Token contains user info
if echo "$LOGIN_RESPONSE" | jq -e '.user.email' > /dev/null 2>&1; then
    USER_EMAIL=$(echo "$LOGIN_RESPONSE" | jq -r '.user.email')
    if [ "$USER_EMAIL" = "admin@localhost" ]; then
        test_passed "Login API returns correct user info"
    else
        test_failed "Login API returns wrong user email" "Expected: admin@localhost, Got: $USER_EMAIL"
    fi
else
    test_failed "Login API doesn't return user info" "Missing user object"
fi

# Test 10: Authenticated API request
echo ""
echo "Test 10: Testing authenticated API request..."
if [ -n "$TOKEN" ]; then
    # Try to create a test folder
    FOLDER_RESPONSE=$(curl -s -X POST http://localhost/api/v1/folders \
      -H "Authorization: Bearer $TOKEN" \
      -H "Content-Type: application/json" \
      -d '{"name": "Test Deployment Folder", "parent_folder_id": null}')

    if echo "$FOLDER_RESPONSE" | jq -e '.id' > /dev/null 2>&1; then
        test_passed "Authenticated API requests work"
        FOLDER_ID=$(echo "$FOLDER_RESPONSE" | jq -r '.id')

        # Clean up test folder
        curl -s -X DELETE "http://localhost/api/v1/folders/$FOLDER_ID" \
          -H "Authorization: Bearer $TOKEN" > /dev/null 2>&1
    else
        test_failed "Authenticated API request failed" "Response: $FOLDER_RESPONSE"
    fi
else
    test_warning "Skipping authenticated API test (no token)"
fi

echo ""
echo "===== SSR Tests ====="
echo ""

# Test 11: Files page doesn't error (SSR check)
echo "Test 11: Checking files page SSR..."
FILES_STATUS=$(curl -s -o /dev/null -w "%{http_code}" http://localhost/files)
if [ "$FILES_STATUS" = "200" ]; then
    test_passed "Files page renders without SSR errors"
else
    test_failed "Files page returns HTTP $FILES_STATUS" "Check frontend logs for SSR errors"
fi

# Test 12: Check for common SSR errors in logs
echo ""
echo "Test 12: Checking frontend logs for errors..."
ERROR_COUNT=$(docker logs rustshare-frontend-1 --tail 100 2>&1 | grep -i "error\|500" | grep -v "grep" | wc -l | tr -d ' ')
if [ "$ERROR_COUNT" -eq "0" ]; then
    test_passed "No errors in frontend logs"
else
    test_warning "Found $ERROR_COUNT error messages in frontend logs"
    echo "  Recent errors:"
    docker logs rustshare-frontend-1 --tail 100 2>&1 | grep -i "error\|500" | tail -5 | sed 's/^/    /'
fi

echo ""
echo "===== API URL Configuration Tests ====="
echo ""

# Test 13: Check API URL in frontend build
echo "Test 13: Checking API URL configuration in frontend..."
if docker exec rustshare-frontend-1 grep -r "localhost:8080\|backend:8080" /app/build 2>/dev/null | grep -q .; then
    test_failed "Frontend contains hardcoded backend URLs" "Should use /api paths"
else
    test_passed "Frontend uses correct relative API paths"
fi

# Test 14: Check VITE_API_URL is embedded correctly
if docker exec rustshare-frontend-1 grep -q '"/api' /app/build/client/_app/immutable/chunks/*.js 2>/dev/null; then
    test_passed "VITE_API_URL is correctly embedded as /api"
else
    test_warning "Could not verify VITE_API_URL in build"
fi

echo ""
echo "===== Nginx Configuration Tests ====="
echo ""

# Test 15: Nginx routing
echo "Test 15: Checking nginx API routing..."
NGINX_API_STATUS=$(curl -s -o /dev/null -w "%{http_code}" http://localhost/api/v1/auth/login)
if [ "$NGINX_API_STATUS" = "400" ] || [ "$NGINX_API_STATUS" = "401" ] || [ "$NGINX_API_STATUS" = "405" ]; then
    # These status codes indicate the API is reachable (we didn't send proper request)
    test_passed "Nginx correctly routes /api requests to backend"
elif [ "$NGINX_API_STATUS" = "200" ]; then
    test_passed "Nginx correctly routes /api requests to backend"
else
    test_failed "Nginx API routing not working" "HTTP status: $NGINX_API_STATUS"
fi

# Test 16: Nginx frontend routing
echo ""
echo "Test 16: Checking nginx frontend routing..."
NGINX_FRONTEND_STATUS=$(curl -s -o /dev/null -w "%{http_code}" http://localhost/)
if [ "$NGINX_FRONTEND_STATUS" = "200" ]; then
    test_passed "Nginx correctly routes / requests to frontend"
else
    test_failed "Nginx frontend routing not working" "HTTP status: $NGINX_FRONTEND_STATUS"
fi

echo ""
echo "=================================="
echo "Test Summary"
echo "=================================="
echo -e "${GREEN}Passed: $PASSED_TESTS${NC}"
echo -e "${RED}Failed: $FAILED_TESTS${NC}"
echo ""

if [ $FAILED_TESTS -eq 0 ]; then
    echo -e "${GREEN}✓ All tests passed! Deployment is healthy.${NC}"
    echo ""
    echo "You can now access RustShare at:"
    echo "  http://localhost"
    echo ""
    echo "Login credentials:"
    echo "  Email: admin@localhost"
    echo "  Password: admin123"
    exit 0
else
    echo -e "${RED}✗ Some tests failed. Please review the errors above.${NC}"
    exit 1
fi
