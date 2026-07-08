# RustShare Mail Phase 2 — Linking to RustShare Objects Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** Add the ability to link imported mail artifacts to Notes, Kanban cards, Kanban boards, Meetings, Files, and Folders with permission-aware access and audit events.

**Architecture:** Introduce a `mail_links` join table and a `MailLink` domain type with a `LinkTargetType` discriminator. Provide a small service layer that verifies the caller can read both the mail artifact and the target object before creating a link, emits `MailLinked`/`MailUnlinked` audit events, and exposes REST endpoints under `/api/v1/mail/messages/{id}/links`. The module stays disabled by default.

**Tech Stack:** Rust, Axum, SQLx, PostgreSQL, utoipa, existing RustShare permission resolver and event store.

---

## Context for implementers

- Worktree: `/Users/scolak/Projects/x/rustshare/.worktrees/mail-phase2-linking`
- Branch: `feat/mail-phase2-linking`
- Parent Phase 1 branch: `feat/mail-eml-upload`
- Epic: `docs/epics/0147-mail-module.md`
- Spec: `docs/specs/mail-module.md`
- ADR: `docs/adr/0032-mail-module-boundaries.md`
- All commits must be signed off (`git commit -s`) and use `Refs #147`, never `Fixes #147`.

Current Phase 1 state (from explore):

- `mail_messages`, `mail_message_parts`, `mail_attachments` tables exist.
- `MailService` lives in `backend/server/src/services/mail_service.rs` with `import_eml`, `get_message`, `list_attachments`.
- `MetadataStore` lives in `backend/crates/storage/src/metadata.rs` with mail query methods.
- Mail artifacts are file-backed under `/Workspace/Mail/YYYY-MM-{slug}-{short-uuid}/`.
- Attachments are already promoted to RustShare `File` artifacts.
- Notes/Kanban/Meetings are file/folder-backed modules; their IDs are `folders.id` or `files.id`.
- Permission checks use `PermissionResolver` on `Resource::File` / `Resource::Folder`.
- Audit events use `EventStore::append_in_tx` and the `EventType` enum in `backend/crates/core/src/events/types.rs`.

---

## Task 1: Data layer for mail links

**Files:**
- Create: `backend/migrations/20260708150001_create_mail_links_table.sql`
- Create: `backend/crates/core/src/domain/mail_link.rs`
- Modify: `backend/crates/core/src/domain/mod.rs`
- Modify: `backend/crates/core/src/domain/mail_message.rs` (add `MailLinkId` alias if not added via `mod.rs`)
- Modify: `backend/crates/storage/src/metadata.rs`
- Create: `backend/crates/core/tests/mail_link_domain_test.rs`

**Step 1: Write the migration**

Table `mail_links`:

```sql
CREATE TABLE IF NOT EXISTS mail_links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    message_id UUID NOT NULL REFERENCES mail_messages(id) ON DELETE CASCADE,
    target_type VARCHAR(50) NOT NULL,
    target_id UUID NOT NULL,
    created_by UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

CREATE INDEX idx_mail_links_message_id ON mail_links(message_id);
CREATE INDEX idx_mail_links_target ON mail_links(target_type, target_id);
CREATE INDEX idx_mail_links_tenant_id ON mail_links(tenant_id);
```

Use `deleted_at` soft-delete. Composite unique: active links are unique on `(message_id, target_type, target_id)` where `deleted_at IS NULL`.

**Step 2: Add domain types**

In `backend/crates/core/src/domain/mail_link.rs`:

```rust
pub type MailLinkId = Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LinkTargetType {
    Note,
    KanbanCard,
    KanbanBoard,
    Meeting,
    File,
    Folder,
    MailMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MailLink {
    pub id: MailLinkId,
    pub tenant_id: Uuid,
    pub message_id: MailMessageId,
    pub target_type: String,
    pub target_id: Uuid,
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}
```

Provide `LinkTargetType::as_str()` and `LinkTargetType::try_from(&str)` consistent with `MailSourceMode` / `MailVisibility` patterns.

**Step 3: Register IDs**

Add `pub type MailLinkId = Uuid;` in `backend/crates/core/src/domain/mod.rs` and re-export `MailLink`, `LinkTargetType`.

**Step 4: Add MetadataStore methods**

In `backend/crates/storage/src/metadata.rs`, add:

- `create_mail_link(&self, link: &MailLink) -> Result<(), StorageError>`
- `soft_delete_mail_link(&self, link_id: MailLinkId, deleted_by: UserId) -> Result<bool, StorageError>`
- `list_mail_links_by_message(message_id: MailMessageId, tenant_id: Uuid) -> Result<Vec<MailLink>, StorageError>`
- `find_mail_link_by_id(link_id: MailLinkId, tenant_id: Uuid) -> Result<Option<MailLink>, StorageError>`
- `find_active_mail_link(message_id, target_type, target_id, tenant_id) -> Result<Option<MailLink>, StorageError>`

