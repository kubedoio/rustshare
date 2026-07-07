# Mail Schema and Domain Types Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:subagents:subagent-driven-development to implement this plan task-by-task.

**Goal:** Create the database schema and Rust domain types for imported mail artifacts, plus the minimal service skeleton and module registry entry needed for follow-up `.eml` import work.

**Architecture:** Add PostgreSQL tables (`mail_messages`, `mail_message_parts`, `mail_attachments`) following existing tenant-scoped patterns. Define domain structs in `rustshare-core`, a concrete `MailService` in `rustshare-server`, register a disabled-by-default `mail` module, and wire the service into `AppState`. Keep all persistence logic placeholder-only; no upload, parsing, or IMAP behavior yet.

**Tech Stack:** Rust, SQLx, PostgreSQL, Axum, Utoipa. Follow existing `rustshare-core` / `rustshare-server` patterns.

---

## Context

- Worktree: `/Users/scolak/Projects/x/rustshare/.worktrees/mail-schema-domain-types`
- Branch: `feat/mail-schema-domain-types`
- Epic: `docs/epics/0147-mail-module.md`
- Spec: `docs/specs/mail-module.md`
- ADR: `docs/adr/0032-mail-module-boundaries.md`
- Refs #147

Existing patterns to follow:
- Domain types: `backend/crates/core/src/domain/file.rs`, `folder.rs`, `module.rs`
- Domain exports: `backend/crates/core/src/domain/mod.rs`
- Module registry: `backend/server/src/services/module_service.rs`
- Service wiring: `backend/server/src/bootstrap.rs`, `backend/server/src/state.rs`
- Routes: `backend/server/src/routes.rs`, merged in `backend/server/src/main.rs`

---

## Task 1: Create PostgreSQL migration for mail tables

**Files:**
- Create: `backend/migrations/20260707150001_create_mail_tables.sql`

**Step 1: Write the migration**

Create the migration with three tenant-scoped tables:

```sql
CREATE TABLE mail_messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source_mode VARCHAR(50) NOT NULL DEFAULT 'eml_upload',
    source_folder TEXT,
    source_uid BIGINT,
    message_id TEXT,
    in_reply_to TEXT,
    reference_ids TEXT[],
    subject TEXT,
    from_address TEXT,
    from_name TEXT,
    to_addresses JSONB NOT NULL DEFAULT '[]',
    cc_addresses JSONB NOT NULL DEFAULT '[]',
    bcc_addresses JSONB NOT NULL DEFAULT '[]',
    sent_at TIMESTAMPTZ,
    imported_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    imported_by UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    visibility VARCHAR(50) NOT NULL DEFAULT 'private',
    object_key TEXT,
    blob_key TEXT,
    blob_sha256 VARCHAR(64),
    size_bytes BIGINT,
    has_attachments BOOLEAN NOT NULL DEFAULT false,
    deleted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_mail_messages_tenant_id ON mail_messages(tenant_id);
CREATE INDEX idx_mail_messages_owner_id ON mail_messages(owner_id);
CREATE INDEX idx_mail_messages_message_id ON mail_messages(tenant_id, message_id);
CREATE INDEX idx_mail_messages_sent_at ON mail_messages(tenant_id, sent_at DESC);
CREATE INDEX idx_mail_messages_deleted_at ON mail_messages(deleted_at);

CREATE TABLE mail_message_parts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    message_id UUID NOT NULL REFERENCES mail_messages(id) ON DELETE CASCADE,
    part_index INTEGER NOT NULL,
    content_type VARCHAR(255) NOT NULL,
    charset VARCHAR(50),
    blob_key TEXT,
    blob_sha256 VARCHAR(64),
    size_bytes BIGINT,
    is_body BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_mail_message_parts_tenant_id ON mail_message_parts(tenant_id);
CREATE INDEX idx_mail_message_parts_message_id ON mail_message_parts(message_id);

CREATE TABLE mail_attachments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    message_id UUID NOT NULL REFERENCES mail_messages(id) ON DELETE CASCADE,
    file_id UUID REFERENCES files(id) ON DELETE SET NULL,
    filename TEXT NOT NULL,
    mime_type VARCHAR(255),
    size_bytes BIGINT,
    part_index INTEGER,
    content_disposition VARCHAR(50),
    blob_key TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_mail_attachments_tenant_id ON mail_attachments(tenant_id);
CREATE INDEX idx_mail_attachments_message_id ON mail_attachments(message_id);
```

