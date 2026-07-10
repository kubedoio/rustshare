# RustShare Mail Phase 4 — Archive Jobs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add folder/date-range IMAP archive jobs with incremental sync, retention, retry, and failure handling.

**Architecture:** Extend the existing `mail_import_jobs` table with archive-specific nullable columns. Reuse the existing worker polling infrastructure and branch on `source_mode` to call a new `process_archive_job` path. Imported archive messages use the existing `.eml`/MIME import path with `MailSourceMode::ImapArchive`.

**Tech Stack:** Rust, axum, sqlx, async-imap, chrono, serde, utoipa, tokio.

**Refs #147**

---

## Conventions

- Branch: `feat/mail-phase4-archive-jobs`.
- Base branch: `main` (must include merged Phase 3).
- Worktree: `.worktrees/feat/mail-phase4-archive-jobs`.
- Tenant-scoped tables with nil UUID default, soft-delete, `ON DELETE CASCADE` to `users`.
- All commits signed-off (`git commit -s`) and reference `#147`.
- Run `cargo sqlx prepare --workspace` after query changes.

---

## File map

| File | Responsibility |
|---|---|
| `backend/migrations/20260709150001_mail_archive_jobs.sql` | Add archive columns to `mail_import_jobs`. |
| `backend/crates/core/src/domain/mail_account.rs` | Extend `MailImportJob` domain type and add `new_archive` constructor. |
| `backend/crates/core/src/events/types.rs` | Add archive job event types and payloads. |
| `backend/crates/storage/src/metadata.rs` | Add MetadataStore methods for archive jobs. |
| `backend/server/src/services/imap_client.rs` | Add `fetch_uids_by_date_range` session method. |
| `backend/server/src/services/mail_service.rs` | Add archive job service methods and `process_archive_job`. |
| `backend/server/src/mail_import_worker.rs` | Branch on `source_mode` to dispatch archive jobs. |
| `backend/server/src/handlers/mail.rs` | Add archive job request/response types and handlers. |
| `backend/server/src/routes.rs` | Register archive job routes. |
| `backend/server/src/openapi.rs` | Register new archive handlers and schemas. |
| `backend/crates/core/tests/mail_archive_job_domain_test.rs` | Unit tests for archive job domain type. |
| `backend/tests/mail_archive_job_test.rs` | Integration test for create/cancel/delete archive job. |
| `docs/plans/2026-07-09-mail-phase4-archive-jobs.md` | Design doc (already written). |

---

## Task 1: Database migration for archive jobs

**Files:**
- Create: `backend/migrations/20260709150001_mail_archive_jobs.sql`

- [ ] **Step 1: Write the migration**

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

- [ ] **Step 2: Apply migration locally**

Run:
```bash
cd backend
DATABASE_URL=postgres://scolak@localhost/rustshare cargo sqlx migrate run
```

Expected: migration applies successfully.

- [ ] **Step 3: Commit**

```bash
git add backend/migrations/20260709150001_mail_archive_jobs.sql
git commit -s -m "feat(mail): add archive job columns to mail_import_jobs

Refs #147"
```

---

## Task 2: Extend domain type for archive jobs

**Files:**
- Modify: `backend/crates/core/src/domain/mail_account.rs`

- [ ] **Step 1: Add archive fields to `MailImportJob`**

Add after `source_uidvalidity`:

```rust
pub archive_since: Option<DateTime<Utc>>,
pub archive_before: Option<DateTime<Utc>>,
pub last_uid_validity: Option<i64>,
pub last_imported_uid: Option<i64>,
pub retention_days: Option<i32>,
pub retry_count: i32,
pub max_retries: i32,
```

- [ ] **Step 2: Update `MailImportJob::new` to default new fields**

Set all new archive fields to `None` and `retry_count` to `0`, `max_retries` to `3`.

- [ ] **Step 3: Add `MailImportJob::new_archive` constructor**

