# RustShare Troubleshooting Guide

> **Audience:** Operators deploying and maintaining RustShare  
> **Last updated:** 2026-04-29

---

## 1. Backend Won't Start

### Missing Environment Variables

**Symptoms:** Container exits immediately with an error like:

```
Error: missing environment variable: JWT_SECRET
```

**Solution:**

```bash
# Auto-generate missing secrets
cd /path/to/rustshare
./scripts/pre-flight.sh

# Verify the .env file exists and is readable
cat .env | grep -E "JWT_SECRET|RUSTSHARE_SECRET_ENCRYPTION_KEY"
```

Ensure `.env` is in the project root and Docker Compose can read it.

### Weak Secrets Rejection

**Symptoms:**

```
Error: RUSTSHARE_SECRET_ENCRYPTION_KEY is too weak or matches a known default. Refusing to start.
```

**Solution:**

```bash
# Generate a new key
openssl rand -base64 32

# Update .env and restart
sed -i '' 's/^RUSTSHARE_SECRET_ENCRYPTION_KEY=.*/RUSTSHARE_SECRET_ENCRYPTION_KEY=your-new-key/' .env
docker compose up -d --force-recreate backend
```

### Database Connection Failures

**Symptoms:**

```
Error: connection refused: postgres:5432
```

**Solution:**

1. Check that PostgreSQL is healthy:
   ```bash
   docker compose logs postgres
   docker compose ps
   ```
2. Verify `DATABASE_URL` matches the Docker service name:
   ```bash
   # Correct
   DATABASE_URL=postgres://rustshare:yourpassword@postgres:5432/rustshare
   # Wrong (uses localhost instead of service name)
   DATABASE_URL=postgres://rustshare:yourpassword@localhost:5432/rustshare
   ```
3. Ensure the backend `depends_on` condition is met (PostgreSQL must report healthy before backend starts).

### Migration Failures

**Symptoms:**

```
error: migration 20260404000002 was previously applied but has been modified
```

**Solution:**

This happens when a migration file changed after it was applied. For the specific `20260404000002_add_tenant_sharing_config.sql` migration:

```bash
# Connect to PostgreSQL
docker compose exec postgres psql -U rustshare -d rustshare

# Remove the migration record (safe to re-run; it only adds tables/columns)
DELETE FROM _sqlx_migrations WHERE version = '20260404000002';
\q

# Restart the backend
docker compose up -d --force-recreate backend
```

> **Warning:** Only do this if you understand what the migration does. For production databases, take a backup first.

---

## 2. Database Issues

### Connection Refused

**Symptoms:** API returns `500`, logs show `connection refused`.

**Common causes:**
- PostgreSQL container is not running or is still starting.
- Port `5432` is firewalled on the host.
- `DATABASE_URL` uses `localhost` instead of the Docker service name `postgres`.

**Solution:**

```bash
docker compose ps
docker compose logs postgres --tail 50
```

### Slow Queries

**Symptoms:** File listing or search is slow; high PostgreSQL CPU.

**Solution:**

1. Check for missing indexes (the migration set should create them, but verify):
   ```sql
   SELECT indexname FROM pg_indexes WHERE tablename = 'files';
   ```
2. Look for long-running queries:
   ```sql
   SELECT pid, query, state, query_start 
   FROM pg_stat_activity 
   WHERE state = 'active' AND query_start < NOW() - INTERVAL '5 seconds';
   ```
3. Consider connection pool tuning if you see pool exhaustion errors.

### Disk Space

**Symptoms:** PostgreSQL refuses writes; `could not extend file` errors.

**Solution:**

```bash
# Check volume usage
docker system df -v

# Check PostgreSQL data size
docker compose exec postgres du -sh /var/lib/postgresql/data
```

---

## 3. File Upload Failures

### Nginx Size Limits

**Symptoms:** `413 Request Entity Too Large` on upload.

**Solution:**

The included `docker/nginx.conf` sets `client_max_body_size 0` (unlimited) and `proxy_request_buffering off`. If you use a custom Nginx config or an external reverse proxy, ensure:

```nginx
client_max_body_size 0;
# or a sufficiently high value:
# client_max_body_size 10G;
```

### Storage Endpoint Misconfiguration

**Symptoms:** Upload appears to succeed but file is not stored; or `500` with S3 errors.

**Solution:**

```bash
# Verify RustFS / S3 is reachable from the backend container
docker compose exec backend curl -s http://rustfs:9000

# Check environment variables
docker compose exec backend env | grep -E "RUSTFS|AWS|STORAGE"
```

