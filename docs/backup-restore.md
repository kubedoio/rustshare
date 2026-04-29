# Backup and Restore Guide

> **Audience:** Operators and administrators maintaining RustShare deployments  
> **Scope:** Docker Compose deployments using the bundled backup scripts

---

## What Gets Backed Up

The `scripts/backup-stack.sh` script creates a timestamped bundle containing:

| Artifact | Description |
|----------|-------------|
| `postgres.sql.gz` | Logical dump of the PostgreSQL database |
| `rustfs-data.tar.gz` | Snapshot of the RustFS data volume |
| `config.tar.gz` | Docker Compose files, scripts, and key documentation |
| `manifest.env` | Backup metadata (timestamp, git commit, service names) |
| `SHA256SUMS` | Checksums for integrity verification (if `shasum` is available) |

This covers the three critical layers of a RustShare deployment:

1. **Database** — all metadata, users, permissions, shares, and folder structures
2. **Object storage** — the actual file content and versions stored in RustFS
3. **Configuration** — compose files, environment references, and operational docs

> **Note:** The `.env` file is **not** included in the backup bundle. Keep it in a separate secrets manager or secure location, since it contains passwords and encryption keys.

---

## Creating a Backup

### Manual Backup

```bash
./scripts/backup-stack.sh
```

This produces a timestamped directory under `./backups/` (e.g., `./backups/20260429T081322Z/`).

You can specify a custom backup root:

```bash
./scripts/backup-stack.sh /mnt/rustshare-backups
```

### Requirements

- The `postgres` and `rustfs` services must be running.
- Docker Compose (v2 or v1) must be available.
- The script must be run from the project root or a subdirectory.

### Environment Overrides

| Variable | Default | Description |
|----------|---------|-------------|
| `POSTGRES_SERVICE` | `postgres` | Name of the Postgres service in Compose |
| `POSTGRES_DB` | `rustshare` | Database name to dump |
| `POSTGRES_USER` | `rustshare` | Database user for `pg_dump` |
| `RUSTFS_SERVICE` | `rustfs` | Name of the RustFS service |

### Automated Backups (Cron)

Add to your crontab for daily backups at 2 AM:

```bash
0 2 * * * cd /opt/rustshare && ./scripts/backup-stack.sh /mnt/backups/rustshare >> /var/log/rustshare-backup.log 2>&1
```