Use SQLx query macros so offline metadata can be prepared. Reuse existing `StorageError` patterns.

**Step 5: Write domain unit tests**

Create `backend/crates/core/tests/mail_link_domain_test.rs`:

- `link_target_type_round_trips_strings`
- `mail_link_defaults_to_no_deleted_at`

**Step 6: Verify**

Run:

```bash
SQLX_OFFLINE=true cargo check --workspace
SQLX_OFFLINE=true cargo test -p rustshare-core --test mail_link_domain_test
```

**Step 7: Commit**

```bash
git add backend/migrations backend/crates/core/src/domain backend/crates/core/tests backend/crates/storage/src/metadata.rs
git commit -s -m "feat(mail): add mail_links table and data layer. Refs #147"
```

---

## Task 2: Service layer — permission-aware link/unlink and audit events

**Files:**
- Modify: `backend/server/src/services/mail_service.rs`
- Modify: `backend/crates/core/src/events/types.rs`
- Modify: `backend/crates/core/src/events/types.rs` payload definitions (same file)
- Modify: `backend/server/src/services/mod.rs` (if new error variants needed)
- Create: `backend/server/src/services/mail_service_link_tests.rs` (module tests inside mail_service.rs via `#[cfg(test)] mod link_tests`)

**Step 1: Add event types**

In `backend/crates/core/src/events/types.rs`:

