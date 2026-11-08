# Backup and Restore Runbook

> **Audience:** Operators responsible for Elembra data protection and recovery
> **Scope:** Docker Compose deployments using the bundled backup and restore scripts  
> **RPO target:** < 24 hours  
> **RTO target:** < 2 hours (must be confirmed by a real isolated restore drill)

---

## 1. What Gets Backed Up

The `scripts/backup-stack.sh` script creates a timestamped bundle containing:

| Artifact | Description |
|----------|-------------|
| `postgres.sql.gz` | Logical dump of the PostgreSQL database |
| `rustfs-data.tar.gz` | Snapshot of the RustFS data volume |
| `buzz-postgres.sql.gz` | Dedicated Buzz PostgreSQL dump (with `--with-chat`) |
| `buzz-rustfs-data.tar.gz` | Dedicated Buzz RustFS snapshot (with `--with-chat`) |
| `config.tar.gz` | Docker Compose files, scripts, and key documentation |
| `manifest.env` | Backup metadata (timestamp, git commit, service names) |
| `SHA256SUMS` | Integrity checksums (if `shasum` is available) |

This covers the three critical layers of a RustShare deployment:

1. **Database** — metadata, users, permissions, shares, folder structures, audit logs.
2. **Object storage** — actual file content and versions stored in RustFS/S3.
3. **Configuration** — compose files, environment references, and operational docs.

> **Important:** The local bundle created directly by `backup-stack.sh` omits
> `.env` and `.elembra/chat.env`. The encrypted archive made by
> `elembra-offsite-backup.sh` includes them, plus the RustFS KMS override when
> present. Never upload the local plaintext bundle.

For the bundled Chat deployment, the offsite workflow uses `--with-chat` to
include Buzz's dedicated database and object store in the encrypted archive.

---

## 2. Creating a Backup

### 2.1 Manual Backup

Run from the project root:

```bash
./scripts/backup-stack.sh --with-chat
```

This creates a timestamped directory under `./backups/` (e.g., `./backups/20260617T142000Z/`).

To use a custom backup root:

```bash
./scripts/backup-stack.sh --with-chat /mnt/rustshare-backups
```

### 2.2 Environment Overrides

| Variable | Default | Description |
|----------|---------|-------------|
| `POSTGRES_SERVICE` | `postgres` | Compose service name for Postgres |
| `POSTGRES_DB` | `rustshare` | Database name to dump |
| `POSTGRES_USER` | `rustshare` | Database user for `pg_dump` |
| `RUSTFS_SERVICE` | `rustfs` | Compose service name for RustFS |

### 2.3 Automated Backups (Cron)

The cron examples below are local staging backups only. They are not a
host-independent recovery copy, do not include `.env` or `.elembra/chat.env`,
and must not be treated as disaster recovery.

Daily backups at 02:00 UTC:

```bash
0 2 * * * cd /opt/rustshare && ./scripts/backup-stack.sh /mnt/backups/rustshare >> /var/log/rustshare-backup.log 2>&1
```

For a bundled Chat Alpha, the scheduled command must include
`--with-chat`:

```bash
0 2 * * * cd /opt/rustshare && ./scripts/backup-stack.sh --with-chat /mnt/backups/rustshare >> /var/log/rustshare-backup.log 2>&1
```

Use the encrypted S3 workflow below for the supported Elembra/Buzz off-host
copy. Do not clean backup directories with a broad `find ... -exec rm` command.

### 2.4 Encrypted S3 backups for Elembra and Buzz

