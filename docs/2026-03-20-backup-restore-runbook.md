# Backup and Restore Runbook

> **Audience:** Operators responsible for RustShare data protection and recovery  
> **Scope:** Docker Compose deployments using the bundled backup and restore scripts

---

## Recovery Objectives

| Objective | Target | Notes |
|-----------|--------|-------|
| **RPO** (Recovery Point Objective) | < 24 hours | Daily automated backups; consider more frequent snapshots for high-churn environments |
| **RTO** (Recovery Time Objective) | < 2 hours | Full stack restore including verification smoke test |

---

## Prerequisites

- Docker and Docker Compose (v2 or v1) installed on the target host
- `postgres` and `rustfs` services running and healthy before backup
- `.env` file stored separately in a secrets manager (it is **not** included in backup bundles)
- Sufficient disk space for the backup bundle (typically 1.2–1.5× the size of the database + object storage)

---

## 1. Creating a Backup

### Manual Backup

Run from the project root:

```bash
./scripts/backup-stack.sh
```

This creates a timestamped directory under `./backups/` (e.g., `./backups/20260611T184231Z/`).

To use a custom backup root:

```bash
./scripts/backup-stack.sh /mnt/rustshare-backups
```

### What Gets Created

| File | Description |
|------|-------------|
| `postgres.sql.gz` | Logical dump of the PostgreSQL database |
| `rustfs-data.tar.gz` | Snapshot of the RustFS data volume |
| `config.tar.gz` | Docker Compose files, scripts, and key documentation |
| `manifest.env` | Backup metadata (timestamp, git commit, service names) |
| `SHA256SUMS` | Integrity checksums (if `shasum` is available) |

### Environment Overrides

| Variable | Default | Description |
|----------|---------|-------------|
| `POSTGRES_SERVICE` | `postgres` | Compose service name for Postgres |
| `POSTGRES_DB` | `rustshare` | Database name to dump |
| `POSTGRES_USER` | `rustshare` | Database user for `pg_dump` |
| `RUSTFS_SERVICE` | `rustfs` | Compose service name for RustFS |

### Automated Backups (Cron)

Daily backups at 02:00 UTC:

```bash
0 2 * * * cd /opt/rustshare && ./scripts/backup-stack.sh /mnt/backups/rustshare >> /var/log/rustshare-backup.log 2>&1
```

---

## 2. Verifying a Backup Bundle

Before any restore operation, verify the bundle integrity:

```bash
./scripts/verify-backup-bundle.sh /mnt/backups/rustshare/20260611T184231Z
```

### Checks Performed

1. All required artifacts exist (`postgres.sql.gz`, `rustfs-data.tar.gz`, `config.tar.gz`, `manifest.env`)
2. `postgres.sql.gz` is valid gzip
3. `rustfs-data.tar.gz` and `config.tar.gz` are valid tar archives
4. `manifest.env` contains required keys (`BACKUP_TIMESTAMP`, `GIT_COMMIT`)
5. `SHA256SUMS` matches when present

**Exit code:** `0` if valid, non-zero if any check fails.

---

## 3. Restoring from Backup

### Full Stack Restore

```bash
./scripts/restore-stack.sh /mnt/backups/rustshare/20260611T184231Z
```

### Step-by-Step Breakdown

1. **Start core services** — brings up `postgres` and `rustfs` if not running, then waits for them to be healthy.
2. **Stop application traffic** — stops `backend` and `nginx` to prevent writes during restore.
3. **Restore PostgreSQL** — terminates active connections, drops the existing database, recreates it, and replays the logical dump.
4. **Restore RustFS data** — stops `rustfs`, wipes the data volume, and extracts the archived snapshot.
5. **Restart services** — brings up `rustfs`, `backend`, and `nginx`, waiting for each to become healthy.

### Environment Overrides

| Variable | Default | Description |
|----------|---------|-------------|
| `POSTGRES_SERVICE` | `postgres` | Postgres service name |
| `POSTGRES_DB` | `rustshare` | Database name |
| `POSTGRES_USER` | `rustshare` | Database user |
| `RUSTFS_SERVICE` | `rustfs` | RustFS service name |
| `BACKEND_SERVICE` | `backend` | Backend service name |
| `EDGE_SERVICE` | `nginx` | Reverse-proxy service name |