- Add `MailLinked` and `MailUnlinked` variants to `EventType`.
- Add payload structs:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailLinkedPayload {
    pub message_id: Uuid,
    pub link_id: Uuid,
    pub target_type: String,
    pub target_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailUnlinkedPayload {
    pub message_id: Uuid,
    pub link_id: Uuid,
    pub target_type: String,
    pub target_id: Uuid,
}
```

Ensure serialization follows existing event payload conventions.

**Step 2: Extend MailService with PermissionResolver**

`MailService` currently holds `metadata_store`, `object_store`, `file_service`, `folder_service`. Add:

```rust
permission_resolver: Arc<PermissionResolver>
event_store: Arc<EventStore>
```

Update `MailService::new` and `backend/server/src/bootstrap.rs` wiring. Also update `backend/server/src/state.rs` if `ServiceState` construction needs it (likely already has these services available).

**Step 3: Add target-permission check**

Add private helper:

```rust
async fn require_target_read(
    &self,
    caller: UserId,
    target_type: &LinkTargetType,
    target_id: Uuid,
) -> Result<(), MailError>
```

Implementation:

- `File` target: `PermissionResolver::check_file_permission(target_id, caller, SharePermissions::View).await`.
- `Folder`, `Note`, `KanbanCard`, `KanbanBoard`, `Meeting` targets: treat as folder permission check on `target_id` (notes/cards/boards/meetings are folder-backed).
- `MailMessage` target: use existing `MailService::get_message` to verify caller can read the target mail.

Return `MailError::PermissionDenied` on failure.

**Step 4: Add link/unlink methods**

```rust
pub async fn link_message(
    &self,
    tenant_id: Uuid,
    caller: UserId,
    message_id: MailMessageId,
    target_type: LinkTargetType,
    target_id: Uuid,
) -> Result<MailLink, MailError>
```

Behavior:

1. Load source mail with `get_message(tenant_id, caller, message_id)` → checks owner/permission on mail.
2. `require_target_read(caller, target_type, target_id)`.
3. If an active link already exists, return it (idempotent create).
4. Build `MailLink` with `created_by = caller`.
5. Insert via `metadata_store.create_mail_link` inside a transaction with `EventStore::append_in_tx` for `MailLinked`.
6. Return link.

```rust
pub async fn unlink_message(
    &self,
    tenant_id: Uuid,
    caller: UserId,
    link_id: MailLinkId,
) -> Result<(), MailError>
```

Behavior:

1. Load link via `metadata_store.find_mail_link_by_id`.
2. Load source mail and verify caller can read it.
3. Soft-delete the link in a transaction and append `MailUnlinked` event.
4. Return Ok.

```rust
pub async fn list_message_links(
    &self,
    tenant_id: Uuid,
    caller: UserId,
    message_id: MailMessageId,
) -> Result<Vec<MailLink>, MailError>
```

Behavior:

1. Verify caller can read the mail.
2. Return active links for the message.

**Step 5: Add module tests**

Inside `backend/server/src/services/mail_service.rs` add a `#[cfg(test)] mod link_tests` block with mocked or real services. Because the permission resolver and event store require a database, mark tests that need them as `#[ignore]` and document the required env vars, or use the existing integration-test approach.

At minimum add a unit test for `LinkTargetType` parsing if not already covered.

**Step 6: Verify**

Run:

```bash
SQLX_OFFLINE=true cargo check --workspace
SQLX_OFFLINE=true cargo clippy --all-features -- -D warnings
SQLX_OFFLINE=true cargo test -p rustshare-server --lib
```

**Step 7: Commit**

```bash
git add backend/crates/core/src/events backend/server/src/services backend/server/src/bootstrap.rs backend/server/src/state.rs
git commit -s -m "feat(mail): permission-aware link/unlink with audit events. Refs #147"
```

---

## Task 3: API layer — handlers, routes, OpenAPI, integration tests

**Files:**
- Modify: `backend/server/src/handlers/mail.rs`
- Modify: `backend/server/src/routes.rs`
- Modify: `backend/server/src/openapi.rs`
- Create: `backend/tests/mail_linking_test.rs`
- Modify: `backend/server/src/handlers/mod.rs` (if adding error variants)
- Modify: `backend/.sqlx/` (run `cargo sqlx prepare --workspace`)

**Step 1: Add request/response DTOs**

In `backend/server/src/handlers/mail.rs`:

```rust
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateMailLinkRequest {
    pub target_type: String,
    pub target_id: Uuid,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MailLinkResponse {
    pub id: Uuid,
    pub message_id: Uuid,
    pub target_type: String,
    pub target_id: Uuid,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MailLinkListResponse {
    pub links: Vec<MailLinkResponse>,
}
```

**Step 2: Add handlers**

- `POST /api/v1/mail/messages/{id}/links` → `create_mail_link`
- `DELETE /api/v1/mail/messages/{id}/links/{link_id}` → `delete_mail_link`
- `GET /api/v1/mail/messages/{id}/links` → `list_mail_links`

Add utoipa annotations for all three. Use `AuthenticatedUser` extractor to get `user_id` and `tenant_id`.

**Step 3: Wire routes**

In `backend/server/src/routes.rs` inside `mail_routes()`:

```rust
.route("/api/v1/mail/messages/{id}/links", get(list_mail_links).post(create_mail_link))
.route("/api/v1/mail/messages/{id}/links/{link_id}", delete(delete_mail_link))
```

**Step 4: Register OpenAPI**

In `backend/server/src/openapi.rs`:

- Add handler function names to the `paths` macro list.
- Add request/response schemas to the `components` macro list.

**Step 5: Integration tests**

Create `backend/tests/mail_linking_test.rs` with these tests (all `#[ignore]` unless `DATABASE_URL` and object storage are configured, following `mail_import_test.rs` pattern):

- `create_mail_link_to_note_requires_read_on_note_and_mail`
- `create_mail_link_denied_when_user_cannot_read_target`
- `list_mail_links_returns_active_links`
- `delete_mail_link_soft_deletes_and_emits_audit_event`
- `duplicate_link_is_idempotent`

Use the existing test-support helpers for tenant/user setup if available; otherwise follow the pattern in `backend/tests/mail_import_test.rs`.

**Step 6: SQLx prepare and final verification**

Run:

```bash
cd backend
DATABASE_URL=postgres://scolak@localhost/rustshare_prepare SQLX_OFFLINE=true cargo sqlx prepare --workspace --check
cd ..
cargo fmt --check
SQLX_OFFLINE=true cargo clippy --all-features -- -D warnings
SQLX_OFFLINE=true cargo check --workspace
SQLX_OFFLINE=true cargo test --workspace --lib
```

If `cargo sqlx prepare --check` fails because new queries are not cached, run `DATABASE_URL=postgres://scolak@localhost/rustshare_prepare cargo sqlx prepare --workspace` (without `--check`) and commit the updated `backend/.sqlx/` files.

**Step 7: Commit**

```bash
git add backend/server/src/handlers backend/server/src/routes.rs backend/server/src/openapi.rs backend/tests backend/.sqlx
git commit -s -m "feat(mail): link/unlink REST API and integration tests. Refs #147"
```

---

## Out of scope

- IMAP account connection or selected import (Phase 3)
- Archive jobs (Phase 4)
- Search or AI/RAG indexing (Phase 5)
- SMTP / outbound sending (Phase 6)
- Converting mail content into a note body or Kanban card body (this is linking only; conversion flows may be Phase 2b)
- Frontend UI for linking (backend-only Phase 2)

---

## Verification checklist for the whole PR

- [ ] `cargo fmt --check` passes
- [ ] `SQLX_OFFLINE=true cargo clippy --all-features -- -D warnings` passes
- [ ] `SQLX_OFFLINE=true cargo check --workspace` passes
- [ ] `SQLX_OFFLINE=true cargo test --workspace --lib` passes
- [ ] `DATABASE_URL=... cargo sqlx prepare --workspace --check` passes
- [ ] New migration applies cleanly
- [ ] Domain unit tests pass
- [ ] Integration tests compile (`cargo test -p rustshare-server --test mail_linking_test --no-run`)
- [ ] All commits signed off and reference `#147`