**Step 2: Verify migration syntax**

Run: `cd backend && sqlx migrate info`
Expected: New migration appears in the list, no errors.

**Step 3: Commit**

```bash
git add backend/migrations/20260707150001_create_mail_tables.sql
git commit -s -m "mail: add migration for mail_messages, parts, attachments

Refs #147"
```

---

## Task 2: Define mail domain types in rustshare-core

**Files:**
- Create: `backend/crates/core/src/domain/mail_message.rs`
- Modify: `backend/crates/core/src/domain/mod.rs`

**Step 1: Add ID aliases and re-export**

In `backend/crates/core/src/domain/mod.rs`, add after existing ID aliases:

```rust
pub type MailMessageId = Uuid;
pub type MailMessagePartId = Uuid;
pub type MailAttachmentId = Uuid;
```

Add module declaration and re-export:

```rust
mod mail_message;
pub use mail_message::{MailAttachment, MailMessage, MailMessagePart, MailSourceMode, MailVisibility};
```

**Step 2: Create domain file**

Create `backend/crates/core/src/domain/mail_message.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

use super::{MailAttachmentId, MailMessageId, MailMessagePartId, UserId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, utoipa::ToSchema)]
#[sqlx(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum MailSourceMode {
    EmlUpload,
    ImapSelected,
    ImapArchive,
    InboundAddress,
}

impl Default for MailSourceMode {
    fn default() -> Self {
        Self::EmlUpload
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, utoipa::ToSchema)]
#[sqlx(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum MailVisibility {
    Private,
    Workspace,
    Project,
    AdminArchive,
}

impl Default for MailVisibility {
    fn default() -> Self {
        Self::Private
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow, ToSchema)]
pub struct MailMessage {
    #[schema(value_type = Uuid)]
    pub id: MailMessageId,
    pub tenant_id: Uuid,
    #[schema(value_type = Uuid)]
    pub owner_id: UserId,
    pub source_mode: String,
    pub source_folder: Option<String>,
    pub source_uid: Option<i64>,
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
    #[sqlx(rename = "reference_ids")]
    pub references: Option<Vec<String>>,
    pub subject: Option<String>,
    pub from_address: Option<String>,
    pub from_name: Option<String>,
    pub to_addresses: serde_json::Value,
    pub cc_addresses: serde_json::Value,
    pub bcc_addresses: serde_json::Value,
    pub sent_at: Option<DateTime<Utc>>,
    pub imported_at: DateTime<Utc>,
    #[schema(value_type = Uuid)]
    pub imported_by: UserId,
    pub visibility: String,
    pub object_key: Option<String>,
    pub blob_key: Option<String>,
    pub blob_sha256: Option<String>,
    pub size_bytes: Option<i64>,
    pub has_attachments: bool,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MailMessage {
    pub fn new(
        tenant_id: Uuid,
        owner_id: UserId,
        imported_by: UserId,
        source_mode: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            owner_id,
            source_mode: source_mode.into(),
            source_folder: None,
            source_uid: None,
            message_id: None,
            in_reply_to: None,
            references: None,
            subject: None,
            from_address: None,
            from_name: None,
            to_addresses: serde_json::Value::Array(vec![]),
            cc_addresses: serde_json::Value::Array(vec![]),
            bcc_addresses: serde_json::Value::Array(vec![]),
            sent_at: None,
            imported_at: now,
            imported_by,
            visibility: "private".to_string(),
            object_key: None,
            blob_key: None,
            blob_sha256: None,
            size_bytes: None,
            has_attachments: false,
            deleted_at: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow, ToSchema)]
pub struct MailMessagePart {
    #[schema(value_type = Uuid)]
    pub id: MailMessagePartId,
    pub tenant_id: Uuid,
    #[schema(value_type = Uuid)]
    pub message_id: MailMessageId,
    pub part_index: i32,
    pub content_type: String,
    pub charset: Option<String>,
    pub blob_key: Option<String>,
    pub blob_sha256: Option<String>,
    pub size_bytes: Option<i64>,
    pub is_body: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow, ToSchema)]
pub struct MailAttachment {
    #[schema(value_type = Uuid)]
    pub id: MailAttachmentId,
    pub tenant_id: Uuid,
    #[schema(value_type = Uuid)]
    pub message_id: MailMessageId,
    #[schema(value_type = Option<Uuid>)]
    pub file_id: Option<Uuid>,
    pub filename: String,
    pub mime_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub part_index: Option<i32>,
    pub content_disposition: Option<String>,
    pub blob_key: Option<String>,
    pub created_at: DateTime<Utc>,
}
```