### Partial Restore (Object Storage Only)

If PostgreSQL is intact but RustFS data is lost:

```bash
docker compose stop backend rustfs

# Identify the RustFS volume
RUSTFS_VOLUME=$(docker inspect --format '{{range .Mounts}}{{if eq .Destination "/data"}}{{.Name}}{{end}}{{end}}' $(docker compose ps -q rustfs))

# Wipe and restore
docker run --rm -v "${RUSTFS_VOLUME}:/data" alpine:3.21 \
  sh -lc 'rm -rf /data/* /data/.[!.]* /data/..?* 2>/dev/null || true; tar -xzf - -C /data' \
  < /mnt/backups/rustshare/20260611T184231Z/rustfs-data.tar.gz

docker compose up -d rustfs backend
```

---

## 4. Post-Restore Validation

Run the smoke test after every restore:

```bash
ADMIN_EMAIL=admin@yourdomain.com \
ADMIN_PASSWORD=<admin-password> \
  ./scripts/post-restore-smoke.sh
```

### What It Verifies

1. **Password login** — authenticates as the admin user and confirms a `rustshare_session` cookie is created.
2. **Authenticated session** — fetches `/api/v1/me` and validates the email matches.
3. **Root folder listing** — requests `/api/v1/folders/root/contents` and confirms `files` and `folders` keys exist.
4. **Public share flow** — discovers or uses a configured public share token, creates a share session, and verifies file download or folder listing.

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `BASE_URL` | `http://localhost` | Deployment base URL |
| `API_BASE_URL` | `${BASE_URL}/api/v1` | API base path |
| `ADMIN_EMAIL` | `admin@localhost` | Admin email for login test |
| `ADMIN_PASSWORD` | *(empty)* | Admin password |
| `PUBLIC_SHARE_TOKEN` | *(optional)* | Specific share token to test |
| `PUBLIC_SHARE_PASSWORD` | *(optional)* | Password for protected shares |
| `ALLOW_SKIP_PUBLIC_SHARE` | `false` | Allow test to pass without a public share |

---

## 5. Restore Drill (Non-Destructive)

Prove backups are recoverable without touching production:

```bash
./scripts/run-restore-drill.sh /mnt/backups/rustshare/20260611T184231Z
```

### What It Does

1. Verifies the backup bundle.
2. Spins up an isolated Docker Compose project (`rustshare-restore-drill`) on alternate ports (`18080`, `18081`).
3. Restores the backup into the isolated project.
4. Runs the post-restore smoke test against the isolated stack.
5. Tears down the drill stack (unless `DRILL_KEEP_STACK=true`).
6. Writes a report to `./restore-drill-reports/`.

### Environment Overrides

| Variable | Default | Description |
|----------|---------|-------------|
| `DRILL_PROJECT_NAME` | `rustshare-restore-drill` | Isolated Compose project name |
| `DRILL_COMPOSE_FILE` | `docker-compose.restore-drill.yml` | Compose file for drill |
| `DRILL_BASE_URL` | `http://localhost:18080` | URL of the drill stack |
| `DRILL_KEEP_STACK` | `false` | Keep the drill stack running after test |
| `DRILL_REPORT_DIR` | `./restore-drill-reports` | Where to write the report |

**Recommended schedule:** monthly, or after any significant infrastructure change.

---

## 6. Incident Response Scenarios

### Scenario A: Database Corruption

1. Stop application traffic: `docker compose stop backend nginx`
2. Identify the most recent valid backup.
3. Run `./scripts/restore-stack.sh <backup_dir>`.
4. Run `./scripts/post-restore-smoke.sh`.
5. If metadata looks inconsistent, run `./scripts/repair-metadata.sh --execute all`.

### Scenario B: Complete Host Failure

1. Provision a new host with Docker and Docker Compose.
2. Clone or copy the RustShare project files.
3. Restore `.env` from your secrets manager.
4. Run `./scripts/restore-stack.sh <backup_dir>`.
5. Run `./scripts/post-restore-smoke.sh`.
6. Update DNS or load-balancer targets.

---

## See Also

- [Backup and Restore Guide](backup-restore.md)
- [Production Readiness](PRODUCTION_READINESS.md)
