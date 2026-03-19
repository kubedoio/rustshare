# Frontend Deployment Guide

## Production Architecture

The RustShare frontend is deployed using a multi-container Docker setup with nginx as a reverse proxy.

```
┌─────────────┐
│   Client    │
└──────┬──────┘
       │
       ▼
┌─────────────┐
│   Nginx     │  (Port 80)
│  (Reverse   │
│   Proxy)    │
└──────┬──────┘
       │
       ├───────────────┐
       │               │
       ▼               ▼
┌────────────┐   ┌───────────┐
│  Frontend  │   │  Backend  │
│ SvelteKit  │   │   Rust    │
│  (Node.js) │   │           │
│ Port 3000  │   │ Port 8080 │
└────────────┘   └───────────┘
```

## Docker Configuration

### Multi-Stage Build

The `frontend/Dockerfile` uses a multi-stage build to optimize the production image:

1. **Builder Stage**
   - Uses `node:20-alpine` base image
   - Installs dependencies with `npm install --legacy-peer-deps`
   - Builds the SvelteKit app with Vite
   - Build-time environment variables: `VITE_API_URL`, `VITE_WS_URL`

2. **Production Stage**
   - Copies only the built artifacts and production dependencies
   - Runs the SvelteKit Node.js server (`node build`)
   - Exposes port 3000

### Environment Variables

**Build-time (ARG):**
- `VITE_API_URL`: API endpoint (default: `/api`)
- `VITE_WS_URL`: WebSocket endpoint (default: `/api`)

**Runtime (ENV):**
- `NODE_ENV`: Set to `production`
- `ORIGIN`: Expected origin for CSRF protection (e.g., `http://localhost:3000`)

### .dockerignore

The `.dockerignore` file excludes:
- `node_modules` (reinstalled during build)
- `.svelte-kit` and `build` (generated during build)
- `.git` directory
- Environment files (`.env`, `.env.*`)
- Logs and test outputs
- Editor directories (`.vscode`, `.idea`)
- OS files (`.DS_Store`)

## Nginx Configuration

### Features

1. **Reverse Proxy**
   - Frontend (`/`) → `frontend:3000` (SvelteKit)
   - Backend API (`/api/`) → `backend:8080` (Rust)
   - Storage (`/storage/`) → `rustfs:9000` (MinIO)

2. **WebSocket Support**
   - Enabled for HMR (Hot Module Replacement) on frontend
   - Enabled for real-time sync on `/api/sync` endpoint
   - Proper headers: `Upgrade`, `Connection: upgrade`

3. **Security Headers**
   - `X-Frame-Options: SAMEORIGIN`
   - `X-Content-Type-Options: nosniff`
   - `X-XSS-Protection: 1; mode=block`

4. **Performance Optimizations**
   - Gzip compression enabled for text/JS/CSS/JSON
   - `sendfile`, `tcp_nopush`, `tcp_nodelay` enabled
   - Request buffering disabled for large uploads
   - No client body size limit (`client_max_body_size 0`)

5. **Health Checks**
   - `/health` endpoint returns `200 OK`
   - `/nginx-status` for monitoring (localhost only)

### Timeouts

- **Frontend**: 60s (HMR support)
- **Backend API**: 300s (5 minutes for large uploads)
- **Storage**: 300s (5 minutes for large downloads)

## Docker Compose Integration

The `docker-compose.yml` includes:

```yaml
frontend:
  build:
    context: ./frontend
    dockerfile: Dockerfile
    args:
      VITE_API_URL: /api
      VITE_WS_URL: /api
  ports:
    - "3000:3000"
  environment:
    ORIGIN: http://localhost:3000
  depends_on:
    - backend

nginx:
  image: nginx:alpine
  ports:
    - "80:80"
  volumes:
    - ./docker/nginx.conf:/etc/nginx/nginx.conf:ro
  depends_on:
    - backend
    - frontend
  healthcheck:
    test: ["CMD", "wget", "--quiet", "--tries=1", "--spider", "http://localhost/health"]
    interval: 10s
    timeout: 5s
    retries: 3
```

## Deployment Steps

### Development

```bash
# Start all services
docker compose up

# Frontend available at: http://localhost (via nginx)
# Direct frontend access: http://localhost:3000
# Backend API: http://localhost/api
# MinIO console: http://localhost:9001
```

### Production

1. **Set environment variables**
   ```bash
   export JWT_SECRET="your-secure-secret-here"
   # Update other secrets in docker-compose.yml
   ```

2. **Build images**
   ```bash
   docker compose build
   ```

3. **Start services**
   ```bash
   docker compose up -d
   ```

