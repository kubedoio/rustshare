# RustShare Mail Phase 4 — Archive Jobs

**Goal:** Add folder/date-range IMAP archive jobs with incremental sync, retention, retry, and failure handling.

**Refs #147**

## Status

- Phase 1 (`.eml` upload / artifact folder / attachment promotion) — merged.
- Phase 2 (linking mail to notes/files) — merged.
- Phase 3 (IMAP account CRUD, selected-message import) — merged.
- This document covers Phase 4 only. Phases 5 and 6 are out of scope here.

## Architecture

Archive jobs reuse the existing `mail_import_jobs` table with `source_mode = 'imap_archive'`. The table already allows this value in its `CHECK` constraint, and the worker already polls it. We add archive-specific columns (`archive_since`, `archive_before`, `last_uid_validity`, `last_imported_uid`, `retention_days`, `retry_count`, `max_retries`) as nullable fields. The worker detects the source mode and either runs the existing selected-import path or the new archive path.

The archive path fetches UIDs in the folder/date range, imports each message via the existing `.eml`/MIME import path, and persists `last_uid_validity` and `last_imported_uid` so the next run resumes without re-importing everything. A retention policy soft-deletes archived messages whose `imported_at` is older than the job's `retention_days`.

Retry uses `retry_count` and exponential backoff derived from `updated_at`. Jobs transition through `pending → running → completed/failed/cancelled`. A separate cleanup pass is future work.

## Design decisions

1. **Extend `mail_import_jobs`, do not create a new table.**
   The existing table already distinguishes `imap_selected` and `imap_archive` via `source_mode`. Adding nullable columns is a smaller, safer migration than a new table plus duplicated worker polling logic.

2. **Archive jobs are owner-scoped and tenant-scoped.**
   Only the owner (and workspace admins) can create, list, cancel, or delete archive jobs. This matches Phase 3 import jobs.

3. **Incremental sync state lives on the job row.**
   `last_uid_validity` and `last_imported_uid` are updated after each successful UID import. If `UIDVALIDITY` changes, the client resets `last_imported_uid` to 0 and re-imports from the start of the date range.

4. **Retention is soft-delete only in this phase.**
   `retention_days` causes `mail_messages` rows (and their parts/attachments via `ON DELETE CASCADE`) to be soft-deleted when `imported_at < now() - retention_days`. Hard deletion is a follow-up issue.

5. **Retry with exponential backoff.**
   `retry_count` increments on failure. The worker skips a failed job until `updated_at + backoff(retry_count)` has passed, up to `max_retries`, after which the job is marked `failed`. `max_retries` defaults to 3.

6. **No admin-only archive jobs in this phase.**
   Archive jobs follow the same ownership model as selected imports. Admin archive visibility is future work.

## Data model changes

### Migration: `backend/migrations/20260709150001_mail_archive_jobs.sql`

Add columns to `mail_import_jobs`:

```sql
ALTER TABLE mail_import_jobs
    ADD COLUMN archive_since TIMESTAMPTZ,
    ADD COLUMN archive_before TIMESTAMPTZ,
    ADD COLUMN last_uid_validity BIGINT,
    ADD COLUMN last_imported_uid BIGINT,
    ADD COLUMN retention_days INTEGER,
    ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN max_retries INTEGER NOT NULL DEFAULT 3;

CREATE INDEX idx_mail_import_jobs_source_mode ON mail_import_jobs(source_mode) WHERE deleted_at IS NULL;
```

### Domain type

Extend `MailImportJob` in `backend/crates/core/src/domain/mail_account.rs` with the new columns. Add a constructor `MailImportJob::new_archive(...)` and keep `MailImportJob::new` for selected imports.

## Service layer

Extend `MailService` in `backend/server/src/services/mail_service.rs`:

- `create_archive_job(...)` — validates the account, date range, and retention, inserts a pending archive job, emits `MailArchiveJobCreated`.
- `list_archive_jobs(account_id, caller)` — lists non-deleted archive jobs for an account that the caller owns.
- `get_archive_job(job_id, caller)` — fetches a single archive job if owned by caller.
- `cancel_archive_job(job_id, caller)` — sets status to `cancelled` if currently `pending` or `running`, emits `MailArchiveJobCancelled`.
- `soft_delete_archive_job(job_id, caller)` — soft-deletes the job row, emits `MailArchiveJobDeleted`.
- `process_archive_job(job)` — opens IMAP session, fetches UIDs in folder/date range greater than `last_imported_uid`, imports each via `import_eml`, updates progress and sync state, applies retention soft-delete, emits start/complete/fail events.

## Background worker

Extend `backend/server/src/mail_import_worker.rs`:

- The existing `claim_next_pending_mail_import_job` query is updated to respect backoff and source mode.
- After claiming a job, the worker calls `service.process_import_job(&job)` for `imap_selected` and `service.process_archive_job(&job)` for `imap_archive`.
- Add `reset_stale_running_mail_archive_jobs` (or extend the existing query to include archive jobs).

## REST API

Add routes in `backend/server/src/routes.rs` and handlers in `backend/server/src/handlers/mail.rs`:

- `POST /api/v1/mail/accounts/{account_id}/archive-jobs`
- `GET /api/v1/mail/accounts/{account_id}/archive-jobs`
- `GET /api/v1/mail/archive-jobs/{job_id}`
- `PATCH /api/v1/mail/archive-jobs/{job_id}/cancel`
- `DELETE /api/v1/mail/archive-jobs/{job_id}`

All endpoints require the `mail` module to be enabled and the caller to be authenticated.

## Audit events

Add event types in `backend/crates/core/src/events/types.rs`:

- `MailArchiveJobCreated`
- `MailArchiveJobStarted`
- `MailArchiveJobCompleted`
- `MailArchiveJobFailed`
- `MailArchiveJobCancelled`
- `MailArchiveJobDeleted`

Events are appended via `EventStore::append_in_tx` and published via the broadcaster.

## Tests

- Unit tests for `MailImportJob::new_archive` and backoff calculation.
- Integration test for creating, listing, cancelling, and deleting an archive job.
- Worker integration test with a mock IMAP client if a real server is unavailable.

## Follow-up issues

After Phase 4 merges, create:

- mail: add retention hard-delete job for archived messages
- mail: expose archive job metrics and alerts

## Risks

- Extending `mail_import_jobs` with archive columns makes the table wider for selected imports. The columns are nullable and have no performance impact on the selected-import path.
- Retention soft-delete runs during archive job processing. For large mailboxes this could be slow; consider a dedicated background job in the future.
