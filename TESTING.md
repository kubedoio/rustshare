# RustShare Testing Guide

This guide explains how to test the RustShare deployment to prevent issues before they reach production.

## Quick Test

Run the automated deployment test suite:

```bash
./test-deployment.sh
```

This will verify:
- ✓ All containers are running
- ✓ Database connectivity
- ✓ Object storage health
- ✓ Frontend pages load correctly
- ✓ Login API functionality
- ✓ Authentication works
- ✓ API routing through nginx
- ✓ No SSR errors
- ✓ Correct API URL configuration

## Manual Browser Testing

After automated tests pass, manually verify in your browser:

### 1. Access the Application
Open: http://localhost

**Expected:**
- Brief loading spinner
- Automatic redirect to /login

### 2. Login Page
**Expected:**
- "RustShare" title
- Email field (placeholder: admin@localhost)
- Password field
- Login button

**Test:**
- Enter: admin@localhost / admin123
- Click Login

**Expected:**
- Redirect to /files
- No errors in browser console (F12)

### 3. File Browser
**Expected:**
- Sidebar with navigation
- Header with user avatar
- Empty file list (on fresh deployment)
- "New Folder" button works
- Upload button present

### 4. Create Test Folder
**Test:**
- Click "New Folder"
- Enter name: "Test Folder"
- Click Create

**Expected:**
- Folder appears in list
- No errors in console

### 5. Browser Console Check
Press F12 and check Console tab:

**Should NOT see:**
- ❌ "Failed to fetch"
- ❌ "404 Not Found"
- ❌ "Cannot call goto()"
- ❌ "CORS error"
- ❌ Network errors to localhost:8080

**Should see:**
- ✓ Successful API calls to /api/*
- ✓ WebSocket connection (if implemented)

## Common Issues & Solutions

### Issue: "Welcome to SvelteKit" on homepage
**Cause:** Root page redirect not implemented
**Fix:**
```bash
# Check frontend/src/routes/+page.svelte has redirect logic
docker-compose build frontend
docker-compose up -d --force-recreate frontend
```

### Issue: HTTP 500 on /files page
**Cause:** SSR error with goto()
**Fix:**
```bash
# Check (app)/+layout.svelte uses browser check
# Ensure: if (browser && !$isAuthenticated) { goto('/login'); }
docker-compose build frontend
docker-compose up -d --force-recreate frontend
```

### Issue: Login API returns "Failed to fetch"
**Cause:** Frontend using wrong API URL
**Check:**
```bash
# Should show "/api" not "http://backend:8080"
docker exec rustshare-frontend-1 grep -r "backend:8080\|localhost:8080" /app/build
```

**Fix:**
```bash
# Rebuild with correct VITE_API_URL
docker-compose build --no-cache frontend
docker-compose up -d --force-recreate frontend
```

### Issue: Nginx returns 404 for /api requests
**Cause:** Nginx not routing to backend
**Check:**
```bash
# Should return 200 or 400 (not 404)
curl -I http://localhost/api/auth/login
```

**Fix:**
```bash
# Check docker/nginx.conf has proxy_pass to backend
docker-compose restart nginx
```

## Pre-Deployment Checklist

Before deploying or committing changes:

- [ ] Run `./test-deployment.sh` - all tests pass
- [ ] Login works in browser
- [ ] Can create folders
- [ ] Can upload files (when implemented)
- [ ] Browser console has no errors
- [ ] No hardcoded URLs in frontend build
- [ ] SSR works (no 500 errors)
- [ ] All containers are healthy

## Continuous Testing

### After Code Changes

1. **Backend changes:**
```bash
docker-compose build backend
docker-compose up -d --force-recreate backend
./test-deployment.sh
```

2. **Frontend changes:**
```bash
docker-compose build --no-cache frontend
docker-compose up -d --force-recreate frontend
./test-deployment.sh
```

3. **Docker/Config changes:**
```bash
docker-compose down
docker-compose up -d --build
./test-deployment.sh
```

### Integration with CI/CD

Add to your CI pipeline:

```yaml
# Example GitHub Actions
- name: Test Deployment
  run: |
    docker-compose up -d
    ./test-deployment.sh
```

## Monitoring in Production

Check logs for errors:

```bash
# Frontend logs
docker logs rustshare-frontend-1 --tail 100 | grep -i error

# Backend logs
docker logs rustshare-backend-1 --tail 100 | grep -i error

# Nginx logs
docker logs rustshare-nginx-1 --tail 100 | grep -E "error|404|500"
```

## Test Coverage

Current test coverage:

- ✅ Container health checks
- ✅ Database connectivity
- ✅ Object storage health
- ✅ Frontend SSR rendering
- ✅ Login API functionality
- ✅ JWT authentication
- ✅ Nginx routing
- ✅ API URL configuration
- ⚠️  File upload (manual test required)
- ⚠️  WebSocket real-time sync (when implemented)
- ⚠️  Share links (when implemented)

## Troubleshooting

### Tests fail but deployment works
The test script may be too strict. Check:
1. Are containers actually unhealthy?
2. Is the test URL correct?
3. Run tests manually: `curl http://localhost/api/auth/login`

### Tests pass but browser fails
Browser-specific issues:
1. Check browser console (F12)
2. Check Network tab for failed requests
3. Clear browser cache
4. Try incognito/private mode

### All tests fail
Complete deployment issue:
1. Check: `docker-compose ps`
2. Check: `docker-compose logs`
3. Try: `docker-compose down && docker-compose up -d --build`

## Future Enhancements

Planned test additions:
- [ ] File upload/download tests
- [ ] Share link creation/access tests
- [ ] WebSocket connection tests
- [ ] Performance benchmarks
- [ ] Security scanning
- [ ] Load testing

## Support

If issues persist:
1. Check logs: `docker-compose logs`
2. Review this guide's Common Issues section
3. Run: `./test-deployment.sh` for detailed diagnostics
