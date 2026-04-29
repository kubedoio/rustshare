# RustShare Deployment Guide

## Architecture Overview

RustShare deploys as a multi-container Docker Compose stack. The frontend is **built into the backend image** and served by the Axum backend. Nginx acts as a reverse proxy for all traffic.

```
┌─────────────┐
│   Client    │
└──────┬──────┘
       │
       ▼
┌─────────────┐     ┌─────────────┐
│    Nginx    │────▶│   Backend   │
│   (Port 80) │     │  (Port 8080)│
└─────────────┘     └──────┬──────┘
                           │
           ┌───────────────┴───────────────┐
           ▼                               ▼
    ┌─────────────┐                ┌─────────────┐
    │  PostgreSQL │                │   RustFS    │
    │  (Port 5432)│                │(Port 9000/1)│
    └─────────────┘                └─────────────┘
```

### Why no separate frontend container?

The backend Dockerfile uses a multi-stage build:
1. **Frontend builder** — compiles the SvelteKit SPA
2. **Rust builder** — compiles the backend binary
3. **Runtime** — copies the backend binary and the built frontend assets (`/app/frontend-build`)

This keeps the production runtime simple: one backend container serves both the API and the static SPA.

---

## Quick Start

### 1. Clone and enter the repository

```bash
git clone <repository-url>
cd rustshare
```

### 2. Set required secrets

Copy the example environment file and edit it:

```bash
cp .env.example .env
```

Or generate secrets automatically with the pre-flight script:

```bash
./scripts/pre-flight.sh
```

At minimum, change these values in `.env` for any non-local deployment:

```bash
JWT_SECRET=your-random-secret-here
RUSTSHARE_SECRET_ENCRYPTION_KEY=your-32-byte-base64-key-here
```

For local testing, the defaults in `docker-compose.yml` are sufficient.

### 3. Build and start the stack

```bash
docker compose up -d
```

The first build will compile both the frontend and backend, so it may take several minutes.

### 4. Verify health

```bash
./scripts/final-launch-smoke.sh
```

Expected output: all containers running, health checks passing, login API responding.

### 5. Access the application

Open http://localhost in your browser.

Default accounts (when `PASSWORD_LOGIN_ENABLED=true`):
- Admin: `admin@localhost` — password from `RUSTSHARE_ADMIN_PASSWORD` in `.env`
- Demo viewer: `viewer@localhost` — password from `RUSTSHARE_DEMO_VIEWER_PASSWORD` in `.env`

> If you ran `./scripts/pre-flight.sh`, passwords were auto-generated. Retrieve them from the backend container logs: `docker logs rustshare-backend-1 | grep "Bootstrap admin password"`

---

## TLS / HTTPS Setup

RustShare requires HTTPS in production. TLS is terminated at the nginx reverse proxy.

### Option A: Let's Encrypt with Certbot (Recommended)

1. **Install certbot** on the Docker host:
   ```bash
   # Debian/Ubuntu
   sudo apt update && sudo apt install -y certbot
   # macOS
   brew install certbot
   ```

2. **Obtain certificates**:
   ```bash
   sudo certbot certonly --standalone -d yourdomain.com -d www.yourdomain.com
   ```

3. **Mount certificates** into the nginx container via `docker-compose.prod.yml`:
   ```yaml
   services:
     nginx:
       volumes:
         - /etc/letsencrypt/live/yourdomain.com/fullchain.pem:/etc/nginx/ssl/cert.pem:ro
         - /etc/letsencrypt/live/yourdomain.com/privkey.pem:/etc/nginx/ssl/key.pem:ro
   ```

4. **Enable the 443 server block** in `docker/nginx.conf` by uncommenting the SSL configuration and setting `server_name yourdomain.com;`.

5. **Set up auto-renewal** with a cron job:
   ```bash
   echo "0 3 * * * root certbot renew --quiet && docker compose -f docker-compose.yml -f docker-compose.prod.yml exec -T nginx nginx -s reload" | sudo tee /etc/cron.d/rustshare-certbot
   ```

### Option B: Manual Certificates

1. Place your certificate and private key in `./certs/`:
   ```bash
   mkdir -p certs
   cp your-cert.pem certs/cert.pem
   cp your-key.pem certs/key.pem
   ```

2. Mount them in `docker-compose.prod.yml`:
   ```yaml
   services:
     nginx:
       volumes:
         - ./certs/cert.pem:/etc/nginx/ssl/cert.pem:ro
         - ./certs/key.pem:/etc/nginx/ssl/key.pem:ro
   ```

