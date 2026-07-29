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

### 2. Generate required secrets (REQUIRED)

`.env.example` ships with **empty** secrets, and the backend refuses to start
until they are set. Copy the example file, then generate strong secrets into
it with the pre-flight script:

```bash
cp .env.example .env
./scripts/pre-flight.sh
```

The script appends the generated values to `.env`. If you prefer to edit
`.env` manually, generate real values with `openssl rand -base64 32` (or
`openssl rand -hex 32` where noted in `.env.example`) for at least:

```bash
JWT_SECRET=<openssl rand -base64 32>
RUSTSHARE_SECRET_ENCRYPTION_KEY=<openssl rand -base64 32>
POSTGRES_PASSWORD=<openssl rand -hex 32>
DATABASE_URL=postgres://rustshare:<POSTGRES_PASSWORD>@postgres:5432/rustshare
RUSTFS_ROOT_USER=<alphanumeric access key>
RUSTFS_ROOT_PASSWORD=<openssl rand -hex 32>
AWS_ACCESS_KEY_ID=<same as RUSTFS_ROOT_USER>
AWS_SECRET_ACCESS_KEY=<same as RUSTFS_ROOT_PASSWORD>
RUSTSHARE_CHAT_WEBHOOK_SECRET=<openssl rand -base64 32>
```

