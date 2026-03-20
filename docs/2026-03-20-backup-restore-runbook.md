# Backup And Restore Runbook

This runbook covers the current single-stack Docker deployment of Rustshare with:
- PostgreSQL metadata
- RustFS primary object storage
- Axum backend
- nginx edge proxy

## What To Back Up

Rustshare is not recoverable from the database alone. A usable backup needs:
- PostgreSQL metadata
- RustFS object data
- deployment configuration

The repository now includes:
- `scripts/backup-stack.sh`
- `scripts/restore-stack.sh`
- `scripts/verify-backup-bundle.sh`

## Create A Backup

From the project root:

```bash
scripts/backup-stack.sh
```

Artifacts are written to `./backups/<timestamp>/`:
- `postgres.sql.gz`
- `rustfs-data.tar.gz`
- `config.tar.gz`
- `manifest.env`
- `SHA256SUMS` when `shasum` is available

Recommended operator policy:
- run at least daily for pilot and small production environments
- copy finished backup directories off the host after creation
- keep at least one verified restore point in a separate location

Verify a backup bundle before trusting it:

```bash
scripts/verify-backup-bundle.sh backups/<timestamp>
```

## Restore Procedure

Restore from a specific bundle:

```bash
scripts/restore-stack.sh backups/20260320T120000Z
```

The restore script:
1. starts `postgres` and `rustfs`
2. stops `backend` and `nginx`
3. recreates the `rustshare` database from the SQL dump
4. replaces the RustFS volume contents from the tar snapshot
5. restarts `rustfs`, `backend`, and `nginx`

Important constraints:
- treat restore as destructive for the current stack state
- do not run restore against a live production stack without a maintenance window
- restore scripts assume the current Docker Compose project owns the active volumes

## Post-Restore Verification

After restore, verify:

```bash
docker compose ps
curl -f http://localhost/health
curl -f http://localhost:8080/health
```

Then verify application-level behavior:
- log in as an admin user
- browse files
- download a known file
- open a known public share
- confirm RustFS object count and DB metadata roughly match expectations

## Failure Modes

If PostgreSQL restore fails:
- inspect container logs with `docker compose logs postgres`
- verify the dump was not truncated
- rerun restore after clearing the failed database state

If RustFS restore fails:
- inspect `docker compose logs rustfs`
- verify the `rustfs-data.tar.gz` archive can be listed with `tar -tzf`
- confirm the compose project still points to the same RustFS volume

## Current Limits

This runbook is suitable for the current Docker-based single-stack deployment.

It does not yet provide:
- point-in-time PostgreSQL recovery
- incremental object storage backups
- multi-node RustFS recovery procedures
- automated scheduled backup orchestration

## Drill Discipline

Run a periodic restore drill using:
- [2026-03-20-restore-drill-checklist.md](/Users/scolak/Projects/x/rustshare/docs/2026-03-20-restore-drill-checklist.md)