3. Enable the 443 server block in `docker/nginx.conf` and set `server_name` to your domain.

### Option C: External TLS Termination (Cloudflare, AWS ALB, etc.)

If TLS is terminated at a CDN or load balancer upstream of RustShare:

1. **Forward plain HTTP** from the edge to nginx. Ensure the nginx port is not exposed to the public internet directly (bind to `127.0.0.1:80` in `docker-compose.prod.yml`).

2. **Set the `X-Forwarded-Proto` header** at your edge proxy:
   ```
   X-Forwarded-Proto: https
   ```

3. **Update `ORIGIN`** in `.env` to your HTTPS URL:
   ```bash
   ORIGIN=https://yourdomain.com
   ```

4. The included nginx config already passes `X-Forwarded-Proto` to the backend via proxy headers.

---

## Compose Profiles

### Standard stack (`docker-compose.yml`)

Services: `postgres`, `rustfs`, `backend`, `nginx`

Use this for local development, self-hosted deployments, and production builds from source.

```bash
docker compose up -d
docker compose build --no-cache backend
docker compose up -d --force-recreate backend
```

### Pilot stack (`docker-compose.pilot.yml`)

Use this to run a pre-built backend image instead of building from source.

```bash
export RUSTSHARE_BACKEND_IMAGE=ghcr.io/kubedoio/rustshare-backend:latest
docker compose -f docker-compose.yml -f docker-compose.pilot.yml up -d
```

### Development override (`docker-compose.dev.yml`)

Overrides the backend with debug-friendly settings (more verbose logging, exposed port 8080).

```bash
docker compose -f docker-compose.yml -f docker-compose.dev.yml up -d
```

### Frontend dev server (`docker-compose.frontend.yml`)

Runs a hot-reload Vite dev server on port 5173. Useful when you want to work on the frontend against a running backend.

```bash
docker compose -f docker-compose.frontend.yml up -d
```

Then open http://localhost:5173.

---

## Rebuilding after code changes

### Frontend changes

Because the frontend is baked into the backend image, you must rebuild the backend container:

```bash
docker compose build --no-cache backend
docker compose up -d --force-recreate backend
```

### Backend changes

```bash
docker compose build backend
docker compose up -d --force-recreate backend
```

### Full reset

```bash
docker compose down
docker compose up -d --build
```

---

## Environment Variables

### Backend / runtime

| Variable | Default | Purpose |
|----------|---------|---------|
| `JWT_SECRET` | *(empty — must be set)* | Signing key for session tokens |
| `RUSTSHARE_SECRET_ENCRYPTION_KEY` | *(empty — must be set)* | Encryption key for sensitive data |
| `DATABASE_URL` | *(empty — must be set)* | PostgreSQL connection |
| `RUSTFS_ENDPOINT` | `http://rustfs:9000` | Internal S3-compatible object storage |
| `RUSTFS_PUBLIC_ENDPOINT` | `http://localhost:9000` | Public-facing object storage URL |
| `RUSTFS_BUCKET` | `rustshare-files` | Object storage bucket |
| `RUSTSHARE_METADATA_BACKEND` | `postgres` | Metadata store backend (`postgres`, `rustfs`, `dual_write`, `rustfs_reads`, `localfs`) |
| `PASSWORD_LOGIN_ENABLED` | `true` | Whether password login is available |
| `OIDC_ISSUER_URL` | — | OIDC provider URL |
| `OIDC_CLIENT_ID` | — | OIDC client ID |
| `OIDC_CLIENT_SECRET` | — | OIDC client secret |
| `OIDC_REDIRECT_URL` | — | OIDC callback URL |

### Frontend / build-time

These are passed as `ARG` values in `docker/backend.Dockerfile`:

| Variable | Default | Purpose |
|----------|---------|---------|
| `VITE_API_URL` | `/api/v1` | API base path |
| `VITE_WS_URL` | `/api/ws` | WebSocket endpoint path |

---

## Production Hardening

The `docker-compose.prod.yml` override applies production-hardened settings. Use it with the base compose file:

```bash
docker compose -f docker-compose.yml -f docker-compose.prod.yml up -d
```

### Restart Policies

All services are configured with `restart: unless-stopped` so the stack recovers automatically after host reboots or container crashes.

### Resource Limits

CPU and memory limits prevent a single container from consuming all host resources. Default limits are defined in `docker-compose.prod.yml`; adjust them based on your workload and host capacity.

### Log Rotation

Docker logging drivers are configured with `max-size` and `max-file` limits to prevent unbounded log growth:

```yaml
logging:
  driver: "json-file"
  options:
    max-size: "10m"
    max-file: "3"
```

