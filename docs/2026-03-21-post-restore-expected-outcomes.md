# Post-Restore Expected Outcomes

Date: 2026-03-21

## Purpose

Define what “successful restore” means at the application level after `restore-stack.sh` or `run-restore-drill.sh`.

Use this with:

- [Backup And Restore Runbook](/Users/scolak/Projects/x/rustshare/docs/2026-03-20-backup-restore-runbook.md)
- [Restore Drill Checklist](/Users/scolak/Projects/x/rustshare/docs/2026-03-20-restore-drill-checklist.md)

## Infrastructure Outcomes

Expected:

- `postgres` is healthy
- `rustfs` is healthy
- `backend` is healthy
- `nginx` is healthy

Validation:

- `docker compose ps`
- `curl -f http://localhost/health`
- `curl -f http://localhost:8080/health`

## Auth Outcomes

Expected:

- browser login works for a known admin user
- `/api/v1/me` resolves correctly after login
- logout works

Failure meaning:

- restore is not operationally complete even if containers are healthy

## File-Browsing Outcomes

Expected:

- root folder listing loads
- at least one known folder or file from the backup can be opened
- file metadata and object content are consistent enough for normal browsing

Failure meaning:

- DB metadata and object storage may be out of sync

## Share Outcomes

Expected:

- a known public share link opens
- a known internal share remains visible to the intended user

Failure meaning:

- restore may have succeeded structurally while product behavior is still broken

## Download Outcomes

Expected:

- a known file can be downloaded
- the downloaded content matches expected size or checksum if available

Failure meaning:

- object storage restore may be incomplete or presign/download behavior may be broken

## Replication Outcomes

Expected:

- replication summary endpoints respond
- required targets are visible
- no unexpected permanent failed state appears immediately after restore

Failure meaning:

- restored system may not be durable even if basic app routes work

## Minimum Success Definition

A restore is considered successful only when:

- infrastructure health checks pass
- auth works
- file browsing works
- a known file downloads
- a known share works
- replication visibility works

If any one of these fails, the restore must be treated as incomplete.