**Step 3: Run cargo check**

Run: `cd backend && SQLX_OFFLINE=true cargo check --workspace`
Expected: Compiles or shows expected SQLx offline errors for new queries (none yet).

**Step 4: Commit**

```bash
git add backend/crates/core/src/domain/mod.rs backend/crates/core/src/domain/mail_message.rs
git commit -s -m "mail: add domain types for mail messages, parts, attachments

Refs #147"
```

---

## Task 3: Create MailService skeleton in rustshare-server

**Files:**
- Create: `backend/server/src/services/mail_service.rs`
- Modify: `backend/server/src/services/mod.rs`

**Step 1: Create service file**

Create `backend/server/src/services/mail_service.rs`:

```rust
use std::sync::Arc;

use rustshare_core::domain::{MailMessage, MailMessagePart, MailAttachment};
use rustshare_storage::MetadataStore;
use uuid::Uuid;

#[derive(Clone)]
pub struct MailService {
    metadata_store: Arc<MetadataStore>,
}

impl MailService {
    pub fn new(metadata_store: Arc<MetadataStore>) -> Self {
        Self { metadata_store }
    }

    /// Placeholder: list imported mail messages for a user.
    pub async fn list_messages(
        &self,
        _tenant_id: Uuid,
        _owner_id: Uuid,
    ) -> anyhow::Result<Vec<MailMessage>> {
        Ok(vec![])
    }

    /// Placeholder: get a single mail message by ID.
    pub async fn get_message(
        &self,
        _tenant_id: Uuid,
        _owner_id: Uuid,
        _message_id: Uuid,
    ) -> anyhow::Result<Option<MailMessage>> {
        Ok(None)
    }

    /// Placeholder: list parts for a message.
    pub async fn list_parts(
        &self,
        _tenant_id: Uuid,
        _message_id: Uuid,
    ) -> anyhow::Result<Vec<MailMessagePart>> {
        Ok(vec![])
    }

    /// Placeholder: list attachments for a message.
    pub async fn list_attachments(
        &self,
        _tenant_id: Uuid,
        _message_id: Uuid,
    ) -> anyhow::Result<Vec<MailAttachment>> {
        Ok(vec![])
    }
}
```

**Step 2: Export service**

In `backend/server/src/services/mod.rs`, add:

```rust
pub mod mail_service;
```

**Step 3: Commit**

```bash
git add backend/server/src/services/mail_service.rs backend/server/src/services/mod.rs
git commit -s -m "mail: add MailService skeleton

Refs #147"
```

---

## Task 4: Register mail module in the module registry

**Files:**
- Modify: `backend/server/src/services/module_service.rs`

**Step 1: Add mail entry to default_modules()**

Insert a new tuple into the `vec!` in `default_modules()`:

```rust
(
    "mail",
    "Mail",
    "Import, archive, and reference email inside RustShare workspaces.",
    "/Workspace/Mail",
    "mail-list",
    "template_default_mail_list",
    "mail",
    false,
    json!({
        "sidebar": {
            "enabled": true,
            "icon": "mail",
            "order": 60
        },
        "dashboard": {
            "enabled": true
        }
    }),
),
```

**Step 2: Commit**

```bash
git add backend/server/src/services/module_service.rs
git commit -s -m "mail: register disabled-by-default mail module

Refs #147"
```

---

## Task 5: Wire MailService into bootstrap and AppState

**Files:**
- Modify: `backend/server/src/bootstrap.rs`
- Modify: `backend/server/src/state.rs`

**Step 1: Add service to Services struct**

In `backend/server/src/bootstrap.rs`, locate the `Services` struct and add:

```rust
pub mail_service: Arc<crate::services::mail_service::MailService>,
```

**Step 2: Instantiate service in init_services()**

Add an async block inside `init_services()` that creates the service after metadata_store is available:

```rust
async {
    Arc::new(crate::services::mail_service::MailService::new(
        Arc::clone(&metadata_store),
    ))
}
```