Ensure:
- `RUSTFS_ENDPOINT` uses the internal Docker hostname (`rustfs`) for the backend.
- `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` match `RUSTFS_ROOT_USER` / `RUSTFS_ROOT_PASSWORD`.

### Bucket Not Found

**Symptoms:** S3 error `NoSuchBucket`.

**Solution:**

```bash
# Check if the bucket exists
docker compose exec backend curl -s http://rustfs:9000/rustshare-files

# If not, create it via the RustFS console (http://localhost:9001)
# or AWS CLI:
aws --endpoint-url http://localhost:9000 s3 mb s3://rustshare-files
```

---

## 4. Frontend Not Loading

### "Welcome to SvelteKit" or Blank Page

**Symptoms:** You see the SvelteKit fallback page or a blank screen.

**Cause:** Stale frontend assets in the backend container.

**Solution:**

```bash
docker compose build --no-cache backend
docker compose up -d --force-recreate backend
```

### 404 on API Calls

**Symptoms:** Browser console shows `404` for `/api/v1/...`.

**Solution:**

```bash
# Check that nginx is proxying /api/ to the backend
curl -I http://localhost/api/v1/health

# If nginx returns 404, check container health
docker compose ps
docker compose logs nginx --tail 20
```

### CORS Errors

**Symptoms:** Browser blocks requests with `CORS policy` errors.

**Solution:**

Ensure `ORIGIN` matches the URL you access in the browser:

```bash
# For local development
ORIGIN=http://localhost

# For production
ORIGIN=https://files.example.com
```

Restart the backend after changing `ORIGIN`:

```bash
docker compose up -d --force-recreate backend
```

### Wrong `VITE_API_URL` or `VITE_WS_URL`

**Symptoms:** Frontend loads but shows connection errors; API calls go to the wrong host.

**Solution:**

These are **build-time** arguments in `docker/backend.Dockerfile`. Changing them requires a rebuild:

```bash
docker compose build --no-cache backend
docker compose up -d --force-recreate backend
```

For the default Docker setup, the correct values are:

```
VITE_API_URL=/api/v1
VITE_WS_URL=/api/ws
```

---

## 5. WebSocket / Real-Time Sync Not Working

### Symptoms
- No real-time toasts or updates.
- Browser console shows WebSocket connection errors.
- Replication-state badges do not update.

### Nginx Upgrade Headers

**Solution:**

Verify Nginx is forwarding WebSocket upgrade headers, and add an explicit websocket route before the generic `/api/` block:

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

location /api/ {
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection $connection_upgrade;
}
```

The included `docker/nginx.conf` now includes this explicit `/api/ws` handling. If you use a custom proxy (Traefik, Caddy, Cloudflare), ensure WebSocket support is enabled there too.

### Wrong `VITE_WS_URL`

**Solution:**

Check that `VITE_WS_URL` uses the `ws://` or `wss://` scheme and matches your public URL:

```bash
# Local Docker (unencrypted)
VITE_WS_URL=ws://localhost/api

# Production (TLS)
VITE_WS_URL=wss://files.example.com/api
```

Remember: this is a **build-time** variable. Rebuild the backend image after changing it.

### Firewall Blocking WebSocket

Some corporate firewalls block non-HTTPS ports or WebSocket traffic. Ensure port `443` (or `80` for local) is open and the proxy supports WebSocket passthrough.

---

## 6. Authentication Issues

### JWT Secret Mismatch

**Symptoms:** Users are logged out immediately, or API returns `401 Unauthorized` for valid sessions.

**Cause:** `JWT_SECRET` changed after sessions were created, or the backend and a second instance use different secrets.

**Solution:**

```bash
# Verify the current secret
grep JWT_SECRET .env

# If you rotated it intentionally, clear old sessions
docker compose exec postgres psql -U rustshare -d rustshare -c "DELETE FROM user_sessions;"
```

### OIDC Misconfiguration

**Symptoms:** Login button redirects to an error page, or callback fails with `invalid_client`.

**Solution:**

1. Verify all four OIDC variables are set:
   ```bash
   grep OIDC .env
   ```
2. Ensure `OIDC_REDIRECT_URL` exactly matches the URL registered with your IdP (including scheme and path).
3. Check the backend logs for detailed OIDC error messages:
   ```bash
   docker compose logs backend --tail 100 | grep -i oidc
   ```
4. Confirm the IdP supports the requested scopes (`openid profile email` by default).

### Admin Account Not Created

**Symptoms:** Cannot log in with the admin account.

> The admin password is set via `RUSTSHARE_ADMIN_PASSWORD` in `.env` (or auto-generated by `scripts/pre-flight.sh`). Check your `.env` file or backend logs: `docker logs rustshare-backend-1 | grep "Bootstrap admin password"`.