### Internal Port Binding

Internal service ports (backend 8080, postgres 5432, rustfs 9000/9001) are bound to `127.0.0.1` so they are not reachable from outside the host. Only nginx ports 80 and 443 are exposed publicly.

### Non-Root Containers

The backend Dockerfile runs as an unprivileged `appuser`. Nginx already runs as a non-root user in the official `nginx:alpine` image.

---

## Security Headers

The nginx configuration includes several security headers. Verify they are present and understand what each does:

| Header | Value | Purpose |
|--------|-------|---------|
| `Content-Security-Policy` | `default-src 'self'; ...` | Prevents XSS and data injection by controlling resource sources |
| `Strict-Transport-Security` | `max-age=63072000; includeSubDomains; preload` | Enforces HTTPS for 2 years (HSTS) |
| `Referrer-Policy` | `strict-origin-when-cross-origin` | Limits referrer information leaked to third parties |
| `X-Frame-Options` | `SAMEORIGIN` | Prevents clickjacking by restricting iframe embedding |
| `X-Content-Type-Options` | `nosniff` | Prevents MIME-type sniffing |
| `X-XSS-Protection` | `1; mode=block` | Legacy XSS filter (defense in depth) |

These headers are configured in the server block of `docker/nginx.conf`. If you use external TLS termination, ensure your edge proxy also sets equivalent headers.

---

## Production Deployment Checklist

Before deploying to production:

1. **Change all default secrets**
   - `JWT_SECRET`
   - `RUSTSHARE_SECRET_ENCRYPTION_KEY`
   - PostgreSQL password
   - RustFS root credentials

2. **Configure OIDC** (if using SSO)
   - `OIDC_ISSUER_URL`
   - `OIDC_CLIENT_ID`
   - `OIDC_CLIENT_SECRET`
   - `OIDC_REDIRECT_URL`

3. **Configure TLS**
   - [ ] Choose a TLS option (Let's Encrypt, manual certs, or external termination)
   - [ ] Enable the 443 server block in `docker/nginx.conf`
   - [ ] Verify HTTPS is working and redirects from HTTP to HTTPS are active
   - [ ] Set `ORIGIN` in `.env` to your HTTPS URL

4. **Use `docker-compose.prod.yml`**
   - [ ] Start the stack with `docker compose -f docker-compose.yml -f docker-compose.prod.yml up -d`
   - [ ] Verify restart policies, resource limits, and log rotation are active
   - [ ] Confirm internal ports are bound to `127.0.0.1`

5. **Verify security headers**
   - [ ] Check `Content-Security-Policy`, `Strict-Transport-Security`, and `Referrer-Policy` are present
   - [ ] Run `curl -I https://yourdomain.com` and inspect response headers

6. **Verify backups and restore**
   - Run `./scripts/backup-stack.sh`
   - Run `./scripts/run-restore-drill.sh`
   - Confirm data is recoverable

7. **Run the deployment test**
   ```bash
   ./scripts/final-launch-smoke.sh
   ```

See [docs/PRODUCTION_READINESS.md](PRODUCTION_READINESS.md) for the full launch hardening checklist.

---

## Troubleshooting

### "Welcome to SvelteKit" or blank page

The backend is serving stale frontend assets. Rebuild:

```bash
docker compose build --no-cache backend
docker compose up -d --force-recreate backend
```

### API returns 404

Check that nginx is proxying `/api/` to the backend:

```bash
curl -I http://localhost/api/v1/health
```

If nginx returns 404, verify the container is healthy:

```bash
docker compose ps
```

### Database connection errors

Ensure postgres is healthy before the backend starts:

```bash
docker compose logs postgres
docker compose logs backend
```

### Object storage errors

Ensure RustFS is running and reachable:

```bash
curl http://localhost:9000
```

Check the RustFS console at http://localhost:9001 (credentials are `RUSTFS_ROOT_USER` / `RUSTFS_ROOT_PASSWORD` from your `.env` file).

---

## Migration Checksum Fix

If migration `20260404000002_add_tenant_sharing_config.sql` fails with a checksum mismatch error:

```
error: migration 20260404000002 was previously applied but has been modified
```

This can happen if the migration file was modified after being applied to the database.

### Solution

Run the following SQL on the database before deploying:

```sql
DELETE FROM _sqlx_migrations WHERE version = '20260404000002';
```

Then restart the backend. The migration will be re-applied with the new checksum.

### Note

This is safe because the migration only adds a table and columns - re-running it on an already-migrated database will be a no-op (the table/columns already exist).