`scripts/elembra-offsite-backup.sh` creates and verifies a full `--with-chat`
bundle, includes deployment secrets and the local RustFS override when present,
encrypts the archive to an age **public recipient**, uploads it under the fixed
`elembra/` prefix, and uploads a SHA-256 sidecar. AWS CLI v2 requests S3
SHA-256 upload checksums; the S3 service validates the checksum during upload.
The script has no delete permission. The bucket lifecycle rule expires objects
after 90 days and removes abandoned multipart uploads after 7 days. See the
[AWS CLI checksum documentation](https://docs.aws.amazon.com/cli/latest/topic/s3-faq.html)
and the tested rule at `ops/s3/elembra-lifecycle.json`.

#### Operator inputs

Before activation, an operator must provide all of the following. Do not put a
backup endpoint, recipient, writer secret, or alert URL into the repository.

| Setting | Required value and custody |
|---------|-----------------------------|
| `ELEMBRA_BACKUP_S3_ENDPOINT` | HTTPS S3-compatible endpoint in a separate host/account/failure domain from Elembra. |
| `ELEMBRA_BACKUP_S3_BUCKET` | Dedicated private bucket; the script writes only to `elembra/`. |
| `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY` | Least-privilege uploader credential for `PutObject` and multipart upload actions under `elembra/*`; no delete or bucket-administration permission. Store only in a root-owned mode-0600 host environment file. |
| `ELEMBRA_BACKUP_AGE_RECIPIENT` | Operator-provided age public recipient. The matching private identity stays encrypted and independently held off-host. |
| `ELEMBRA_BACKUP_HEARTBEAT_URL` | HTTPS ping URL from an external check/alert service. Configure that service to alert on failed and missed daily pings. |
| `ELEMBRA_BACKUP_STAGING_DIR` | Optional private staging path with enough free space; use tmpfs or encrypted storage because the source bundle exists briefly before age encryption. |

The decryption identity and S3 read/recovery credential are not installed on the
application host. The host credential can write ciphertext but cannot retrieve
or decrypt backups. Keep the alert service's notification recipients in that
external service, not on Elembra.

#### Configure and enable

1. Install Docker Compose, AWS CLI v2, age, curl, and the backup tools already
   listed by `scripts/backup-stack.sh` on the host. The offsite service runs as
   root because the existing volume snapshot helper uses the Docker socket.
2. Create `/etc/elembra/offsite-backup.env` as root with mode `0600`. Populate
   only the operator-supplied settings above and, if needed,
   `AWS_DEFAULT_REGION`. Do not add a private age identity or S3 recovery key.
3. From a separate storage-admin workstation, create the private bucket and
   apply the lifecycle policy. The administrative credential must stay off the
   application host:

   ```bash
   aws --endpoint-url "$ELEMBRA_BACKUP_S3_ENDPOINT" s3api put-bucket-lifecycle-configuration \
     --bucket "$ELEMBRA_BACKUP_S3_BUCKET" \
     --lifecycle-configuration file://ops/s3/elembra-lifecycle.json
   ```

4. Install `ops/systemd/elembra-offsite-backup.service` and
   `ops/systemd/elembra-offsite-backup.timer` under
   `/etc/systemd/system/`. Change `/opt/elembra` in the service to the actual
   deployment checkout path, then enable the timer:

   ```bash
   systemctl daemon-reload
   systemctl enable --now elembra-offsite-backup.timer
   systemctl start elembra-offsite-backup.service
   systemctl status elembra-offsite-backup.service
   ```

The timer runs daily at 02:00 UTC with a randomized delay. The script pings the
external monitor after upload and sends its `/fail` endpoint on runtime errors.
Check the service journal, the external monitor, and the S3 object list after
the first run. A timer configuration alone is not proof that a backup reached
the remote bucket.

The existing snapshot helper copies live RustFS volumes and dumps the two
databases separately. It does not provide an atomic snapshot across services.
Schedule a quiet write window and verify application-level integrity during the
independent restore drill before treating this procedure as qualified recovery.

#### Independent restore

On a separate restore host, use the independently held S3 read credential and
age private identity. Download the archive and sidecar, verify the ciphertext
hash, decrypt into a private staging directory, then follow [Full Stack
Restore](#41-full-stack-restore) in an isolated Compose project:

Set `BACKUP_NAME` to the timestamped object name without `.tar.gz.age`, and
set `OFF_HOST_AGE_IDENTITY` to the independently held private identity file.
The S3 endpoint, bucket, and read credential must come from the operator's
recovery record, not from the application host.

```bash
umask 077
aws --endpoint-url "$ELEMBRA_BACKUP_S3_ENDPOINT" s3 cp \
  "s3://$ELEMBRA_BACKUP_S3_BUCKET/elembra/$BACKUP_NAME.tar.gz.age" .
aws --endpoint-url "$ELEMBRA_BACKUP_S3_ENDPOINT" s3 cp \
  "s3://$ELEMBRA_BACKUP_S3_BUCKET/elembra/$BACKUP_NAME.tar.gz.age.sha256" .
sha256sum --check "$BACKUP_NAME.tar.gz.age.sha256"
age --decrypt --identity "$OFF_HOST_AGE_IDENTITY" \
  --output "$BACKUP_NAME.tar.gz" "$BACKUP_NAME.tar.gz.age"
mkdir -m 700 restored
tar -xzf "$BACKUP_NAME.tar.gz" -C restored
```

The encrypted archive contains Elembra and Buzz databases, both RustFS volume
snapshots, deployment files, `.env`, `.elembra/chat.env`, and the RustFS KMS
override when present. Restore those identities only in the isolated recovery
environment. Browser-local Chat signing keys are not in server backups; each
user must retain and test their own encrypted browser-key export separately.
Do not report independent recovery as passed until this restore uses only the
off-host object and independently held recovery material.

Test the automation against disposable RustFS and PostgreSQL services with:

```bash
./scripts/test-elembra-offsite-backup.sh
```

The test creates and removes only its uniquely named containers and volumes.
It validates lifecycle retention, S3 checksum upload/readback, age decryption,
database and object-volume contents, and success/failure monitor pings. It does
not prove recovery from the production backup target.

---

## 3. Verifying a Backup Bundle

Before any restore operation, verify the bundle integrity:

```bash
./scripts/verify-backup-bundle.sh /mnt/backups/rustshare/20260617T142000Z
```

### Checks Performed

1. All required artifacts exist (`postgres.sql.gz`, `rustfs-data.tar.gz`, `config.tar.gz`, `manifest.env`).
2. Chat bundles contain both Buzz artifacts; partial Buzz bundles fail validation.
3. PostgreSQL dumps are valid gzip archives.
4. RustFS archives and `config.tar.gz` are valid tar archives.
5. `manifest.env` contains required keys (`BACKUP_TIMESTAMP`, `GIT_COMMIT`).
6. `SHA256SUMS` matches when present.

**Exit code:** `0` if valid, non-zero if any check fails.

---

## 4. Restoring from Backup

### 4.1 Full Stack Restore

```bash
./scripts/restore-stack.sh --with-chat /mnt/backups/rustshare/20260617T142000Z
```

The script performs the following steps:

1. Starts `postgres` and `rustfs` if not running, then waits for them to be healthy.
2. Stops `backend` and `nginx` to prevent traffic during restore.
3. Terminates active database connections, drops the existing database, recreates it, and replays the logical dump.
4. Stops `rustfs`, wipes the data volume, and extracts the archived snapshot.
5. Restarts `rustfs`, `backend`, and `nginx`, waiting for each to become healthy.

With `--with-chat`, the command also restores `buzz-postgres` and the
dedicated `buzz-rustfs` volume, then waits for the Buzz relay and managed
observer. Restore the external `.env` and `.elembra/chat.env` secrets before
running it; never regenerate deployment identities during recovery.

The command without `--with-chat` is intentionally core-only and cannot
restore bundled Chat state.

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
2. Checks that no containers, volumes, or networks already use the selected project name, then spins up an isolated Docker Compose project (`rustshare-restore-drill`) on alternate ports (`18080`, `18081`).
3. Restores the core Elembra backup into the isolated project.
4. Runs the post-restore smoke test against the isolated stack.
5. Tears down the drill stack (unless `DRILL_KEEP_STACK=true`).
6. Writes a report to `./restore-drill-reports/`.

The report records `RESTORE_DRILL_STATUS=passed` or `failed`. Failure
paths — including pre-flight failures where the drill never started, such
as another drill holding the project lock — also write a `failed` report
with details, so interrupted or refused drills leave explicit evidence.

The drill refuses to start if resources with the selected Compose project label already exist. It never clears a previous stack automatically: inspect those resources and explicitly remove them only after confirming they belong to this disposable drill project. This protects existing Docker data from an accidental `down -v`.

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

The generic drill compose file is intentionally core-only. For a bundled Chat
Alpha, perform the complete `restore-stack.sh --with-chat` rehearsal on a
separate clean host/project with the Alpha Compose profile and restored
external secrets; it must include Buzz PostgreSQL, dedicated Buzz RustFS,
relay, and observer checks.

---

## 7. Metadata Repair After Restore

If folder listings, search results, or parent/child relationships look inconsistent after a restore, use the admin metadata endpoints (`/admin/metadata/verify/*` and `/admin/metadata/repair`) or contact support. The legacy standalone repair/rebuild scripts have been removed.

---

## 8. Retention Policy

The Elembra offsite timer writes daily backups. The bucket lifecycle file keeps
current objects for 90 days, noncurrent versions for 90 days, and aborts
incomplete multipart uploads after 7 days. Change this policy only through the
storage administrator, then verify it on the destination. The application-host
writer credential has no object-delete or lifecycle-administration permission.

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