**Cause:** The admin account is only created on **first boot** when no users exist. If the database was seeded or restored from backup, the creation script is skipped.

**Solution:**

```bash
# Create an admin manually via the database
docker compose exec postgres psql -U rustshare -d rustshare

-- Generate an Argon2id hash externally (e.g., via a Rust script or online tool)
-- Then insert:
INSERT INTO users (id, username, email, password_hash, display_name, is_admin, storage_quota, created_at, updated_at)
VALUES (
  gen_random_uuid(),
  'admin',
  'admin@localhost',
  '<argon2id-hash>',
  'Administrator',
  true,
  10737418240,
  NOW(),
  NOW()
);
```

---

## 7. Performance Issues

### High Memory Usage

**Symptoms:** Backend container memory grows over time; OOM kills.

**Common causes:**
- Large file uploads buffered in memory (check `proxy_request_buffering off` in Nginx).
- Unbounded WebSocket connections.
- Memory leak in a custom build.

**Solution:**

```bash
# Monitor memory
docker stats backend --no-stream

# Set container memory limits in docker-compose.yml
services:
  backend:
    deploy:
      resources:
        limits:
          memory: 2G
```

### Slow File Downloads

**Symptoms:** Download speed is far below network capacity.

**Solution:**

1. Check if the backend is proxying the entire file stream. For large files, presigned-URL redirects are faster.
2. Verify Nginx `sendfile` and `tcp_nopush` are enabled.
3. Check RustFS disk I/O:
   ```bash
   docker compose exec rustfs iostat -x 1
   ```

### Database Connection Pool Exhaustion

**Symptoms:** API latency spikes; `pool timed out` errors in logs.

**Solution:**

- Increase the pool size in backend configuration (if exposed).
- Check for long-running transactions or connection leaks.
- Ensure the database can handle the connection count:
  ```sql
  SHOW max_connections;
  ```

---

## 8. Backup and Restore

### Creating a Backup

```bash
./scripts/backup-stack.sh
```

This produces a timestamped bundle in `./backups/` containing:
- `postgres.sql.gz` — logical database dump
- `rustfs-data.tar.gz` — object storage snapshot
- `config.tar.gz` — `.env` and compose files
- `manifest.env` — backup metadata

### Restoring from Backup

```bash
./scripts/restore-stack.sh ./backups/YYYY-MM-DD-HHMMSS
```

This stops the backend, restores the database, restores RustFS data, and restarts services.

### Post-Restore Verification

```bash
./scripts/post-restore-smoke.sh
```

### Running a Restore Drill

```bash
./scripts/run-restore-drill.sh
```

This automates a full restore drill in an isolated environment. Run this periodically to confirm your backups are valid.

> **See also:** [PRODUCTION_READINESS.md](PRODUCTION_READINESS.md) for RPO/RTO targets and recovery runbooks.

---

## 9. Getting Help

### Logs and Diagnostics

| Component | Command |
|-----------|---------|
| Backend | `docker compose logs backend --tail 200` |
| Nginx | `docker compose logs nginx --tail 100` |
| PostgreSQL | `docker compose logs postgres --tail 100` |
| RustFS | `docker compose logs rustfs --tail 100` |

### GitHub Resources

- **Discussions & Q&A:** [GitHub Discussions](https://github.com/kubedoio/rustshare/discussions)
- **Bug reports:** Use the issue template and include:
  - `docker compose version` output
  - Relevant log excerpts
  - Steps to reproduce
  - Output of `./scripts/final-launch-smoke.sh`

### Security Issues

Do **not** open public issues for security vulnerabilities. See [SECURITY.md](../SECURITY.md) for responsible disclosure instructions.

---

## Quick Command Reference

```bash
# Full stack restart
docker compose down && docker compose up -d

# Rebuild everything from scratch
docker compose down
docker compose up -d --build

# Check service health
docker compose ps
./scripts/final-launch-smoke.sh

# Enter database
docker compose exec postgres psql -U rustshare -d rustshare

# Check RustFS console
curl http://localhost:9000
# Console: http://localhost:9001 (credentials from RUSTFS_ROOT_USER / RUSTFS_ROOT_PASSWORD in .env)

# Verify metadata parity (if using metadata v2)
./scripts/verify-metadata.sh parity
```

---

## See Also

- [Deployment Guide](DEPLOYMENT.md)
- [Production Readiness](PRODUCTION_READINESS.md)
- [Security Model](security-model.md)
- [Architecture Overview](architecture.md)