For retention, pair this with a cleanup script (see [Retention Policy](#retention-policy)).

---

## Restoring from Backup

### `restore-stack.sh`

```bash
./scripts/restore-stack.sh ./backups/20260429T081322Z
```

This script performs the following steps:

1. Starts `postgres` and `rustfs` if not already running.
2. Stops `backend` and `nginx` to prevent traffic during restore.
3. Terminates active database connections and drops the existing database.
4. Recreates the database and replays the logical dump.
5. Clears the RustFS data volume and extracts the snapshot.
6. Restarts `rustfs`, `backend`, and `nginx`, waiting for each to become healthy.

### Environment Overrides

| Variable | Default | Description |
|----------|---------|-------------|
| `POSTGRES_SERVICE` | `postgres` | Postgres service name |
| `POSTGRES_DB` | `rustshare` | Database name |
| `POSTGRES_USER` | `rustshare` | Database user |
| `RUSTFS_SERVICE` | `rustfs` | RustFS service name |
| `BACKEND_SERVICE` | `backend` | Backend service name |
| `EDGE_SERVICE` | `nginx` | Edge/reverse-proxy service name |

### Post-Restore Verification

After restoring, run the smoke test to verify login, sessions, and file access:

```bash
./scripts/post-restore-smoke.sh
```

This checks:

- Password login succeeds and creates a session cookie.
- Authenticated session returns the correct user profile.
- Root folder listing returns files and folders.
- Public share flows work (if a share token is available).

Configure it via environment variables:

```bash
BASE_URL=http://localhost \
API_BASE_URL=http://localhost/api/v1 \
ADMIN_EMAIL=admin@localhost \
ADMIN_PASSWORD=your-admin-password \
  ./scripts/post-restore-smoke.sh
```

| Variable | Default | Description |
|----------|---------|-------------|
| `BASE_URL` | `http://localhost` | Base URL of the deployment |
| `API_BASE_URL` | `${BASE_URL}/api/v1` | API base path |
| `ADMIN_EMAIL` | `admin@localhost` | Admin email for login test |
| `ADMIN_PASSWORD` | *(empty)* | Admin password |
| `PUBLIC_SHARE_TOKEN` | *(optional)* | Specific share token to test |
| `PUBLIC_SHARE_PASSWORD` | *(optional)* | Password for protected shares |
| `ALLOW_SKIP_PUBLIC_SHARE` | `false` | Allow test to pass without a public share |

---

## Restore Drills

Run a full restore drill in an isolated Docker Compose project to prove your backups are valid without touching production:

```bash
./scripts/run-restore-drill.sh ./backups/20260429T081322Z
```

What it does:

1. Verifies the backup bundle integrity.
2. Spins up a disposable Compose project on alternate ports (`18080`, `18081`).
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

**Recommended schedule:** Run a restore drill monthly, or after any significant infrastructure change.

---

## Metadata Repair Tools

If you encounter metadata inconsistencies—such as missing folder children, broken parent references, or index corruption—use the repair scripts. Both default to **dry-run mode**; you must pass `--execute` to make changes.

### `repair-metadata.sh`

Repairs inconsistencies in the metadata system via the admin API.

```bash
# Dry run — see what would be repaired
./scripts/repair-metadata.sh all

# Execute repairs
./scripts/repair-metadata.sh --execute all

# Repair a specific folder
./scripts/repair-metadata.sh --execute repair folder <folder-id>

# Fix parent references for a folder
./scripts/repair-metadata.sh --execute fix-parent <folder-id>

# Sync a specific entity from PostgreSQL to RustFS
./scripts/repair-metadata.sh --execute sync file <file-id>
```

| Option | Description |
|--------|-------------|
| `--admin-url <url>` | Admin API base URL (default: `http://localhost:8080/api/admin/metadata`) |
| `--api-key <key>` | API key for authentication |
| `--execute` | Perform actual repairs |
| `--yes` / `-y` | Skip confirmation prompts |

**When to use:**
- After a restore, if folder listings look incomplete.
- If user reports files "missing" from a folder but they exist in search.
- After any manual database intervention.

### `rebuild-metadata-index.sh`

Rebuilds secondary indexes from source objects. Use this when indexes are corrupted or after data recovery.

```bash
# Dry run
./scripts/rebuild-metadata-index.sh all

# Rebuild all indexes
./scripts/rebuild-metadata-index.sh --execute all

# Rebuild a specific folder's children index
./scripts/rebuild-metadata-index.sh --execute folder-children <folder-id>

# Rebuild all user folder indexes
./scripts/rebuild-metadata-index.sh --execute user-folders

# Rebuild file version indexes
./scripts/rebuild-metadata-index.sh --execute file-versions <file-id>

# Check current index status
./scripts/rebuild-metadata-index.sh status
```

**When to use:**
- Queries return stale or missing results despite valid source data.
- After a partial restore or replication conflict resolution.
- When advised by the consistency checker.

> **Warning:** Index rebuilds can be resource-intensive on large datasets. Run during low-traffic periods.

---

## Disaster Recovery Playbook

### Scenario 1: Database Corruption

1. Stop the backend and nginx: `docker compose stop backend nginx`
2. Identify the most recent valid backup directory.
3. Run `./scripts/restore-stack.sh <backup_dir>`.
4. Run `./scripts/post-restore-smoke.sh` to verify.
5. If metadata looks inconsistent, run `./scripts/repair-metadata.sh --execute all`.

### Scenario 2: Object Storage Data Loss

1. If PostgreSQL is intact but RustFS data is lost or corrupted:
   - Stop the backend: `docker compose stop backend`
   - Restore only the RustFS volume from `rustfs-data.tar.gz`.
   - Restart services.
2. If you have database records pointing to missing objects, users will see errors on download. Re-upload the files or restore from a full backup.

### Scenario 3: Complete Server Failure

1. Provision a new host with Docker and Docker Compose.
2. Clone or copy the RustShare project files.
3. Restore `.env` from your secrets manager.
4. Run `./scripts/restore-stack.sh <backup_dir>`.
5. Run `./scripts/post-restore-smoke.sh`.
6. Update DNS or load balancer to point to the new host.

### Scenario 4: Accidental File Deletion

RustShare does not yet have user-facing trash/restore. If a file was accidentally deleted:

1. Restore from the most recent backup to a temporary drill environment:
   ```bash
   DRILL_KEEP_STACK=true ./scripts/run-restore-drill.sh <backup_dir>
   ```
2. Log into the drill instance and download the missing file.
3. Re-upload it to the production instance.
4. Tear down the drill stack when done.

---

## Retention Policy Recommendations

A sensible default for production:

| Tier | Frequency | Retention | Storage |
|------|-----------|-----------|---------|
| Daily | Every 24h | 7 days | Local or NFS |
| Weekly | Every Sunday | 4 weeks | Remote (S3, rsync) |
| Monthly | First of month | 12 months | Cold storage |

### Implementation Example

```bash
#!/bin/bash
# /opt/rustshare/backup-with-retention.sh

BACKUP_ROOT=/mnt/backups/rustshare
DAILY_RETENTION=7
WEEKLY_RETENTION=28
MONTHLY_RETENTION=365

cd /opt/rustshare

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
# Example: sync to S3-compatible storage
aws s3 sync /mnt/backups/rustshare s3://my-backup-bucket/rustshare/ \
  --exclude '*' --include '*/postgres.sql.gz' --include '*/rustfs-data.tar.gz' \
  --storage-class GLACIER
```

### Testing Your Backups

A backup you haven't restored is a hope, not a plan. Run a restore drill at least monthly:

```bash
./scripts/run-restore-drill.sh $(ls -d ./backups/20* | tail -1)
```

Keep the last three drill reports for audit purposes.

---

## See Also

- [Troubleshooting Guide](troubleshooting.md)
- [Production Readiness](PRODUCTION_READINESS.md)
- [Deployment Guide](DEPLOYMENT.md)
- [Upgrading Guide](upgrading.md)