Assign it to `services.mail_service`.

**Step 3: Add to AppState**

In `backend/server/src/state.rs`, add:

```rust
pub mail_service: Arc<crate::services::mail_service::MailService>,
```

Also add to `ServiceState` and its `FromRef<AppState>` implementation.

**Step 4: Commit**

```bash
git add backend/server/src/bootstrap.rs backend/server/src/state.rs
git commit -s -m "mail: wire MailService into bootstrap and AppState

Refs #147"
```

---

## Task 6: Add placeholder mail routes

**Files:**
- Create: `backend/server/src/handlers/mail.rs`
- Modify: `backend/server/src/handlers/mod.rs`
- Modify: `backend/server/src/routes.rs`
- Modify: `backend/server/src/main.rs`

**Step 1: Create handler file**

Create `backend/server/src/handlers/mail.rs`:

```rust
use axum::{extract::State, http::StatusCode, Json};
use serde_json::json;

use crate::state::AppState;

pub async fn list_mail_messages(State(_state): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::OK, Json(json!({ "messages": [] })))
}
```

**Step 2: Export handler**

In `backend/server/src/handlers/mod.rs`, add:

```rust
pub mod mail;
```

**Step 3: Add route group**

In `backend/server/src/routes.rs`, add:

```rust
pub fn mail_routes() -> Router<AppState> {
    use axum::routing::get;
    Router::new()
        .route("/api/v1/mail/messages", get(crate::handlers::mail::list_mail_messages))
}
```

**Step 4: Merge routes in main.rs**

In `backend/server/src/main.rs`, add `.merge(routes::mail_routes())` with the other route merges.

**Step 5: Commit**

```bash
git add backend/server/src/handlers/mail.rs backend/server/src/handlers/mod.rs backend/server/src/routes.rs backend/server/src/main.rs
git commit -s -m "mail: add placeholder mail routes

Refs #147"
```

---

## Task 7: Verify build and SQLx metadata

**Files:**
- Modify: `backend/.sqlx/*` (generated)

**Step 1: Format**

Run: `cd backend && cargo fmt`
Expected: No errors.

**Step 2: Check offline**

Run: `cd backend && SQLX_OFFLINE=true cargo check --workspace`
Expected: Compiles cleanly.

**Step 3: Run unit tests**

Run: `cd backend && SQLX_OFFLINE=true cargo test --workspace --lib`
Expected: Existing tests pass; no new test failures.

**Step 4: Prepare SQLx metadata**

Run: `cd backend && cargo sqlx prepare --workspace`
Expected: Generates/updates `.sqlx/*.json` files.

**Step 5: Verify SQLx metadata**

Run: `cd backend && cargo sqlx prepare --workspace --check`
Expected: Passes.

**Step 6: Commit SQLx metadata**

```bash
git add backend/.sqlx/
git commit -s -m "mail: update sqlx offline metadata

Refs #147"
```

---

## Task 8: Add unit test for domain type construction

**Files:**
- Create: `backend/crates/core/tests/mail_message_domain_test.rs`

**Step 1: Write failing test**

```rust
use rustshare_core::domain::{MailMessage, MailSourceMode, MailVisibility};
use uuid::Uuid;

#[test]
fn mail_message_defaults_to_private_visibility() {
    let msg = MailMessage::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        "eml_upload",
    );
    assert_eq!(msg.visibility, "private");
    assert_eq!(msg.source_mode, "eml_upload");
}
```

**Step 2: Run test to verify it fails**

Run: `cd backend && SQLX_OFFLINE=true cargo test --test mail_message_domain_test`
Expected: FAIL because the test file does not exist yet (or passes once created).

**Step 3: Add test file and run**

Create the file, then run:

Run: `cd backend && SQLX_OFFLINE=true cargo test --test mail_message_domain_test`
Expected: PASS.

**Step 4: Commit**

```bash
git add backend/crates/core/tests/mail_message_domain_test.rs
git commit -s -m "mail: add domain type construction test

Refs #147"
```

---

## Final verification

Run the full baseline validation:

```bash
cd backend
cargo fmt --check
SQLX_OFFLINE=true cargo clippy --all-features -- -D warnings
SQLX_OFFLINE=true cargo test --workspace --lib
cargo sqlx prepare --workspace --check
```

Expected: All pass.

Then push the branch.