```rust
#[allow(clippy::too_many_arguments)]
pub fn new_archive(
    tenant_id: Uuid,
    owner_id: UserId,
    account_id: MailAccountId,
    folder_name: String,
    archive_since: Option<DateTime<Utc>>,
    archive_before: Option<DateTime<Utc>>,
    retention_days: Option<i32>,
    max_retries: i32,
) -> Self {
    let now = Utc::now();
    Self {
        id: Uuid::new_v4(),
        tenant_id,
        owner_id,
        account_id,
        source_mode: "imap_archive".to_string(),
        folder_name,
        selected_uids: None,
        source_uidvalidity: None,
        archive_since,
        archive_before,
        last_uid_validity: None,
        last_imported_uid: None,
        retention_days,
        retry_count: 0,
        max_retries,
        status: MailImportJobStatus::Pending.into(),
        total_messages: 0,
        processed_messages: 0,
        failed_messages: 0,
        last_error: None,
        started_at: None,
        completed_at: None,
        deleted_at: None,
        created_at: now,
        updated_at: now,
    }
}
```

- [ ] **Step 4: Commit**

```bash
git add backend/crates/core/src/domain/mail_account.rs
git commit -s -m "feat(mail): extend MailImportJob for archive jobs

Refs #147"
```

---

## Task 3: Add archive job audit event types

**Files:**
- Modify: `backend/crates/core/src/events/types.rs`

- [ ] **Step 1: Add event variants**

In the `EventType` enum, add after `MailImported`:

```rust
MailArchiveJobCreated,
MailArchiveJobStarted,
MailArchiveJobCompleted,
MailArchiveJobFailed,
MailArchiveJobCancelled,
MailArchiveJobDeleted,
```

