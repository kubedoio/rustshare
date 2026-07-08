# RustShare Mail Phase 3 — IMAP Selected Import Implementation Plan

> **For Kimi:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` to implement this plan task-by-task in the worktree `.worktrees/mail-phase3-imap-import`.

**Goal:** Add IMAP account management and selected-message import to the RustShare Mail module, building on top of Phase 1/2 (`feat/mail-phase2-linking`).

**Architecture:** Encrypted IMAP account records in PostgreSQL, a tokio-rustls/async-imap client wrapper, an import-job table processed by a background poller, and REST endpoints for accounts/folders/messages/import-jobs. Imported messages reuse the existing `.eml`/MIME import path so they become normal `MailMessage` artifacts.

**Tech Stack:** Rust, axum, sqlx, async-imap (runtime-tokio), tokio-rustls, webpki-roots, AES-256-GCM secret encryption already provided by `rustshare_crypto`.

**Refs #147**

---

## Conventions to follow

- Branch: `feat/mail-phase3-imap-import` (already checked out in this worktree).
- Base branch: `feat/mail-phase2-linking`.
- Keep `MailSourceMode::ImapSelected` for imported messages and `MailImportJob` rows.
- Tenant-scoped tables with nil-UUID default, soft-delete columns, `ON DELETE CASCADE` to users.
- All new queries require `cargo sqlx prepare --workspace` after changes.
- All commits must be signed-off (`git commit -s`) and reference `#147`.
- Do **not** implement IMAP archive jobs, search, AI/RAG, SMTP, or webmail reply/forward in this phase.

---

## Task 1: Database migrations for IMAP accounts and import jobs

**Files:**
- Create: `backend/migrations/20260708160002_create_mail_accounts_table.sql`
- Create: `backend/migrations/20260708160003_create_mail_import_jobs_table.sql`

**Step 1: Write `mail_accounts` migration**

```sql
CREATE TABLE mail_accounts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    host TEXT NOT NULL,
    port INTEGER NOT NULL DEFAULT 993,
    username TEXT NOT NULL,
    password_enc TEXT NOT NULL,
    tls_mode VARCHAR(20) NOT NULL DEFAULT 'tls' CHECK (tls_mode IN ('tls', 'starttls', 'none')),
    is_enabled BOOLEAN NOT NULL DEFAULT true,
    last_error TEXT,
    last_connected_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_mail_accounts_tenant_id ON mail_accounts(tenant_id);
CREATE INDEX idx_mail_accounts_owner_id ON mail_accounts(owner_id);
CREATE INDEX idx_mail_accounts_deleted_at ON mail_accounts(deleted_at);
```

**Step 2: Write `mail_import_jobs` migration**

```sql
CREATE TABLE mail_import_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    account_id UUID NOT NULL REFERENCES mail_accounts(id) ON DELETE CASCADE,
    source_mode VARCHAR(50) NOT NULL DEFAULT 'imap_selected' CHECK (source_mode IN ('imap_selected', 'imap_archive')),
    folder_name TEXT NOT NULL,
    selected_uids BIGINT[],
    status VARCHAR(50) NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'running', 'completed', 'failed', 'cancelled')),
    total_messages INTEGER NOT NULL DEFAULT 0,
    processed_messages INTEGER NOT NULL DEFAULT 0,
    failed_messages INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_mail_import_jobs_tenant_id ON mail_import_jobs(tenant_id);
CREATE INDEX idx_mail_import_jobs_owner_id ON mail_import_jobs(owner_id);
CREATE INDEX idx_mail_import_jobs_account_id ON mail_import_jobs(account_id);
CREATE INDEX idx_mail_import_jobs_status ON mail_import_jobs(status) WHERE deleted_at IS NULL;
```

**Step 3: Apply migrations locally**

Run:
```bash
cd backend
DATABASE_URL=postgres://scolak@localhost/rustshare cargo sqlx migrate run
```

**Step 4: Commit**

```bash
git add backend/migrations/20260708160002_create_mail_accounts_table.sql backend/migrations/20260708160003_create_mail_import_jobs_table.sql
git commit -s -m "feat(mail): add mail_accounts and mail_import_jobs tables

Refs #147"
```

