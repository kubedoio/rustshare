# RustShare Mail — Phase 1 Follow-Up Issues

> **Status:** Planning document — Phase 1 of the RustShare Mail module (issue #147) is too large for a single PR. This document splits it into smaller, reviewable follow-up issues.
>
> **For:** Epic `docs/epics/0147-mail-module.md`, ADR `docs/adr/0032-mail-module-boundaries.md`, Spec `docs/specs/mail-module.md`
>
> **Refs #147**

## Why Phase 1 is split

Phase 1 as defined in the epic includes:

- Data model for `mail_messages`, `mail_message_parts`, `mail_attachments`
- Mail artifact type and identity rules
- `.eml` upload/import
- Metadata extraction and normalization
- Object storage persistence for raw source and bodies
- Basic detail view
- Tests

After inspecting the RustShare codebase, this work is larger than a single reviewable PR because it touches:

- A new PostgreSQL migration with tenant-scoped tables and indexes
- SQLx offline metadata (`cargo sqlx prepare`) across the workspace
- A new Rust crate dependency for `.eml` parsing
- Object storage writes with content-addressed SHA-256 blobs and integrity checks
- File-backed artifact folder creation under `/Workspace/Mail/...`
- New Axum handlers, routes, OpenAPI entries, and module registry wiring
- Security controls (Bcc redaction, HTML sanitization, size limits)
- Unit and integration tests

To keep reviews focused and safe, Phase 1 is split into the follow-up issues below.

---

## Follow-up issue 1 — mail: add migrations and domain types for imported mail artifacts

**Description:**
Create the database schema and Rust domain types needed for imported mail artifacts.

**Scope:**

- PostgreSQL migration for:
  - `mail_messages`
  - `mail_message_parts`
  - `mail_attachments`
- Tenant-scoped columns (`tenant_id`, `workspace_id`), owner, soft-delete, content hash, and object storage key columns
- Rust domain structs in `backend/crates/core/src/domain/mail.rs` (or similar):
  - `MailMessage`, `MailMessagePart`, `MailAttachment`
- New ID newtypes or UUID fields consistent with existing domain types
- Basic `MailService` skeleton with store trait bounds
- Register the `mail` module in the module registry as a disabled-by-default module

**Acceptance criteria:**

- [ ] Migration applies cleanly against a fresh database and rolls back correctly (if `.down.sql` is provided).
- [ ] Domain structs compile and follow existing patterns (`Debug`, `Clone`, `Serialize`, `Deserialize`, `sqlx::FromRow`, `utoipa::ToSchema`).
- [ ] `cargo sqlx prepare --workspace --check` passes.
- [ ] No product behavior yet beyond schema and types.

**Suggested difficulty:** medium

**Dependencies:** None (Phase 0 epic/ADR/spec already merged).

---

## Follow-up issue 2 — mail: implement .eml parsing and metadata extraction

**Description:**
Add an `.eml` parser that produces normalized mail artifact metadata.

**Scope:**

- Add a MIME/RFC 822 parsing crate (recommended: `mailparse`) to `backend/crates/core/Cargo.toml`
- Implement parser in `backend/crates/core/src/services/mail_parser.rs` (or similar):
  - Extract `Message-ID`, `From`, `To`, `Cc`, `Bcc`, `Subject`, `Date`
  - Extract `In-Reply-To` and `References`
  - Extract plain-text body and HTML body
  - List attachments with filename, MIME type, and size
- Normalize headers to a consistent internal representation
- Add unit tests with sample `.eml` fixtures under `backend/crates/core/fixtures/` or `backend/tests/fixtures/`

**Acceptance criteria:**

- [ ] Parser compiles and handles simple single-part and multipart `.eml` files.
- [ ] Unit tests cover plain-text body extraction, HTML body extraction, and attachment enumeration.
- [ ] Bcc is preserved internally but marked as sensitive; parser output includes a flag for Bcc presence.
- [ ] No storage or API behavior yet; parser returns in-memory structures only.

**Suggested difficulty:** medium

**Dependencies:** Issue 1 (domain types exist).

---

## Follow-up issue 3 — mail: implement .eml upload endpoint and object storage persistence

**Description:**
Implement the `POST /api/v1/mail/upload` endpoint that ingests `.eml` files and persists them as RustShare mail artifacts.

**Scope:**

- Add `mail_routes()` in `backend/server/src/routes.rs`
- Implement multipart upload handler in `backend/server/src/handlers/mail.rs`
- Wire `MailService` in `backend/server/src/bootstrap.rs`
- In `MailService::upload_eml`:
  - Stream upload to a temporary file
  - Parse with the parser from issue 2
  - Compute SHA-256 of the raw `.eml` source
  - Write raw source, text body, and HTML body to object storage under content-addressed `blobs/{sha256}` keys
  - Create the file-backed artifact folder under `/Workspace/Mail/YYYY/MM/{message_id}-{slugified-subject}/`:
    - `source.eml`
    - `.rustshare.json` sidecar
    - `body.txt`
    - `body.html` (sanitized)
    - `attachments/` subfolder (contents in follow-up issue 4)
  - Insert `mail_messages` and `mail_message_parts` rows
  - Emit audit event `mail.message.imported`
- Enforce size limits and tenant/user scoping
- Add OpenAPI path/component entries

**Acceptance criteria:**

- [ ] `POST /api/v1/mail/upload` accepts multipart `.eml` upload and returns the created message ID.
- [ ] Imported artifact is queryable in `mail_messages`.
- [ ] Raw `.eml`, text body, and HTML body blobs exist in object storage and pass integrity checks.
- [ ] Artifact folder and sidecar are created under `/Workspace/Mail/...`.
- [ ] `cargo sqlx prepare --workspace --check` passes.
- [ ] Integration test uploads a sample `.eml` and verifies metadata and blob presence.

**Suggested difficulty:** large

**Dependencies:** Issues 1 and 2.

---

## Follow-up issue 4 — mail: implement imported mail detail endpoint

**Description:**
Add `GET /api/v1/mail/messages/{id}` and related endpoints for viewing an imported mail artifact.

**Scope:**

- `GET /api/v1/mail/messages/{message_id}` — returns normalized metadata and part list
- `GET /api/v1/mail/messages/{message_id}/source` — returns the original `source.eml`
- `GET /api/v1/mail/messages/{message_id}/body.txt` — returns plain-text body
- `GET /api/v1/mail/messages/{message_id}/body.html` — returns sanitized HTML body
- Enforce ownership/tenant permission (default private to importing user)
- Bcc redaction in API responses unless the caller is the owner
- HTML sanitization before display

**Acceptance criteria:**

- [ ] Detail endpoint returns metadata for an imported message.
- [ ] Source/body endpoints return the correct blobs with authorization enforced.
- [ ] Unauthorized users receive a permission error.
- [ ] Bcc is redacted for non-owners.
- [ ] HTML body is sanitized before return.
- [ ] Integration tests verify authorized access, unauthorized access, and Bcc redaction.

**Suggested difficulty:** medium

**Dependencies:** Issue 3 (upload/persistence works).

---

## Follow-up issue 5 — mail: extract and persist mail attachments as RustShare files

**Description:**
Save imported mail attachments as first-class RustShare file artifacts under the mail artifact folder.

**Scope:**

- Extract attachment bytes during `.eml` upload
- Compute SHA-256 and write attachment blobs to object storage
- Create RustShare `files` rows for each attachment under the artifact's `attachments/` folder
- Insert `mail_attachments` rows linking the message to file IDs
- Inherit safe permissions from the mail artifact
- Add `GET /api/v1/mail/messages/{message_id}/attachments` endpoint

**Acceptance criteria:**

- [ ] Uploading an `.eml` with attachments creates file records and `mail_attachments` rows.
- [ ] Attachments are reachable via the canonical file download routes.
- [ ] Attachment list endpoint returns metadata.
- [ ] Attachments inherit permissions from the parent mail artifact.
- [ ] Integration test verifies attachment extraction and download.

**Suggested difficulty:** medium

**Dependencies:** Issues 1, 2, and 3.

---

## Follow-up issue 6 — mail: add mail module integration tests and finalize sqlx metadata

**Description:**
Add end-to-end integration tests for the `.eml` import flow and ensure SQLx offline metadata is complete.

**Scope:**

- Integration test file `backend/tests/mail_upload.rs`:
  - Upload `.eml`
  - Verify metadata
  - Verify detail endpoint
  - Verify source/body endpoints
  - Verify attachment handling (if issue 5 is merged)
  - Verify permission denial
- Run `cargo sqlx prepare --workspace` and commit updated `.sqlx/` files
- Run `cargo fmt --check` and `SQLX_OFFLINE=true cargo clippy --all-features -- -D warnings`
- Add a short `docs/implementation/mail-phase1-notes.md` implementation note

**Acceptance criteria:**

- [ ] New integration tests pass against a running PostgreSQL and S3-compatible store.
- [ ] SQLx offline metadata is up to date.
- [ ] Formatting and clippy pass with no warnings.
- [ ] Implementation note documents the Phase 1 scope, known limitations, and next steps.

**Suggested difficulty:** medium

**Dependencies:** Issues 1–5 (or 1–4 if attachments are deferred).

---

## Suggested PR grouping

| PR | Issues | Description |
|---|---|---|
| PR 1 | 1 + 2 | Schema, domain types, and `.eml` parser |
| PR 2 | 3 + 4 | Upload/import, object storage persistence, and detail API |
| PR 3 | 5 + 6 | Attachments, integration tests, and final QA |

If the team prefers smaller PRs, each issue can be its own PR.

---

## Out of scope for these follow-up issues

- IMAP account connection or selected import (Phase 3)
- Archive jobs (Phase 4)
- Search or AI/RAG indexing (Phase 5)
- SMTP / outbound sending (Phase 6)
- Linking mail to notes, meetings, or Kanban cards (Phase 2)
- Webmail-like UI beyond a basic detail view

---

## Implementation notes

- Use `Refs #147` in all follow-up issue descriptions and commit messages. Do not use `Fixes #147`.
- Follow the existing RustShare patterns for tenant isolation, object storage, file-backed artifacts, and audit events.
- Keep the `mail` module disabled by default until the full Phase 1 set is merged and tested.
