# Backup and Restore Runbook

> **Audience:** Operators responsible for RustShare data protection and recovery  
> **Scope:** Docker Compose deployments using the bundled backup and restore scripts  
> **RPO target:** < 24 hours  
> **RTO target:** < 2 hours

---

## 1. What Gets Backed Up

The `scripts/backup-stack.sh` script creates a timestamped bundle containing:

| Artifact | Description |
|----------|-------------|
| `postgres.sql.gz` | Logical dump of the PostgreSQL database |
| `rustfs-data.tar.gz` | Snapshot of the RustFS data volume |
| `config.tar.gz` | Docker Compose files, scripts, and key documentation |
| `manifest.env` | Backup metadata (timestamp, git commit, service names) |
| `SHA256SUMS` | Integrity checksums (if `shasum` is available) |

This covers the three critical layers of a RustShare deployment:

1. **Database** — metadata, users, permissions, shares, folder structures, audit logs.
2. **Object storage** — actual file content and versions stored in RustFS/S3.
3. **Configuration** — compose files, environment references, and operational docs.

> **Important:** The `.env` file is **not** included in the backup bundle. Keep it in a separate secrets manager or secure location, since it contains passwords and encryption keys.

---

## 2. Creating a Backup

### 2.1 Manual Backup

Run from the project root:

```bash
./scripts/backup-stack.sh
```

This creates a timestamped directory under `./backups/` (e.g., `./backups/20260617T142000Z/`).

To use a custom backup root:

```bash
./scripts/backup-stack.sh /mnt/rustshare-backups
```

### 2.2 Environment Overrides

| Variable | Default | Description |
|----------|---------|-------------|
| `POSTGRES_SERVICE` | `postgres` | Compose service name for Postgres |
| `POSTGRES_DB` | `rustshare` | Database name to dump |
| `POSTGRES_USER` | `rustshare` | Database user for `pg_dump` |
| `RUSTFS_SERVICE` | `rustfs` | Compose service name for RustFS |

### 2.3 Automated Backups (Cron)

Daily backups at 02:00 UTC:

```bash
0 2 * * * cd /opt/rustshare && ./scripts/backup-stack.sh /mnt/backups/rustshare >> /var/log/rustshare-backup.log 2>&1
```