Skipping this step and running `docker compose up -d` with the verbatim
`.env.example` values fails: the backend exits with a configuration
validation error (see [First-start failures](#first-start-failures)).

### 3. Build and start the stack

```bash
docker compose up -d
```

The first build compiles both the frontend and backend, so it may take
several minutes. Once the images are built, a normal stack start takes
roughly a minute: PostgreSQL and RustFS must report healthy before the
backend runs migrations and seeds the bootstrap accounts. Wait for all
containers to be healthy:

```bash
docker compose ps
```

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

> If you ran `./scripts/pre-flight.sh`, passwords were auto-generated. Retrieve the admin password from the secure bootstrap file inside the backend container: `docker exec rustshare-backend-1 cat /tmp/rustshare-bootstrap-password.txt` (path configurable via `RUSTSHARE_BOOTSTRAP_PASSWORD_FILE`).

---

## TLS / HTTPS Setup

RustShare requires HTTPS in production.

The production Compose profile publishes nginx only on `127.0.0.1:8080`. A
same-host HTTPS reverse proxy must accept public traffic and forward it to that
loopback listener. Do not expose port 80 directly: production sessions use
`Secure` cookies and will not authenticate over HTTP.

### Required External TLS Termination

Use a host reverse proxy such as Caddy, nginx, or HAProxy. A CDN or load
balancer can terminate TLS upstream only when its connection to the host is
carried through a private tunnel.

1. **Run a same-host TLS proxy** that forwards plain HTTP to nginx at
   `127.0.0.1:8080`. A remote CDN or load balancer must connect through a private
   tunnel or a same-host proxy; never expose the HTTP listener publicly.

2. **Set the `X-Forwarded-Proto` header** at your edge proxy:
   ```
   X-Forwarded-Proto: https
   ```

3. **Update `ORIGIN`** in `.env` to your HTTPS URL:
   ```bash
   ORIGIN=https://yourdomain.com
   ```

4. The included nginx config validates and preserves `X-Forwarded-Proto: https`
   when forwarding to the backend.

### RustFS Upgrades

RustFS is pinned by digest because it owns persistent data. To upgrade it:

1. Back up the stack and verify the backup bundle.
2. Run a restore drill with the candidate digest in an isolated environment.
3. Review upstream storage-format or migration requirements.
4. Update the digest in both `docker-compose.yml` and
   `docker-compose.restore-drill.yml`, then run the launch smoke test.
5. Keep the previous digest and verified backup available for rollback.

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

### Required production secrets

The following values **must** be set for any production deployment. Generate
them with `scripts/pre-flight.sh` or manually with `openssl rand -base64 32`.

| Variable | How to generate | Rotation |
|----------|-----------------|----------|
| `JWT_SECRET` | `openssl rand -base64 32` | Rotate on suspected compromise or at least quarterly. After rotation, existing sessions are invalidated and users must log in again. |
| `RUSTSHARE_SECRET_ENCRYPTION_KEY` | `openssl rand -base64 32` | Rotate on suspected compromise. **Back up the old key** until all data encrypted with it has been re-encrypted, or you will lose access to stored secrets. |
| `POSTGRES_PASSWORD` | `openssl rand -hex 32` | Rotate periodically and whenever a team member with access leaves. Update `DATABASE_URL` and restart the stack. |
| `RUSTFS_ROOT_USER` / `RUSTFS_ROOT_PASSWORD` | Run `scripts/pre-flight.sh` (user: alphanumeric access key; password: `openssl rand -hex 32`) | Rotate together. Update `STORAGE_ACCESS_KEY` / `STORAGE_SECRET_KEY` and any S3 clients. |
| `STORAGE_ACCESS_KEY` / `STORAGE_SECRET_KEY` | Must match RustFS root credentials | Rotate with RustFS root credentials. |
| `RUSTSHARE_ADMIN_PASSWORD` | Optional — leave empty to auto-generate a password stored in the secure bootstrap file. | Rotate after first login and whenever the admin credential is suspected to be exposed. |
| `RUSTSHARE_DEMO_VIEWER_PASSWORD` | Run `scripts/pre-flight.sh` (`openssl rand -hex 32`) | Rotate if demo mode is enabled in production (not recommended). |

### Optional secrets

| Variable | Purpose | Rotation |
|----------|---------|----------|
| `OIDC_CLIENT_SECRET` | OIDC provider client secret | Follow your IdP's rotation policy; update this value and restart the backend. |
| `RUSTSHARE_CHAT_WEBHOOK_SECRET` | Webhook signing secret | Rotate on suspected compromise. |
| `METRICS_API_TOKEN` | Bearer token for Prometheus `/metrics` endpoint | Rotate periodically if the endpoint is exposed. |

### Backend / runtime

| Variable | Default | Purpose |
|----------|---------|---------|
| `JWT_SECRET` | *(empty — must be set)* | Signing key for session tokens |
| `RUSTSHARE_SECRET_ENCRYPTION_KEY` | *(empty — must be set)* | Encryption key for sensitive data |
| `DATABASE_URL` | *(empty — must be set)* | PostgreSQL connection |
| `RUSTFS_ENDPOINT` | `http://rustfs:9000` | Internal S3-compatible object storage |
| `RUSTFS_PUBLIC_ENDPOINT` | `http://localhost:9000` | Public-facing object storage URL |
| `RUSTFS_BUCKET` | `rustshare-files` | Object storage bucket |
| `RUSTSHARE_OBJECT_STORE_AUTO_CREATE_BUCKET` | `false` | Whether the backend should create a missing object-storage bucket at startup. Keep disabled in production and provision buckets out-of-band. |
| `RUSTSHARE_OBJECT_GC_ENABLED` | `false` | Enable asynchronous deletion of globally unreferenced `blobs/<sha256>` objects. Candidate enqueueing is always active. |
| `RUSTSHARE_OBJECT_GC_INTERVAL_SECONDS` | `300` | Worker interval; minimum 10 seconds. |
| `RUSTSHARE_OBJECT_GC_BATCH_SIZE` | `50` | Maximum candidates per tick; range 1–1000. |
| `RUSTSHARE_OBJECT_GC_GRACE_PERIOD_HOURS` | `24` | Minimum delay since the most recent candidate observation. |
| `RUSTSHARE_OBJECT_GC_MAX_ATTEMPTS` | `10` | Attempt threshold for operator-alert logging; candidates remain visible and retry safely. |
| `RUSTSHARE_OBJECT_GC_LEASE_SECONDS` | `900` | Processing lease before another worker may reclaim a candidate. |
| `RUSTSHARE_OBJECT_GC_MAX_BACKOFF_SECONDS` | `86400` | Maximum retry delay. |
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

### Dev-only overrides

Values marked `[dev-only]` in `.env.example` are safe defaults for local
development. They must be reviewed and changed before any production or
shared-environment deployment.

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

### Secure Cookies

`docker-compose.prod.yml` sets `SESSION_COOKIE_SECURE=true`. This tells the backend to emit session and CSRF cookies with the `Secure` attribute, which means browsers will only send them over HTTPS. The CSRF cookie's `Secure` flag follows the same setting as the session cookie; there is no separate `CSRF_COOKIE_SECURE` variable. **TLS termination is mandatory when using the production compose file.** If you terminate TLS at an upstream load balancer or CDN, ensure the backend still sees HTTPS requests (for example, via `X-Forwarded-Proto: https`) and that the `Secure` cookie setting matches your TLS topology.

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
   - [ ] Configure a same-host HTTPS reverse proxy in front of `127.0.0.1:8080`
   - [ ] Verify the loopback listener is not publicly reachable
   - [ ] Verify HTTPS is working and HTTP redirects to HTTPS
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

### First-start failures

The most common causes of a failed first `docker compose up -d`:

- **Backend exits immediately with a configuration validation error.**
  `docker compose logs backend` lists the offending variables, e.g.
  `JWT_SECRET must be at least 32 characters` or `DATABASE_URL is required`.
  Cause: secrets were left empty (or a weak default was kept) in `.env`.
  Fix: run `./scripts/pre-flight.sh` (or fill in the values manually as in
  [step 2](#2-generate-required-secrets-required)) and recreate the backend:
  `docker compose up -d --force-recreate backend`.

- **Backend exits with an object storage / bucket error.**
  The backend checks the `RUSTFS_BUCKET` bucket at startup and refuses to
  start when the RustFS endpoint is unreachable or the credentials are
  rejected (`AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` must match
  `RUSTFS_ROOT_USER` / `RUSTFS_ROOT_PASSWORD`). Fix: verify RustFS is healthy
  (`docker compose logs rustfs`) and that the credentials line up, then
  restart the backend.

- **No admin password was configured.**
  When `RUSTSHARE_ADMIN_PASSWORD` is empty in `.env`, the backend generates a
  random admin password at bootstrap and writes it to a secure file inside
  the backend container. Retrieve it with:
  `docker compose exec backend cat /tmp/rustshare-bootstrap-password.txt`
  (path configurable via `RUSTSHARE_BOOTSTRAP_PASSWORD_FILE`). Change the
  password after first login.

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

For real-time sync and dashboard/module live updates, nginx must also proxy the websocket endpoint explicitly:

```nginx
location = /api/ws {
    proxy_pass http://backend;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection $connection_upgrade;
    proxy_read_timeout 600s;
    proxy_send_timeout 600s;
    proxy_buffering off;
}
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

In production, the backend does **not** create a missing bucket by default. Provision `RUSTFS_BUCKET` out-of-band before startup. Local Docker Compose bootstrap defaults `RUSTSHARE_OBJECT_STORE_AUTO_CREATE_BUCKET=true`; keep it `false` for production unless you intentionally want the application to create the bucket.

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