---

## Task 2: Domain types for accounts and import jobs

**Files:**
- Modify: `backend/crates/core/src/domain/mod.rs`
- Create: `backend/crates/core/src/domain/mail_account.rs`

**Step 1: Add ID aliases**

In `backend/crates/core/src/domain/mod.rs` near the existing mail IDs:

```rust
pub type MailAccountId = Uuid;
pub type MailImportJobId = Uuid;
```

**Step 2: Create `mail_account.rs`**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

use super::{MailAccountId, MailImportJobId, UserId};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MailTlsMode {
    #[default]
    Tls,
    StartTls,
    None,
}

impl From<MailTlsMode> for String {
    fn from(mode: MailTlsMode) -> Self {
        match mode {
            MailTlsMode::Tls => "tls".to_string(),
            MailTlsMode::StartTls => "starttls".to_string(),
            MailTlsMode::None => "none".to_string(),
        }
    }
}

impl std::str::FromStr for MailTlsMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "tls" => Ok(MailTlsMode::Tls),
            "starttls" => Ok(MailTlsMode::StartTls),
            "none" => Ok(MailTlsMode::None),
            _ => Err(format!("Invalid mail TLS mode: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MailImportJobStatus {
    #[default]
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl From<MailImportJobStatus> for String {
    fn from(status: MailImportJobStatus) -> Self {
        match status {
            MailImportJobStatus::Pending => "pending".to_string(),
            MailImportJobStatus::Running => "running".to_string(),
            MailImportJobStatus::Completed => "completed".to_string(),
            MailImportJobStatus::Failed => "failed".to_string(),
            MailImportJobStatus::Cancelled => "cancelled".to_string(),
        }
    }
}

impl std::str::FromStr for MailImportJobStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(MailImportJobStatus::Pending),
            "running" => Ok(MailImportJobStatus::Running),
            "completed" => Ok(MailImportJobStatus::Completed),
            "failed" => Ok(MailImportJobStatus::Failed),
            "cancelled" => Ok(MailImportJobStatus::Cancelled),
            _ => Err(format!("Invalid import job status: {s}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow, ToSchema)]
pub struct MailAccount {
    #[schema(value_type = Uuid)]
    pub id: MailAccountId,
    pub tenant_id: Uuid,
    #[schema(value_type = Uuid)]
    pub owner_id: UserId,
    pub name: String,
    pub host: String,
    pub port: i32,
    pub username: String,
    pub password_enc: String,
    pub tls_mode: String,
    pub is_enabled: bool,
    pub last_error: Option<String>,
    pub last_connected_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MailAccount {
    pub fn new(
        tenant_id: Uuid,
        owner_id: UserId,
        name: String,
        host: String,
        port: i32,
        username: String,
        password_enc: String,
        tls_mode: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            owner_id,
            name,
            host,
            port,
            username,
            password_enc,
            tls_mode: tls_mode.into(),
            is_enabled: true,
            last_error: None,
            last_connected_at: None,
            deleted_at: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow, ToSchema)]
pub struct MailImportJob {
    #[schema(value_type = Uuid)]
    pub id: MailImportJobId,
    pub tenant_id: Uuid,
    #[schema(value_type = Uuid)]
    pub owner_id: UserId,
    #[schema(value_type = Uuid)]
    pub account_id: MailAccountId,
    pub source_mode: String,
    pub folder_name: String,
    pub selected_uids: Option<Vec<i64>>,
    pub status: String,
    pub total_messages: i32,
    pub processed_messages: i32,
    pub failed_messages: i32,
    pub last_error: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MailImportJob {
    pub fn new(
        tenant_id: Uuid,
        owner_id: UserId,
        account_id: MailAccountId,
        folder_name: String,
        selected_uids: Vec<i64>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            owner_id,
            account_id,
            source_mode: "imap_selected".to_string(),
            folder_name,
            selected_uids: Some(selected_uids),
            status: MailImportJobStatus::Pending.into(),
            total_messages: selected_uids.len() as i32,
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
}
```

**Step 3: Re-export from `domain/mod.rs`**

```rust
pub mod mail_account;
pub use mail_account::*;
```

**Step 4: Add unit test**

Create `backend/crates/core/tests/mail_account_domain_test.rs`:

```rust
use rustshare_core::domain::{MailAccount, MailImportJob, MailTlsMode, MailImportJobStatus};
use uuid::Uuid;

#[test]
fn mail_account_defaults() {
    let account = MailAccount::new(
        Uuid::nil(),
        Uuid::new_v4(),
        "Work Gmail".to_string(),
        "imap.gmail.com".to_string(),
        993,
        "user@example.com".to_string(),
        "enc".to_string(),
        MailTlsMode::Tls,
    );
    assert!(account.is_enabled);
    assert_eq!(account.tls_mode, "tls");
}

#[test]
fn mail_tls_mode_roundtrip() {
    assert_eq!(MailTlsMode::StartTls.to_string(), "starttls");
    assert_eq!("starttls".parse::<MailTlsMode>().unwrap(), MailTlsMode::StartTls);
}

#[test]
fn mail_import_job_defaults() {
    let job = MailImportJob::new(
        Uuid::nil(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        "INBOX".to_string(),
        vec![1, 2, 3],
    );
    assert_eq!(job.status, "pending");
    assert_eq!(job.total_messages, 3);
}
```

**Step 5: Run tests**

```bash
SQLX_OFFLINE=true cargo test -p rustshare-core --test mail_account_domain_test
```

**Step 6: Commit**

```bash
git add backend/crates/core/src/domain/mail_account.rs backend/crates/core/src/domain/mod.rs backend/crates/core/tests/mail_account_domain_test.rs
git commit -s -m "feat(mail): add MailAccount and MailImportJob domain types

Refs #147"
```

---

## Task 3: MetadataStore CRUD for accounts and jobs

**Files:**
- Modify: `backend/crates/storage/src/metadata.rs`

**Step 1: Add imports**

```rust
use rustshare_core::domain::{MailAccount, MailImportJob, MailAccountId, MailImportJobId};
```

**Step 2: Add account methods**

- `create_mail_account(&self, account: &MailAccount) -> Result<()>`
- `get_mail_account(&self, id: MailAccountId, owner_id: UserId) -> Result<Option<MailAccount>>`
- `list_mail_accounts_by_owner(&self, tenant_id: Uuid, owner_id: UserId) -> Result<Vec<MailAccount>>`
- `update_mail_account(&self, account: &MailAccount) -> Result<()>`
- `soft_delete_mail_account(&self, id: MailAccountId, owner_id: UserId) -> Result<bool>`

Use `sqlx::query!` or `sqlx::query_as!` as appropriate. Update `updated_at` on modifications.

**Step 3: Add job methods**

- `create_mail_import_job(&self, job: &MailImportJob) -> Result<()>`
- `get_mail_import_job(&self, id: MailImportJobId, owner_id: UserId) -> Result<Option<MailImportJob>>`
- `list_mail_import_jobs_by_owner(&self, tenant_id: Uuid, owner_id: UserId, account_id: Option<MailAccountId>) -> Result<Vec<MailImportJob>>`
- `claim_next_pending_mail_import_job(&self) -> Result<Option<MailImportJob>>` using `SELECT ... FROM mail_import_jobs WHERE status = 'pending' AND deleted_at IS NULL ORDER BY created_at ASC FOR UPDATE SKIP LOCKED`
- `update_mail_import_job_status(&self, id: MailImportJobId, status: &str, processed: i32, failed: i32, last_error: Option<&str>) -> Result<()>`
- `mark_mail_import_job_running(&self, id: MailImportJobId) -> Result<()>`
- `mark_mail_import_job_completed(&self, id: MailImportJobId) -> Result<()>`
- `mark_mail_import_job_failed(&self, id: MailImportJobId, error: &str) -> Result<()>`

**Step 4: Compile check**

```bash
SQLX_OFFLINE=true cargo check -p rustshare-storage
```

**Step 5: Commit**

```bash
git add backend/crates/storage/src/metadata.rs
git commit -s -m "feat(mail): add MetadataStore methods for accounts and import jobs

Refs #147"
```

---

## Task 4: IMAP client module

**Files:**
- Modify: `backend/server/Cargo.toml`
- Create: `backend/server/src/services/imap_client.rs`
- Modify: `backend/server/src/services/mod.rs`

**Step 1: Add dependencies**

In `backend/server/Cargo.toml` under `[dependencies]`:

```toml
async-imap = { version = "0.11", default-features = false, features = ["runtime-tokio"] }
tokio-rustls = "0.26"
webpki-roots = "0.26"
```

**Step 2: Implement `imap_client.rs`**

Create a wrapper with these types:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ImapError {
    #[error("IMAP connection failed: {0}")]
    ConnectionFailed(String),
    #[error("IMAP authentication failed: {0}")]
    AuthenticationFailed(String),
    #[error("IMAP command failed: {0}")]
    CommandFailed(String),
    #[error("TLS error: {0}")]
    Tls(String),
}

pub struct ImapClient;

pub struct ImapSession {
    session: async_imap::Session<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MailFolder {
    pub name: String,
    pub delimiter: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImapMessageSummary {
    pub uid: u32,
    pub subject: Option<String>,
    pub from_address: Option<String>,
    pub from_name: Option<String>,
    pub sent_at: Option<DateTime<Utc>>,
    pub size_bytes: Option<i64>,
}
```

Provide:

- `ImapClient::connect_tls(host: &str, port: u16, tls_mode: MailTlsMode) -> Result<async_imap::Client<...>, ImapError>`
  - For `Tls`: `TcpStream::connect`, build `tokio_rustls::TlsConnector` with `webpki_roots`, connect.
  - For `None`: `Client::new(tcp_stream)`.
  - `StartTls` is out of scope for Phase 3; return an error if requested.
- `ImapSession::login(username: &str, password: &str) -> Result<Self, ImapError>`
- `ImapSession::list_folders(&mut self) -> Result<Vec<MailFolder>, ImapError>` using `list("", "*")`.
- `ImapSession::select_folder(&mut self, name: &str) -> Result<(), ImapError>`.
- `ImapSession::fetch_message_summaries(&mut self, folder: &str, limit: usize) -> Result<Vec<ImapMessageSummary>, ImapError>`
  - Select folder, `uid_search("ALL")`, then `uid_fetch(uids, "(UID ENVELOPE RFC822.SIZE)")`, parse envelopes.
- `ImapSession::fetch_rfc822(&mut self, folder: &str, uid: u32) -> Result<Vec<u8>, ImapError>`
  - Select folder, `uid_fetch(format!("{uid}"), "RFC822")`, return body bytes.
- `ImapSession::logout(self) -> Result<(), ImapError>`.

**Step 3: Export module**

In `backend/server/src/services/mod.rs`:

```rust
pub mod imap_client;
pub use imap_client::{ImapClient, ImapError, ImapMessageSummary, ImapSession, MailFolder};
```

**Step 4: Compile check**

```bash
SQLX_OFFLINE=true cargo check -p rustshare-server
```

**Step 5: Commit**

```bash
git add backend/server/Cargo.toml backend/server/src/services/imap_client.rs backend/server/src/services/mod.rs
git commit -s -m "feat(mail): add async-imap client wrapper with rustls TLS

Refs #147"
```

---

## Task 5: MailService account management and encrypted credentials

**Files:**
- Modify: `backend/server/src/services/mail_service.rs`
- Modify: `backend/server/src/bootstrap.rs`
- Modify: `backend/server/src/state.rs` (only if needed)

**Step 1: Extend `MailError`**

```rust
#[derive(Debug, thiserror::Error)]
pub enum MailError {
    // ... existing variants ...
    #[error("IMAP error: {0}")]
    Imap(String),
    #[error("Account not found: {0}")]
    AccountNotFound(Uuid),
    #[error("Import job not found: {0}")]
    JobNotFound(Uuid),
}
```

**Step 2: Add `secret_key` to `MailService`**

```rust
pub struct MailService<O = ObjectStore>
where
    O: ObjectStoreOps,
{
    // ... existing fields ...
    secret_key: Arc<rustshare_crypto::SecretEncryptionKey>,
}
```

Update `new()` and all call sites in `bootstrap.rs` and tests.

**Step 3: Add account methods**

```rust
pub async fn create_account(
    &self,
    tenant_id: Uuid,
    owner_id: UserId,
    name: String,
    host: String,
    port: i32,
    username: String,
    password: String,
    tls_mode: MailTlsMode,
) -> Result<MailAccount, MailError> { ... }

pub async fn list_accounts(
    &self,
    tenant_id: Uuid,
    owner_id: UserId,
) -> Result<Vec<MailAccount>, MailError> { ... }

pub async fn get_account(
    &self,
    tenant_id: Uuid,
    owner_id: UserId,
    account_id: MailAccountId,
) -> Result<MailAccount, MailError> { ... }

pub async fn update_account(
    &self,
    tenant_id: Uuid,
    owner_id: UserId,
    account_id: MailAccountId,
    name: Option<String>,
    host: Option<String>,
    port: Option<i32>,
    username: Option<String>,
    password: Option<String>,
    tls_mode: Option<MailTlsMode>,
    is_enabled: Option<bool>,
) -> Result<MailAccount, MailError> { ... }

pub async fn delete_account(
    &self,
    tenant_id: Uuid,
    owner_id: UserId,
    account_id: MailAccountId,
) -> Result<(), MailError> { ... }

pub async fn test_account_connection(
    &self,
    tenant_id: Uuid,
    owner_id: UserId,
    account_id: MailAccountId,
) -> Result<(), MailError> { ... }
```

Encrypt `password` with `rustshare_crypto::encrypt_secret` on create/update. Decrypt with `decrypt_secret` for connection tests and import worker.

**Step 4: Add IMAP browsing methods**

```rust
pub async fn list_imap_folders(
    &self,
    tenant_id: Uuid,
    owner_id: UserId,
    account_id: MailAccountId,
) -> Result<Vec<MailFolder>, MailError> { ... }

pub async fn list_imap_messages(
    &self,
    tenant_id: Uuid,
    owner_id: UserId,
    account_id: MailAccountId,
    folder: &str,
    limit: usize,
) -> Result<Vec<ImapMessageSummary>, MailError> { ... }
```

**Step 5: Refactor import path**

Change `import_eml` to call a new private `import_raw_source` that accepts `source_mode`, `source_folder`, `source_uid`, and the raw bytes. Update the existing `.eml` upload path to call it with `EmlUpload` and no folder/uid.

```rust
async fn import_raw_source(
    &self,
    tenant_id: Uuid,
    owner_id: UserId,
    imported_by: UserId,
    source_mode: MailSourceMode,
    source_folder: Option<&str>,
    source_uid: Option<i64>,
    raw_source: Vec<u8>,
) -> Result<MailMessage, MailError> { ... }
```

**Step 6: Add import-job methods**

```rust
pub async fn create_imap_import_job(
    &self,
    tenant_id: Uuid,
    owner_id: UserId,
    account_id: MailAccountId,
    folder_name: String,
    selected_uids: Vec<i64>,
) -> Result<MailImportJob, MailError> { ... }

pub async fn get_import_job(
    &self,
    tenant_id: Uuid,
    owner_id: UserId,
    job_id: MailImportJobId,
) -> Result<MailImportJob, MailError> { ... }

pub async fn process_import_job(
    &self,
    job: &MailImportJob,
) -> Result<(), MailError> { ... }
```

`process_import_job` connects, fetches each UID via `fetch_rfc822`, calls `import_raw_source`, increments counters, and emits a `MailImported` event per message.

**Step 7: Update bootstrap wiring**

In `backend/server/src/bootstrap.rs`:

```rust
let secret_key_clone = secret_key.clone();
let mail_service = Arc::new(crate::services::mail_service::MailService::new(
    Arc::clone(&metadata_store),
    Arc::clone(&object_store),
    Arc::clone(&file_service),
    Arc::clone(&folder_service),
    Arc::clone(&permission_resolver),
    Arc::clone(&event_store),
    Arc::new(secret_key_clone),
));
```

**Step 8: Compile check**

```bash
SQLX_OFFLINE=true cargo check -p rustshare-server
```

**Step 9: Commit**

```bash
git add backend/server/src/services/mail_service.rs backend/server/src/bootstrap.rs
git commit -s -m "feat(mail): add IMAP account management, credential encryption, and selected import service methods

Refs #147"
```

---

## Task 6: Background import-job worker

**Files:**
- Create: `backend/server/src/mail_import_worker.rs`
- Modify: `backend/server/src/bootstrap.rs`
- Modify: `backend/server/src/main.rs` (only if worker spawn is centralized)

**Step 1: Implement the worker**

Create `backend/server/src/mail_import_worker.rs`:

```rust
pub struct MailImportWorkerConfig {
    pub poll_interval: Duration,
    pub max_concurrent_jobs: usize,
}

impl Default for MailImportWorkerConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(10),
            max_concurrent_jobs: 2,
        }
    }
}

pub fn spawn_mail_import_worker(
    mail_service: Arc<crate::services::mail_service::MailService>,
    metadata_store: Arc<MetadataStore>,
    mut shutdown: broadcast::Receiver<()>,
    config: MailImportWorkerConfig,
) {
    tokio::spawn(async move {
        let sem = Arc::new(tokio::sync::Semaphore::new(config.max_concurrent_jobs));
        loop {
            let timeout = tokio::time::sleep(config.poll_interval);
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = timeout => {}
            }

            loop {
                let permit = match sem.clone().try_acquire_owned() {
                    Ok(p) => p,
                    Err(_) => break,
                };

                let job = match metadata_store.claim_next_pending_mail_import_job().await {
                    Ok(Some(j)) => j,
                    Ok(None) => break,
                    Err(e) => {
                        tracing::error!("Failed to claim mail import job: {e}");
                        break;
                    }
                };

                let service = Arc::clone(&mail_service);
                tokio::spawn(async move {
                    let _permit = permit;
                    if let Err(e) = service.process_import_job(&job).await {
                        tracing::error!("Mail import job {} failed: {e}", job.id);
                    }
                });
            }
        }
    });
}
```

**Step 2: Wire worker in `bootstrap.rs`**

Near the other worker spawns (around lines 545-567):

```rust
crate::mail_import_worker::spawn_mail_import_worker(
    Arc::clone(&mail_service),
    Arc::clone(&metadata_store),
    shutdown_tx.subscribe(),
    crate::mail_import_worker::MailImportWorkerConfig::default(),
);
```

**Step 3: Register module in `backend/server/src/main.rs`**

Add `mod mail_import_worker;` near `mod replication;` etc.

**Step 4: Compile check**

```bash
SQLX_OFFLINE=true cargo check -p rustshare-server
```

**Step 5: Commit**

```bash
git add backend/server/src/mail_import_worker.rs backend/server/src/bootstrap.rs backend/server/src/main.rs
git commit -s -m "feat(mail): add background mail import job worker

Refs #147"
```

---

## Task 7: Audit events for accounts and imports

**Files:**
- Modify: `backend/crates/core/src/events/types.rs`

**Step 1: Extend `AggregateType`**

```rust
MailAccount,
MailImportJob,
```

**Step 2: Extend `EventType`**

```rust
MailAccountCreated,
MailAccountDeleted,
MailImported,
```

**Step 3: Add payloads**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MailAccountCreatedPayload {
    #[schema(value_type = Uuid)]
    pub account_id: MailAccountId,
    pub host: String,
    pub username: String,
    #[schema(value_type = Uuid)]
    pub owner_id: UserId,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MailAccountDeletedPayload {
    #[schema(value_type = Uuid)]
    pub account_id: MailAccountId,
    #[schema(value_type = Uuid)]
    pub owner_id: UserId,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MailImportedPayload {
    #[schema(value_type = Uuid)]
    pub message_id: MailMessageId,
    #[schema(value_type = Uuid)]
    pub account_id: MailAccountId,
    pub folder_name: String,
    pub source_uid: i64,
    #[schema(value_type = Uuid)]
    pub owner_id: UserId,
}
```

**Step 4: Emit events**

In `MailService`:
- `create_account` → `MailAccountCreated`.
- `delete_account` → `MailAccountDeleted`.
- `process_import_job` per imported message → `MailImported`.

Use `EventStore::append` with aggregate type and ID.

**Step 5: Compile check**

```bash
SQLX_OFFLINE=true cargo check --workspace
```

**Step 6: Commit**

```bash
git add backend/crates/core/src/events/types.rs backend/server/src/services/mail_service.rs
git commit -s -m "feat(mail): add audit events for IMAP accounts and imported messages

Refs #147"
```

---

## Task 8: REST API for accounts, folders, messages, and import jobs

**Files:**
- Modify: `backend/server/src/handlers/mail.rs`
- Modify: `backend/server/src/routes.rs`
- Modify: `backend/server/src/openapi.rs`
- Modify: `backend/server/src/handlers/mod.rs` if new `MailError` mapping is needed

**Step 1: Add request/response DTOs**

In `backend/server/src/handlers/mail.rs`:

```rust
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateMailAccountRequest {
    pub name: String,
    pub host: String,
    pub port: i32,
    pub username: String,
    pub password: String,
    pub tls_mode: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateMailAccountRequest {
    pub name: Option<String>,
    pub host: Option<String>,
    pub port: Option<i32>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub tls_mode: Option<String>,
    pub is_enabled: Option<bool>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailAccountResponse {
    #[schema(value_type = Uuid)]
    pub id: Uuid,
    pub name: String,
    pub host: String,
    pub port: i32,
    pub username: String,
    pub tls_mode: String,
    pub is_enabled: bool,
    pub last_connected_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailFolderListResponse {
    pub folders: Vec<MailFolderResponse>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailFolderResponse {
    pub name: String,
    pub delimiter: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailMessageSummaryResponse {
    pub uid: u32,
    pub subject: Option<String>,
    pub from_address: Option<String>,
    pub from_name: Option<String>,
    pub sent_at: Option<DateTime<Utc>>,
    pub size_bytes: i64,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateMailImportJobRequest {
    pub folder_name: String,
    pub selected_uids: Vec<i64>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailImportJobResponse {
    #[schema(value_type = Uuid)]
    pub id: Uuid,
    #[schema(value_type = Uuid)]
    pub account_id: Uuid,
    pub folder_name: String,
    pub status: String,
    pub total_messages: i32,
    pub processed_messages: i32,
    pub failed_messages: i32,
    pub last_error: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}
```

**Step 2: Implement handlers**

- `create_mail_account`
- `list_mail_accounts`
- `get_mail_account`
- `update_mail_account`
- `delete_mail_account`
- `test_mail_account`
- `list_mail_account_folders`
- `list_mail_account_messages` (query param `folder`)
- `create_mail_import_job`
- `get_mail_import_job`

Do **not** return `password_enc` in responses.

**Step 3: Register routes in `backend/server/src/routes.rs`**

Add to `mail_routes()`:

```rust
route.post("/mail/accounts", handlers::mail::create_mail_account)
     .get("/mail/accounts", handlers::mail::list_mail_accounts)
     .get("/mail/accounts/:id", handlers::mail::get_mail_account)
     .patch("/mail/accounts/:id", handlers::mail::update_mail_account)
     .delete("/mail/accounts/:id", handlers::mail::delete_mail_account)
     .post("/mail/accounts/:id/test", handlers::mail::test_mail_account)
     .get("/mail/accounts/:id/folders", handlers::mail::list_mail_account_folders)
     .get("/mail/accounts/:id/messages", handlers::mail::list_mail_account_messages)
     .post("/mail/accounts/:id/import", handlers::mail::create_mail_import_job)
     .get("/mail/import-jobs/:id", handlers::mail::get_mail_import_job)
```

Use the actual `MethodRouter` style already used in the file.

**Step 4: Register in OpenAPI**

Add all new handler paths to `paths(...)` and DTOs to `components(schemas(...))` in `backend/server/src/openapi.rs`.

**Step 5: Update `AppError` mapping**

Ensure `MailError::Imap`, `AccountNotFound`, and `JobNotFound` map to sensible status codes.

**Step 6: Compile check**

```bash
SQLX_OFFLINE=true cargo check -p rustshare-server
```

**Step 7: Commit**

```bash
git add backend/server/src/handlers/mail.rs backend/server/src/routes.rs backend/server/src/openapi.rs backend/server/src/handlers/mod.rs
git commit -s -m "feat(mail): add IMAP account and import job REST API

Refs #147"
```

---

## Task 9: Tests and QA

**Files:**
- Create: `backend/crates/core/tests/mail_account_domain_test.rs` (already created in Task 2)
- Create: `backend/server/src/services/imap_client.rs` unit tests with a mock stream if feasible
- Create: `backend/tests/mail_imap_import_test.rs`
- Modify: `backend/server/Cargo.toml` to register the new integration test

**Step 1: Add integration test entry**

In `backend/server/Cargo.toml`:

```toml
[[test]]
name = "mail_imap_import_test"
path = "../tests/mail_imap_import_test.rs"
```

**Step 2: Write integration test**

Use the existing `backend/tests/contracts/common.rs` `TestContext`. The test should:

1. Create a test user.
2. Create a mail account (pointing at a local IMAP test server such as GreenMail, or skip if `IMAP_TEST_HOST` is not set).
3. Call `list_imap_folders` and assert `INBOX` is present.
4. Create an import job for `INBOX` with selected UIDs.
5. Poll the job status endpoint until it reaches `completed` or `failed` (with timeout).
6. Verify that `mail_messages` rows were created with `source_mode = 'imap_selected'`.

Mark the test `#[ignore = "requires IMAP test server (e.g. GreenMail)"]` so CI does not require it.

**Step 3: Run unit tests**

```bash
SQLX_OFFLINE=true cargo test -p rustshare-core --test mail_account_domain_test
SQLX_OFFLINE=true cargo test -p rustshare-server --lib mail_service
```

**Step 4: Run SQLx prepare**

```bash
cd backend
DATABASE_URL=postgres://scolak@localhost/rustshare_prepare cargo sqlx prepare --workspace
cargo sqlx prepare --workspace --check
```

**Step 5: Run formatting and clippy**

```bash
cargo fmt --check
SQLX_OFFLINE=true cargo clippy --all-features -- -D warnings
SQLX_OFFLINE=true cargo check --workspace
```

**Step 6: Commit**

```bash
git add backend/server/Cargo.toml backend/tests/mail_imap_import_test.rs backend/.sqlx/
git commit -s -m "test(mail): add IMAP import integration test and update sqlx metadata

Refs #147"
```

---

## Task 10: Final verification and push

**Step 1: Full test sweep**

```bash
cd backend
cargo fmt --check
SQLX_OFFLINE=true cargo clippy --all-features -- -D warnings
SQLX_OFFLINE=true cargo test --lib -p rustshare-server
SQLX_OFFLINE=true cargo test -p rustshare-core --test mail_account_domain_test
DATABASE_URL=postgres://scolak@localhost/rustshare SQLX_OFFLINE=true cargo test --test mail_import_test -- --ignored
DATABASE_URL=postgres://scolak@localhost/rustshare SQLX_OFFLINE=true cargo test --test mail_linking_test -- --ignored
```

**Step 2: Push branch**

```bash
git push -u origin feat/mail-phase3-imap-import
```

**Step 3: Open PR**

- Title: `feat(mail): Phase 3 IMAP selected import`
- Body includes `Refs #147` and a short safety note about encrypted credentials, tenant isolation, and audit events.
- Do **not** use `Fixes #147`.

---

## Out of scope

- IMAP archive jobs (Phase 4)
- Full-text search / AI indexing of imported mail (Phase 5)
- Outbound SMTP / reply / forward (Phase 6)
- OAuth IMAP authentication (future)
- STARTTLS support (future; `tls` and `none` only in this phase)
- Two-way synchronization with live mailbox state

---

## Risks and notes

- `async-imap` + `tokio-rustls` combination must be verified to compile and connect. If the trait bounds do not line up, fall back to `async-native-tls/runtime-tokio` and document the native-tls dependency in the PR.
- The background worker uses `FOR UPDATE SKIP LOCKED` but has no cross-process lease table; this is consistent with the existing replication worker pattern but means multiple server instances may race. Acceptable for Phase 3.
- Integration tests require a running IMAP server; keep them `#[ignore]` so CI remains green.
- Account credentials are encrypted at rest but appear briefly in memory during connection tests and import jobs. This is expected; avoid logging them.