Pair automated backups with a retention policy (see [Retention](#6-retention-policy)).

---

## 3. Verifying a Backup Bundle

Before any restore operation, verify the bundle integrity:

```bash
./scripts/verify-backup-bundle.sh /mnt/backups/rustshare/20260617T142000Z
```

### Checks Performed

1. All required artifacts exist (`postgres.sql.gz`, `rustfs-data.tar.gz`, `config.tar.gz`, `manifest.env`).
2. `postgres.sql.gz` is valid gzip.
3. `rustfs-data.tar.gz` and `config.tar.gz` are valid tar archives.
4. `manifest.env` contains required keys (`BACKUP_TIMESTAMP`, `GIT_COMMIT`).
5. `SHA256SUMS` matches when present.

**Exit code:** `0` if valid, non-zero if any check fails.

---

## 4. Restoring from Backup

### 4.1 Full Stack Restore

```bash
./scripts/restore-stack.sh /mnt/backups/rustshare/20260617T142000Z
```

The script performs the following steps:

1. Starts `postgres` and `rustfs` if not running, then waits for them to be healthy.
2. Stops `backend` and `nginx` to prevent traffic during restore.
3. Terminates active database connections, drops the existing database, recreates it, and replays the logical dump.
4. Stops `rustfs`, wipes the data volume, and extracts the archived snapshot.
5. Restarts `rustfs`, `backend`, and `nginx`, waiting for each to become healthy.

### 4.2 Partial Restore (Object Storage Only)

If PostgreSQL is intact but RustFS data is lost or corrupted:

```bash
docker compose stop backend rustfs

# Identify the RustFS volume
RUSTFS_VOLUME=$(docker inspect --format '{{range .Mounts}}{{if eq .Destination "/data"}}{{.Name}}{{end}}{{end}}' $(docker compose ps -q rustfs))

# Wipe and restore
docker run --rm -v "${RUSTFS_VOLUME}:/data" alpine:3.21 \
  sh -lc 'rm -rf /data/* /data/.[!.]* /data/..?* 2>/dev/null || true; tar -xzf - -C /data' \
  < /mnt/backups/rustshare/20260617T142000Z/rustfs-data.tar.gz

docker compose up -d rustfs backend
```

> If database records point to objects that no longer exist, users will see errors on download. Re-upload the files or restore from a full backup.

### 4.3 Environment Overrides

| Variable | Default | Description |
|----------|---------|-------------|
| `POSTGRES_SERVICE` | `postgres` | Postgres service name |
| `POSTGRES_DB` | `rustshare` | Database name |
| `POSTGRES_USER` | `rustshare` | Database user |
| `RUSTFS_SERVICE` | `rustfs` | RustFS service name |
| `BACKEND_SERVICE` | `backend` | Backend service name |
| `EDGE_SERVICE` | `nginx` | Reverse-proxy service name |

---

## 5. Post-Restore Validation

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

## 6. Restore Drill (Non-Destructive)

Prove backups are recoverable without touching production:

```bash
./scripts/run-restore-drill.sh /mnt/backups/rustshare/20260617T142000Z
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
| `ADMIN_EMAIL` | `admin@localhost` | Admin email for smoke test |
| `ADMIN_PASSWORD` | *(empty)* | Admin password |

**Recommended schedule:** monthly, or after any significant infrastructure change.

---

## 7. Metadata Repair After Restore

If folder listings, search results, or parent/child relationships look inconsistent after a restore, use the admin metadata endpoints (`/admin/metadata/verify/*` and `/admin/metadata/repair`) or contact support. The legacy standalone repair/rebuild scripts have been removed.

---

## 8. Retention Policy

A sensible default for production:

| Tier | Frequency | Retention | Storage |
|------|-----------|-----------|---------|
| Daily | Every 24h | 7 days | Local or NFS |
| Weekly | Every Sunday | 4 weeks | Remote (S3, rsync) |
| Monthly | First of month | 12 months | Cold storage |

### Example Cleanup Script

```bash
#!/bin/bash
# /opt/rustshare/backup-with-retention.sh

BACKUP_ROOT=/mnt/backups/rustshare
DAILY_RETENTION=7
WEEKLY_RETENTION=28
MONTHLY_RETENTION=365

cd /opt/rustshare || exit 1

# Create backup
./scripts/backup-stack.sh "$BACKUP_ROOT"

# Clean old daily backups
find "$BACKUP_ROOT" -maxdepth 1 -type d -name '20*' -mtime +$DAILY_RETENTION -exec rm -rf {} +
```

Add to cron:

```bash
0 2 * * * /opt/rustshare/backup-with-retention.sh >> /var/log/rustshare-backup.log 2>&1
```

### Off-Site Copies

For true disaster recovery, replicate weekly backups off-site:

```bash
aws s3 sync /mnt/backups/rustshare s3://my-backup-bucket/rustshare/ \
  --exclude '*' --include '*/postgres.sql.gz' --include '*/rustfs-data.tar.gz' \
  --storage-class GLACIER
```

---

## 9. Incident Scenarios

### 9.1 Database Corruption

1. Stop application traffic: `docker compose stop backend nginx`
2. Identify the most recent valid backup.
3. Run `./scripts/restore-stack.sh <backup_dir>`.
4. Run `./scripts/post-restore-smoke.sh`.
5. If metadata looks inconsistent, use the admin metadata repair endpoints or restore from an earlier backup.

### 9.2 Object Storage Data Loss

1. Stop the backend: `docker compose stop backend`
2. Restore only the RustFS volume from `rustfs-data.tar.gz` (see [Partial Restore](#42-partial-restore-object-storage-only)).
3. Restart services.
4. If database records point to missing objects, re-upload files or restore from a full backup.

### 9.3 Complete Host Failure

1. Provision a new host with Docker and Docker Compose.
2. Clone or copy the RustShare project files.
3. Restore `.env` from your secrets manager.
4. Run `./scripts/restore-stack.sh <backup_dir>`.
5. Run `./scripts/post-restore-smoke.sh`.
6. Update DNS or load-balancer targets.

### 9.4 Accidental File Deletion

RustShare does not yet have user-facing trash/restore. To recover a deleted file:

1. Restore from the most recent backup to a temporary drill environment:
   ```bash
   DRILL_KEEP_STACK=true ./scripts/run-restore-drill.sh <backup_dir>
   ```
2. Log into the drill instance and download the missing file.
3. Re-upload it to the production instance.
4. Tear down the drill stack when done.

---

## See Also

- [Backup and Restore Guide](../backup-restore.md)
- [Production Readiness](../PRODUCTION_READINESS.md)
- [Deployment Guide](../DEPLOYMENT.md)
- [Troubleshooting Guide](../troubleshooting.md)
- [Security Incident Runbook](security-incident.md)