- [ ] **Step 2: Add payload structs**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MailArchiveJobCreatedPayload {
    pub job_id: MailImportJobId,
    pub account_id: MailAccountId,
    pub folder_name: String,
    pub owner_id: UserId,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MailArchiveJobStartedPayload {
    pub job_id: MailImportJobId,
    pub account_id: MailAccountId,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MailArchiveJobCompletedPayload {
    pub job_id: MailImportJobId,
    pub account_id: MailAccountId,
    pub processed_messages: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MailArchiveJobFailedPayload {
    pub job_id: MailImportJobId,
    pub account_id: MailAccountId,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MailArchiveJobCancelledPayload {
    pub job_id: MailImportJobId,
    pub account_id: MailAccountId,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MailArchiveJobDeletedPayload {
    pub job_id: MailImportJobId,
    pub account_id: MailAccountId,
}
```

- [ ] **Step 3: Commit**

```bash
git add backend/crates/core/src/events/types.rs
git commit -s -m "feat(mail): add archive job event types

Refs #147"
```

---

## Task 4: MetadataStore archive job methods

**Files:**
- Modify: `backend/crates/storage/src/metadata.rs`

- [ ] **Step 1: Update `create_mail_import_job` insert to include new columns**

Add the new columns to the `INSERT INTO mail_import_jobs (...)` query and bind the new fields.

- [ ] **Step 2: Update `get_mail_import_job` and `list_mail_import_jobs_by_owner` selects**

Add the new columns to the SELECT lists so the struct binds correctly.

- [ ] **Step 3: Add `claim_next_pending_mail_import_job` backoff filter**

Update the CTE to skip jobs where `retry_count >= max_retries` and where `updated_at + interval '1 second' * (2 ^ retry_count)` is in the future. Use `GREATEST(retry_count, 0)` to avoid negative exponents.

```sql
AND (
    retry_count < max_retries
    AND updated_at <= now() - (interval '1 second' * (2 ^ GREATEST(retry_count, 0)))
)
```

- [ ] **Step 4: Add `update_mail_archive_job_progress`**

```rust
pub async fn update_mail_archive_job_progress(
    &self,
    id: MailImportJobId,
    processed: i32,
    failed: i32,
    last_uid_validity: Option<i64>,
    last_imported_uid: Option<i64>,
    last_error: Option<&str>,
) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE mail_import_jobs
        SET processed_messages = $1,
            failed_messages = $2,
            last_uid_validity = $3,
            last_imported_uid = $4,
            last_error = $5,
            updated_at = now()
        WHERE id = $6 AND deleted_at IS NULL
        "#,
        processed,
        failed,
        last_uid_validity,
        last_imported_uid,
        last_error,
        id
    )
    .execute(&self.pool)
    .await?;
    Ok(())
}
```

- [ ] **Step 5: Add `soft_delete_mail_archive_job`**

```rust
pub async fn soft_delete_mail_archive_job(
    &self,
    id: MailImportJobId,
    owner_id: UserId,
) -> Result<bool> {
    let rows = sqlx::query!(
        r#"
        UPDATE mail_import_jobs
        SET deleted_at = now(), updated_at = now()
        WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL
          AND source_mode = 'imap_archive'
        "#,
        id,
        owner_id
    )
    .execute(&self.pool)
    .await?
    .rows_affected();
    Ok(rows > 0)
}
```

- [ ] **Step 6: Add retention soft-delete method**

```rust
pub async fn apply_archive_retention(
    &self,
    job_id: MailImportJobId,
    owner_id: UserId,
    retention_days: i32,
) -> Result<u64> {
    let rows = sqlx::query!(
        r#"
        UPDATE mail_messages
        SET deleted_at = now(), updated_at = now()
        WHERE owner_id = $1
          AND source_mode = 'imap_archive'
          AND imported_at < now() - (interval '1 day' * $2)
          AND deleted_at IS NULL
        "#,
        owner_id,
        retention_days as f64
    )
    .execute(&self.pool)
    .await?
    .rows_affected();
    Ok(rows)
}
```

- [ ] **Step 7: Commit**

```bash
git add backend/crates/storage/src/metadata.rs
git commit -s -m "feat(mail): add MetadataStore archive job queries

Refs #147"
```

---

## Task 5: IMAP client date-range UID fetch

**Files:**
- Modify: `backend/server/src/services/imap_client.rs`

- [ ] **Step 1: Add `fetch_uids_by_date_range` method to `ImapSession`**

```rust
pub async fn fetch_uids_by_date_range(
    &mut self,
    folder: &str,
    since: Option<DateTime<Utc>>,
    before: Option<DateTime<Utc>>,
) -> Result<(Option<u32>, Vec<u32>), ImapError> {
    self.select_folder(folder).await?;
    let uid_validity = self
        .session
        .uid_validity()
        .ok_or_else(|| ImapError::CommandFailed("Missing UIDVALIDITY".to_string()))?;

    let mut criteria = Vec::new();
    criteria.push("ALL".to_string());
    if let Some(since) = since {
        criteria.push(format!("SINCE {}", since.format("%d-%b-%Y")));
    }
    if let Some(before) = before {
        criteria.push(format!("BEFORE {}", before.format("%d-%b-%Y")));
    }
    let query = criteria.join(" ");

    let uids = self
        .session
        .uid_search(query)
        .await
        .map_err(|e| ImapError::CommandFailed(format!("UID SEARCH failed: {e}")))?;

    let mut uids: Vec<u32> = uids.into_iter().collect();
    uids.sort_unstable();
    Ok((Some(uid_validity), uids))
}
```

- [ ] **Step 2: Commit**

```bash
git add backend/server/src/services/imap_client.rs
git commit -s -m "feat(mail): add IMAP date-range UID search

Refs #147"
```

---

## Task 6: MailService archive job methods

**Files:**
- Modify: `backend/server/src/services/mail_service.rs`

- [ ] **Step 1: Add `create_archive_job`**

```rust
#[allow(clippy::too_many_arguments)]
pub async fn create_archive_job(
    &self,
    tenant_id: Uuid,
    owner_id: UserId,
    account_id: MailAccountId,
    folder_name: String,
    archive_since: Option<DateTime<Utc>>,
    archive_before: Option<DateTime<Utc>>,
    retention_days: Option<i32>,
    max_retries: Option<i32>,
) -> Result<MailImportJob, MailError> {
    // Verify account exists and belongs to caller
    let account = self
        .metadata_store
        .get_mail_account(account_id, owner_id)
        .await
        .map_err(|e| MailError::Database(e.to_string()))?
        .ok_or(MailError::AccountNotFound(account_id))?;

    if account.tenant_id != tenant_id {
        return Err(MailError::PermissionDenied);
    }

    let max_retries = max_retries.unwrap_or(3).max(0);
    let job = MailImportJob::new_archive(
        tenant_id,
        owner_id,
        account_id,
        folder_name,
        archive_since,
        archive_before,
        retention_days,
        max_retries,
    );

    self.metadata_store
        .create_mail_import_job(&job)
        .await
        .map_err(|e| MailError::Database(e.to_string()))?;

    let event = Event::new(
        EventType::MailArchiveJobCreated,
        job.id,
        AggregateType::MailImportJob,
        serde_json::to_value(MailArchiveJobCreatedPayload {
            job_id: job.id,
            account_id: job.account_id,
            folder_name: job.folder_name.clone(),
            owner_id: job.owner_id,
        })?,
        owner_id,
    );
    self.event_store
        .append(&event, &self.broadcaster)
        .await
        .map_err(|e| MailError::Database(e.to_string()))?;

    Ok(job)
}
```

- [ ] **Step 2: Add `list_archive_jobs`, `get_archive_job`, `cancel_archive_job`, `delete_archive_job`**

`list_archive_jobs` filters `list_mail_import_jobs_by_owner` to `source_mode = "imap_archive"`.

`get_archive_job` fetches by id and verifies `source_mode = "imap_archive"`.

`cancel_archive_job` updates status to `cancelled` if currently `pending` or `running`, emits `MailArchiveJobCancelled`.

`delete_archive_job` calls `soft_delete_mail_archive_job` and emits `MailArchiveJobDeleted`.

- [ ] **Step 3: Add `process_archive_job`**

Branch from `process_import_job` pattern:

1. Guard status.
2. Load account, decrypt password.
3. Mark running, emit `MailArchiveJobStarted`.
4. Connect and select folder.
5. Determine UID range using `fetch_uids_by_date_range`.
6. If `last_uid_validity` matches and `last_imported_uid` is set, skip UIDs <= last_imported_uid. If UIDVALIDITY changed, reset `last_imported_uid`.
7. Loop UIDs, fetch RFC822, import via `import_raw_source(..., MailSourceMode::ImapArchive, ...)`.
8. After each UID, update progress and sync state, refresh updated_at.
9. Apply retention soft-delete.
10. Mark completed or failed, emit corresponding event.

- [ ] **Step 4: Commit**

```bash
git add backend/server/src/services/mail_service.rs
git commit -s -m "feat(mail): add archive job service methods

Refs #147"
```

---

## Task 7: Worker dispatches archive jobs

**Files:**
- Modify: `backend/server/src/mail_import_worker.rs`

- [ ] **Step 1: Branch on source_mode**

In the job processing spawn block, replace:

```rust
if let Err(e) = service.process_import_job(&job).await { ... }
```

with:

```rust
let result = match job.source_mode.as_str() {
    "imap_archive" => service.process_archive_job(&job).await,
    _ => service.process_import_job(&job).await,
};
if let Err(e) = result { ... }
```

- [ ] **Step 2: Commit**

```bash
git add backend/server/src/mail_import_worker.rs
git commit -s -m "feat(mail): dispatch archive jobs in import worker

Refs #147"
```

---

## Task 8: Archive job REST handlers

**Files:**
- Modify: `backend/server/src/handlers/mail.rs`
- Modify: `backend/server/src/handlers/mod.rs` (if error mapping needs changes)

- [ ] **Step 1: Add request/response types**

```rust
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
pub struct CreateMailArchiveJobRequest {
    pub folder_name: String,
    pub archive_since: Option<DateTime<Utc>>,
    pub archive_before: Option<DateTime<Utc>>,
    pub retention_days: Option<i32>,
    pub max_retries: Option<i32>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailArchiveJobResponse {
    pub id: Uuid,
    pub account_id: Uuid,
    pub folder_name: String,
    pub source_mode: String,
    pub status: String,
    pub archive_since: Option<DateTime<Utc>>,
    pub archive_before: Option<DateTime<Utc>>,
    pub last_uid_validity: Option<i64>,
    pub last_imported_uid: Option<i64>,
    pub retention_days: Option<i32>,
    pub retry_count: i32,
    pub max_retries: i32,
    pub total_messages: i32,
    pub processed_messages: i32,
    pub failed_messages: i32,
    pub last_error: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

- [ ] **Step 2: Add response helper**

```rust
fn archive_job_to_response(job: MailImportJob) -> MailArchiveJobResponse {
    MailArchiveJobResponse { ... }
}
```

- [ ] **Step 3: Add handlers**

`create_mail_archive_job`, `list_mail_archive_jobs`, `get_mail_archive_job`, `cancel_mail_archive_job`, `delete_mail_archive_job`.

Return `StatusCode::ACCEPTED` for create, `StatusCode::OK` for cancel/delete.

- [ ] **Step 4: Commit**

```bash
git add backend/server/src/handlers/mail.rs
git commit -s -m "feat(mail): add archive job handlers

Refs #147"
```

---

## Task 9: Register archive routes

**Files:**
- Modify: `backend/server/src/routes.rs`

- [ ] **Step 1: Add routes**

```rust
.route("/api/v1/mail/accounts/{account_id}/archive-jobs", get(handlers::mail::list_mail_archive_jobs).post(handlers::mail::create_mail_archive_job))
.route("/api/v1/mail/archive-jobs/{job_id}", get(handlers::mail::get_mail_archive_job).delete(handlers::mail::delete_mail_archive_job))
.route("/api/v1/mail/archive-jobs/{job_id}/cancel", patch(handlers::mail::cancel_mail_archive_job))
```

- [ ] **Step 2: Commit**

```bash
git add backend/server/src/routes.rs
git commit -s -m "feat(mail): register archive job routes

Refs #147"
```

---

## Task 10: OpenAPI registration

**Files:**
- Modify: `backend/server/src/openapi.rs`

- [ ] **Step 1: Add handlers to the mail_paths tuple**

Add the five new handlers to the existing `mail_paths` macro/tuple list.

- [ ] **Step 2: Add schemas**

Add `CreateMailArchiveJobRequest` and `MailArchiveJobResponse` to the schemas list.

- [ ] **Step 3: Commit**

```bash
git add backend/server/src/openapi.rs
git commit -s -m "feat(mail): register archive job OpenAPI schemas

Refs #147"
```

---

## Task 11: Domain unit tests

**Files:**
- Create: `backend/crates/core/tests/mail_archive_job_domain_test.rs`

- [ ] **Step 1: Write tests**

```rust
use chrono::Utc;
use rustshare_core::domain::{MailImportJob, MailSourceMode};
use uuid::Uuid;

#[test]
fn archive_job_defaults_to_imap_archive_source_mode() {
    let job = MailImportJob::new_archive(
        Uuid::nil(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        "INBOX".to_string(),
        None,
        None,
        Some(365),
        3,
    );
    assert_eq!(job.source_mode, "imap_archive");
    assert_eq!(job.retry_count, 0);
    assert_eq!(job.max_retries, 3);
    assert_eq!(job.retention_days, Some(365));
}
```

- [ ] **Step 2: Run tests**

```bash
cd backend
SQLX_OFFLINE=true cargo test -p rustshare-core --test mail_archive_job_domain_test
```

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add backend/crates/core/tests/mail_archive_job_domain_test.rs
git commit -s -m "test(mail): add archive job domain tests

Refs #147"
```

---

## Task 12: Integration test for archive job lifecycle

**Files:**
- Create: `backend/tests/mail_archive_job_test.rs`

- [ ] **Step 1: Write integration test**

Use `setup_test_env()` from `contracts::common`. Create a mail account, create an archive job, verify it is persisted with `source_mode = 'imap_archive'`, cancel it, delete it. This test does not require a real IMAP server.

```rust
#[tokio::test]
async fn archive_job_lifecycle_create_cancel_delete() {
    let ctx = setup_test_env().await;
    let user = ctx.create_test_user("archiveowner").await;
    let account = ctx
        .mail_service()
        .create_account(ctx.tenant_id, user.id, "Test".to_string(), "imap.example.com".to_string(), 993, "user".to_string(), "pass".to_string(), rustshare_core::domain::MailTlsMode::Tls)
        .await
        .unwrap();

    let job = ctx
        .mail_service()
        .create_archive_job(ctx.tenant_id, user.id, account.id, "INBOX".to_string(), None, None, Some(90), None)
        .await
        .unwrap();

    assert_eq!(job.source_mode, "imap_archive");
    assert_eq!(job.retention_days, Some(90));

    ctx.mail_service().cancel_archive_job(ctx.tenant_id, user.id, job.id).await.unwrap();
    let cancelled = ctx.mail_service().get_archive_job(ctx.tenant_id, user.id, job.id).await.unwrap();
    assert_eq!(cancelled.status, "cancelled");

    ctx.mail_service().delete_archive_job(ctx.tenant_id, user.id, job.id).await.unwrap();
}
```

- [ ] **Step 2: Run test compile**

```bash
cd backend
SQLX_OFFLINE=true cargo test -p rustshare-server --test mail_archive_job_test --no-run
```

Expected: compiles.

- [ ] **Step 3: Commit**

```bash
git add backend/tests/mail_archive_job_test.rs
git commit -s -m "test(mail): add archive job lifecycle integration test

Refs #147"
```

---

## Task 13: SQLx offline metadata

**Files:**
- Modify: `backend/.sqlx/*.json` (generated)

- [ ] **Step 1: Regenerate SQLx metadata**

With the local database running and migrations applied:

```bash
cd backend
DATABASE_URL=postgres://scolak@localhost/rustshare SQLX_OFFLINE=true cargo sqlx prepare --workspace
```

- [ ] **Step 2: Verify metadata**

```bash
DATABASE_URL=postgres://scolak@localhost/rustshare SQLX_OFFLINE=true cargo sqlx prepare --workspace --check
```

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add backend/.sqlx/
git commit -s -m "chore(mail): regenerate sqlx offline metadata for archive jobs

Refs #147"
```

---

## Task 14: Verification

- [ ] **Step 1: Format check**

```bash
cargo fmt --check
```

Expected: no formatting issues.

- [ ] **Step 2: Clippy**

```bash
cd backend
SQLX_OFFLINE=true cargo clippy --all-features -- -D warnings
```

Expected: no warnings.

- [ ] **Step 3: Workspace check**

```bash
SQLX_OFFLINE=true cargo check --workspace
SQLX_OFFLINE=true cargo check --tests
```

Expected: both pass.

- [ ] **Step 4: Unit tests**

```bash
DATABASE_URL=postgres://scolak@localhost/rustshare SQLX_OFFLINE=true cargo test --workspace --lib
```

Expected: pass (DB required for some tests).

- [ ] **Step 5: Integration test**

```bash
SQLX_OFFLINE=true cargo test -p rustshare-server --test mail_archive_job_test --no-run
```

Expected: compiles.

---

## Self-review checklist

- [ ] Spec coverage: archive jobs, incremental sync, retention, retry, audit events, REST API, tests all have tasks.
- [ ] No placeholders: every step has code or exact commands.
- [ ] Type consistency: `MailImportJob` fields, method names, and event payloads match across tasks.
