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
./test-deployment.sh
```

Expected output: all containers running, health checks passing, login API responding.

### 5. Access the application

Open http://localhost in your browser.

Default accounts (when `PASSWORD_LOGIN_ENABLED=true`):
- Admin: `admin@localhost` / `admin123`
- Demo viewer: `viewer@localhost` / `viewer123`

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
| `JWT_SECRET` | `dev-secret-change-in-production` | Signing key for session tokens |
| `RUSTSHARE_SECRET_ENCRYPTION_KEY` | `AAAAAAAA...` | Encryption key for sensitive data |
| `DATABASE_URL` | `postgres://rustshare:changeme@postgres:5432/rustshare` | PostgreSQL connection |
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

## Production Checklist

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

3. **Set up TLS**
   - Terminate TLS at your reverse proxy or load balancer
   - The included nginx config listens on port 80 only

4. **Verify backups and restore**
   - Run `./scripts/backup.sh`
   - Run `./scripts/restore-drill.sh`
   - Confirm data is recoverable

5. **Run the deployment test**
   ```bash
   ./test-deployment.sh
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

Check the RustFS console at http://localhost:9001 (credentials: `rustfsadmin` / `rustfsadmin`).

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