4. **Verify health**
   ```bash
   curl http://localhost/health  # Should return "OK"
   docker compose ps             # All services should be "healthy"
   ```

### Scaling

To scale the frontend for high availability:

```bash
docker compose up -d --scale frontend=3
```

Nginx will automatically load balance across frontend instances.

## SvelteKit Configuration

### Adapter

Uses `@sveltejs/adapter-node` for Node.js deployment:

```javascript
// svelte.config.js
import adapter from '@sveltejs/adapter-node';

const config = {
  kit: {
    adapter: adapter()
  }
};
```

### Build Output

The build produces:
- `build/` - Server code (Node.js)
- `build/client/` - Static assets (JS, CSS, images)

### Server Features

- SSR (Server-Side Rendering) enabled
- API routes in `src/routes/api/`
- Dynamic imports for code splitting
- Prerendering for static pages

## Monitoring

### Logs

```bash
# View all logs
docker compose logs -f

# Frontend logs only
docker compose logs -f frontend

# Nginx access logs
docker compose exec nginx tail -f /var/log/nginx/access.log

# Nginx error logs
docker compose exec nginx tail -f /var/log/nginx/error.log
```

### Metrics

- Nginx status: `http://localhost/nginx-status` (localhost only)
- Frontend metrics: Available via Node.js runtime
- Backend metrics: Check Rust application logs

## Troubleshooting

### Frontend won't build

```bash
# Check for syntax errors
docker compose exec frontend npm run check

# Rebuild from scratch
docker compose build --no-cache frontend
```

### 502 Bad Gateway

- Check if frontend container is running: `docker compose ps frontend`
- Check frontend logs: `docker compose logs frontend`
- Verify nginx can reach frontend: `docker compose exec nginx wget -qO- http://frontend:3000`

### WebSocket connection fails

- Verify nginx WebSocket configuration (headers: `Upgrade`, `Connection`)
- Check browser console for WebSocket errors
- Verify backend WebSocket endpoint is working: `wscat -c ws://localhost/api/sync`

### File upload fails

- Check `client_max_body_size` in nginx.conf (should be 0 for unlimited)
- Verify timeouts are sufficient (300s recommended)
- Check disk space on host and containers

## Performance Tuning

### Frontend

- **Code splitting**: Automatic via SvelteKit
- **Tree shaking**: Enabled in Vite production build
- **Minification**: Automatic via Vite
- **Prerendering**: Configure in `+page.ts` with `export const prerender = true;`

### Nginx

- **Worker processes**: Defaults to 1, increase in nginx.conf for multi-core systems
- **Worker connections**: Set to 1024, increase for high traffic
- **Gzip level**: Set to 6, adjust for CPU/bandwidth tradeoff

### Caching

Future improvements:
- Add `Cache-Control` headers for static assets
- Enable nginx caching for API responses
- Use CDN for static assets in production

## Security Considerations

### Current Protections

- Security headers (X-Frame-Options, X-Content-Type-Options, X-XSS-Protection)
- CSRF protection via SvelteKit ORIGIN check
- JWT authentication on backend
- No direct access to backend from internet (proxied via nginx)

### Production Recommendations

1. **HTTPS**: Use Let's Encrypt or similar for SSL/TLS
2. **Secrets**: Store JWT_SECRET and DB passwords in secure vault
3. **Rate limiting**: Add nginx rate limiting for API endpoints
4. **Firewall**: Restrict access to internal ports (3000, 8080, 5432, 9000)
5. **Updates**: Keep Node.js, nginx, and dependencies up to date

## CI/CD Integration

### GitHub Actions Example

```yaml
name: Build and Deploy

on:
  push:
    branches: [main]

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Build frontend image
        run: docker build -t rustshare-frontend:${{ github.sha }} frontend/
      - name: Push to registry
        # Push to your container registry
```

### Health Check Script

```bash
#!/bin/bash
# health-check.sh

# Check nginx
curl -f http://localhost/health || exit 1

# Check frontend
curl -f http://localhost:3000 || exit 1

# Check backend
curl -f http://localhost:8080/api/health || exit 1

echo "All health checks passed"
```

## Backup and Recovery

### Frontend State

- Activity history: Stored in browser localStorage (user-specific)
- No server-side state to backup

### Configuration Backup

```bash
# Backup nginx config
cp docker/nginx.conf docker/nginx.conf.backup

# Backup environment
cp .env .env.backup
```

## Rollback Procedure

```bash
# Stop current deployment
docker compose down

# Rebuild from previous commit
git checkout <previous-commit>
docker compose build

# Start with old version
docker compose up -d
```
