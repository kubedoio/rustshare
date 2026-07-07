# Specification: Mail Module

Status: Draft
Owner: RustShare Core Team
Related ADRs: ADR-0032, ADR-0016, ADR-0017, ADR-0018, ADR-0020, ADR-0021, ADR-0025, ADR-0031
Related Epic: Epic-0147

## 1. Purpose

This specification defines the **RustShare Mail module**: a file-backed, permission-aware capability for importing, storing, viewing, searching, linking, and sharing email artifacts inside a RustShare workspace. It is a companion document to `docs/adr/0032-mail-module-boundaries.md` and `docs/epics/0147-mail-module.md`.

This spec defines:

- The Mail module data model and sidecar formats.
- The imported **mail artifact** format and normalization rules.
- API surface shape, storage strategy, and permission model.
- IMAP integration behavior, security controls, and audit requirements.
- UI/UX structure and integration with Notes, Meetings, Kanban, and Files.
- AI/RAG indexing boundaries for imported mail.

This spec does **not** define:

- A live Mail Transfer Agent (MTA), SMTP submission server, outbound email relay, or hosted inbox service.
- Real-time bidirectional mailbox sync (a future extension if inbound addresses are added).
- Replacements for Gmail, Exchange, Roundcube, or other full email suites.

Use only the names **Mail**, **RustShare Mail**, or **Mail module** in user-facing text, API names, and documentation.

## 2. Core concepts

### 2.1 Mail artifact

A **mail artifact** is an immutable, file-backed RustShare object that represents one imported email message. It preserves the original RFC 822/MIME source and carries normalized metadata sidecars so the WebUI can render, search, and link it without re-parsing MIME at read time.

### 2.2 Live mailbox connection vs imported artifact

| Concept | Description |
|---------|-------------|
| **Live mailbox connection** | An authenticated IMAP session to a user-controlled external server. RustShare Mail can list folders and messages, but it does not host the mailbox. |
| **Imported artifact** | A durable RustShare object created by importing selected messages or running an import/archive job. Imported artifacts are the only mail objects that participate in workspace permissions, search, RAG, and sharing. |

### 2.3 Mail source modes

| Mode | Key | Description |
|------|-----|-------------|
| `.eml` upload | `eml_upload` | User uploads one or more `.eml` files. Each file becomes a standalone mail artifact. |
| IMAP selected import | `imap_selected` | User selects messages from a connected IMAP account and imports them explicitly. |
| IMAP archive job | `imap_archive` | User-defined job (folder + date range + filters) that imports messages asynchronously. |
| Future inbound address | `inbound_address` | Reserved for future receive-via-alias behavior. Not required for the initial release. |

### 2.4 Import job

An **import job** records the configuration, progress, and outcome of importing one or more messages. Jobs are durable and resumable. A job can be triggered manually or scheduled to run periodically.

## 3. Data model

The Mail module stores metadata in PostgreSQL during the Public Preview migration, but it must be sidecar-first and aligned with RustShare's zero-PostgreSQL direction. Wherever possible, durable metadata lives in object-storage sidecars; PostgreSQL tables are queryable projections.

### 3.1 `mail_accounts`

Represents an external IMAP account connection owned by a RustShare user.

```json
{
  "id": "acc_01J2P4K8X7MZQ",
  "tenant_id": "ten_123",
  "workspace_id": "ws_456",
  "owner_user_id": "usr_789",
  "label": "Support inbox",
  "source_mode": "imap_archive",
  "protocol": "imap",
  "host": "imap.example.com",
  "port": 993,
  "username": "support@example.com",
  "credential_ref": "vault:mail-creds/acc_01J2P4K8X7MZQ",
  "tls_verify": true,
  "oauth_provider": null,
  "last_connected_at": "2026-07-06T14:00:00Z",
  "last_error_at": null,
  "last_error_message": null,
  "created_at": "2026-07-01T10:00:00Z",
  "updated_at": "2026-07-06T14:00:00Z"
}
```

Fields:

- `id` — stable account identifier.
- `tenant_id`, `workspace_id`, `owner_user_id` — ownership and isolation.
- `label` — user-provided display name.
- `source_mode` — `imap_selected`, `imap_archive`, `eml_upload`, or `inbound_address`.
- `protocol` — `imap` for IMAP accounts; may extend later.
- `host`, `port` — server endpoint.
- `username` — login identity.
- `credential_ref` — reference to an encrypted secret store entry; never plaintext.
- `tls_verify` — require valid TLS certificates when true.
- `oauth_provider` — optional OAuth provider key.
- `last_connected_at`, `last_error_at`, `last_error_message` — operational state.

### 3.2 `mail_messages`

Query projection for imported mail artifacts.

```json
{
  "id": "msg_01J2P4K9A1B2C",
  "tenant_id": "ten_123",
  "workspace_id": "ws_456",
  "owner_user_id": "usr_789",
  "account_id": "acc_01J2P4K8X7MZQ",
  "source_mode": "imap_archive",
  "source_folder": "INBOX/Support",
  "source_uid": 1842,
  "message_id": "<abc123@example.com>",
  "in_reply_to": "<parent456@example.com>",
  "references": ["<parent456@example.com>"],
  "subject": "Q3 budget approval",
  "from": [{"name": "Alice", "address": "alice@example.com"}],
  "to": [{"name": "Support", "address": "support@example.com"}],
  "cc": [],
  "bcc": [],
  "date": "2026-07-05T09:30:00Z",
  "imported_at": "2026-07-06T14:05:00Z",
  "imported_by": "usr_789",
  "visibility": "private",
  "object_key": "tenants/ten_123/workspaces/ws_456/mail/msg_01J2P4K9A1B2C/artifact.json",
  "blob_key": "blobs/{sha256-of-rfc822-source}",
  "blob_sha256": "aabbccdd...",
  "size_bytes": 24580,
  "has_attachments": true,
  "thread_id": "th_01J2P4K9A1B2D",
  "deleted_at": null,
  "created_at": "2026-07-06T14:05:00Z",
  "updated_at": "2026-07-06T14:05:00Z"
}
```

Fields:

- `id` — stable artifact identifier.
- `account_id` — nullable; links to IMAP account for IMAP-derived artifacts.
- `source_mode`, `source_folder`, `source_uid` — provenance.
- `message_id`, `in_reply_to`, `references`, `subject`, `from`, `to`, `cc`, `bcc`, `date` — normalized headers.
- `blob_key`, `blob_sha256`, `size_bytes` — content-addressed RFC 822 source.
- `object_key` — path to the canonical artifact sidecar.
- `visibility` — `private`, `workspace`, `project`, or `admin_archive`.
- `thread_id` — optional conversation grouping derived from `References`/`In-Reply-To`.

### 3.3 `mail_message_parts`

Sidecar stored alongside the artifact. Lists parsed MIME parts and their storage references.

```json
{
  "message_id": "msg_01J2P4K9A1B2C",
  "parts": [
    {
      "part_id": "1",
      "content_type": "text/plain",
      "charset": "utf-8",
      "blob_key": "blobs/{sha256-plain-text}",
      "sha256": "...",
      "size_bytes": 1240,
      "is_body": true
    },
    {
      "part_id": "2",
      "content_type": "text/html",
      "charset": "utf-8",
      "blob_key": "blobs/{sha256-html-body}",
      "sha256": "...",
      "size_bytes": 3840,
      "is_body": true,
      "sanitized": true
    }
  ]
}
```

### 3.4 `mail_attachments`

Attachment projection. Each imported attachment is stored as a normal RustShare file and linked back to the mail artifact.

```json
{
  "id": "att_01J2P4K9A1B2E",
  "message_id": "msg_01J2P4K9A1B2C",
  "filename": "budget.xlsx",
  "mime_type": "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
  "size_bytes": 18420,
  "file_id": "file_01J2P4K9A1B2F",
  "folder_id": "folder_01J2P4K9A1B2G",
  "part_index": 3,
  "content_disposition": "attachment",
  "blob_key": "blobs/{sha256-attachment}",
  "created_at": "2026-07-06T14:05:00Z"
}
```

The referenced `file_id` is a first-class RustShare file stored under the mail artifact's `attachments/` folder, following ADR-0021. This makes attachments visible in the file tree, exportable, and governed by the standard permission model.

### 3.5 `mail_links`

Links a mail artifact to another RustShare object (note, meeting, kanban card, file, decision).

```json
{
  "id": "link_01J2P4K9A1B2H",
  "message_id": "msg_01J2P4K9A1B2C",
  "target_type": "note",
  "target_id": "note_01J2P4K9A1B2I",
  "created_by": "usr_789",
  "created_at": "2026-07-06T14:10:00Z"
}
```

### 3.6 `mail_import_jobs`

Tracks import job state.

```json
{
  "id": "job_01J2P4K9A1B2J",
  "tenant_id": "ten_123",
  "workspace_id": "ws_456",
  "owner_user_id": "usr_789",
  "account_id": "acc_01J2P4K8X7MZQ",
  "source_mode": "imap_archive",
  "status": "running",
  "filters": {
    "folders": ["INBOX/Support"],
    "since": "2026-06-01T00:00:00Z",
    "before": "2026-07-06T00:00:00Z",
    "keywords": ["budget"]
  },
  "cursor": {
    "folder": "INBOX/Support",
    "last_uid": 1842
  },
  "total_messages": 500,
  "processed_messages": 184,
  "imported_messages": 182,
  "failed_messages": 2,
  "failures": [
    {
      "uid": 1201,
      "error": "attachment too large",
      "at": "2026-07-06T14:04:00Z"
    }
  ],
  "started_at": "2026-07-06T14:00:00Z",
  "completed_at": null,
  "created_at": "2026-07-06T14:00:00Z"
}
```

## 4. Mail artifact format

Each imported mail artifact is stored as a RustShare folder:

```text
/Workspace/Mail/2026/07/msg_01J2P4K9A1B2C-q3-budget-approval/
  source.eml                 # original RFC 822/MIME bytes
  .rustshare.json            # mail artifact metadata sidecar
  body.txt                   # extracted plain-text body
  body.html                  # sanitized HTML body (optional)
  attachments/
    budget.xlsx
    logo.png
```

### 4.1 Required artifact contents

- **RFC 822/MIME source** — preserved verbatim in `source.eml`. This is the durable original and the source of truth for re-parsing or export.
- **Normalized metadata** in `.rustshare.json`:
  - `message_id`, `in_reply_to`, `references`
  - `from`, `to`, `cc`, `bcc` (name/address arrays)
  - `subject`
  - `date` (RFC 2822 date normalized to UTC)
  - `folder_source` (IMAP folder or `eml_upload`)
  - `import_timestamp`, `imported_by`
  - `workspace_id`, `owner_user_id`, `tenant_id`
  - `account_id` when imported via IMAP
- **Text body** — extracted plain text from the best `text/plain` part, with charset normalized to UTF-8.
- **Sanitized HTML body** — if a `text/html` part exists, stored after HTML sanitization (see section 9).
- **Attachments** — stored as RustShare files under `attachments/`, each with a `mail_attachments` projection row.
- **Checksum** — SHA-256 of `source.eml`, recorded in `mail_messages.blob_sha256` and verified on read.
- **Source account reference** — `account_id` for IMAP-derived artifacts.

### 4.2 `.rustshare.json` example

```json
{
  "rustshare": {
    "id": "msg_01J2P4K9A1B2C",
    "type": "mail_message",
    "version": 1
  },
  "message_id": "<abc123@example.com>",
  "in_reply_to": "<parent456@example.com>",
  "references": ["<parent456@example.com>"],
  "subject": "Q3 budget approval",
  "from": [{"name": "Alice", "address": "alice@example.com"}],
  "to": [{"name": "Support", "address": "support@example.com"}],
  "cc": [],
  "bcc": [],
  "date": "2026-07-05T09:30:00Z",
  "folder_source": "INBOX/Support",
  "source_mode": "imap_archive",
  "import": {
    "timestamp": "2026-07-06T14:05:00Z",
    "imported_by": "usr_789",
    "account_id": "acc_01J2P4K8X7MZQ",
    "job_id": "job_01J2P4K9A1B2J"
  },
  "blobs": {
    "source": {
      "key": "blobs/{sha256-of-rfc822-source}",
      "sha256": "aabbccdd...",
      "size_bytes": 24580
    },
    "body_text": {
      "key": "blobs/{sha256-plain-text}",
      "sha256": "...",
      "size_bytes": 1240
    },
    "body_html": {
      "key": "blobs/{sha256-html-body}",
      "sha256": "...",
      "size_bytes": 3840,
      "sanitized": true
    }
  },
  "attachments": [
    {
      "filename": "budget.xlsx",
      "mime_type": "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
      "file_id": "file_01J2P4K9A1B2F"
    }
  ]
}
```

## 5. API surface

The Mail module exposes a JSON API under `/api/v1/mail`. The backend auto-generates OpenAPI as described in `docs/specs/openapi-auto-generation.md`; the operations below define the required shape.

### 5.1 Upload `.eml`

```text
POST /api/v1/mail/upload
Content-Type: multipart/form-data
```

Parameters:

- `files` — one or more `.eml` files.
- `visibility` — `private` (default), `workspace`, `project`, or `admin_archive`.
- `folder_id` — optional destination folder; defaults to the user's Mail module root.

Response: list of created message IDs and import status per file.

### 5.2 Create IMAP account connection

```text
POST /api/v1/mail/accounts
```

Request body:

```json
{
  "label": "Support inbox",
  "host": "imap.example.com",
  "port": 993,
  "username": "support@example.com",
  "password": "app-specific-password",
  "tls_verify": true,
  "oauth_provider": null
}
```

Response: account object without credential material.

### 5.3 List connected accounts

```text
GET /api/v1/mail/accounts
```

Returns accounts owned by the caller in the workspace, omitting secrets.

### 5.4 List IMAP folders/messages

```text
GET /api/v1/mail/accounts/{account_id}/folders
GET /api/v1/mail/accounts/{account_id}/folders/{folder}/messages?limit=50&offset=0
```

Returns folder list and message envelope metadata for selection. These endpoints operate against the live IMAP server; no artifact is created until the user explicitly imports.

### 5.5 Import selected messages

```text
POST /api/v1/mail/accounts/{account_id}/import
```

Request body:

```json
{
  "folder": "INBOX/Support",
  "uids": [1840, 1841, 1842],
  "visibility": "workspace",
  "destination_folder_id": "folder_01J2P4K9A1B2G"
}
```

Response: import job ID and initial status.

### 5.6 Get imported message detail

```text
GET /api/v1/mail/messages/{message_id}
GET /api/v1/mail/messages/{message_id}/source
GET /api/v1/mail/messages/{message_id}/body.txt
GET /api/v1/mail/messages/{message_id}/body.html
```

The detail endpoint returns normalized metadata and part list. Source and body endpoints return the corresponding blobs with authorization enforced.

### 5.7 List attachments

```text
GET /api/v1/mail/messages/{message_id}/attachments
```

Returns attachment metadata; downloads use the canonical file routes (`GET /api/v1/files/{file_id}/content`).

### 5.8 Link mail to note/meeting/kanban/file

```text
POST /api/v1/mail/messages/{message_id}/links
DELETE /api/v1/mail/messages/{message_id}/links/{link_id}
```

Request body:

```json
{
  "target_type": "note",
  "target_id": "note_01J2P4K9A1B2I"
}
```

### 5.9 Share/unshare mail artifact

```text
POST /api/v1/mail/messages/{message_id}/shares
PATCH /api/v1/mail/messages/{message_id}/shares/{share_id}
DELETE /api/v1/mail/messages/{message_id}/shares/{share_id}
```

Shares reuse RustShare's existing share primitives (see `docs/adr/0031-tenant-isolation-share-links-and-rls.md`). Public share rendering must exclude hidden metadata and source `.eml` unless explicitly allowed.

### 5.10 Get import job status

```text
GET /api/v1/mail/jobs/{job_id}
GET /api/v1/mail/jobs?account_id=...&status=...
POST /api/v1/mail/jobs/{job_id}/cancel
```

### 5.11 Search imported mail

```text
GET /api/v1/mail/messages/search?q=budget&from=alice@example.com&folder=INBOX/Support&since=2026-06-01
```

Search queries the indexed metadata projection and, if enabled, indexed text bodies (see section 12).

## 6. Storage strategy

The Mail module follows RustShare's file-backed, sidecar-first direction (ADR-0016, ADR-0021, ADR-0025).

- **RFC 822 source** — stored as a content-addressed blob at `blobs/{sha256}` in S3-compatible/RustFS object storage.
- **Artifact folder** — stored under `/Workspace/Mail/YYYY/MM/{message_id}-{slugified-subject}/` with `source.eml`, `.rustshare.json`, `body.txt`, `body.html`, and `attachments/`.
- **Metadata sidecars** — `.rustshare.json` is the durable metadata source; PostgreSQL `mail_messages` is a queryable projection rebuilt by scanning sidecars.
- **Attachments** — stored as normal RustShare files in the artifact's `attachments/` folder. Each attachment file is also content-addressed in object storage.
- **Content-addressed blobs** — deduplicate identical message sources, bodies, and attachments. SHA-256 verification on read/write is mandatory.
- **Backup/restore** — because artifacts are files and content-addressed blobs, standard RustShare backup scripts (`scripts/backup-stack.sh`) capture Mail module data. Restore preserves message IDs and blob integrity.

## 7. Permission model

Default visibility for all imported mail artifacts is **private** to the importing user.

### 7.1 Visibility levels

| Level | Meaning |
|-------|---------|
| `private` | Only the owner can read, link, share, or delete. |
| `workspace` | Readable by workspace members; only the owner can delete or change visibility. |
| `project` | Readable by members of a designated project/group; owner-controlled. |
| `admin_archive` | Readable by workspace admins; owner cannot delete without admin override. |

### 7.2 Sharing

- Explicit internal shares and public links follow the existing RustShare share model.
- Sharing a mail artifact does not automatically expose its attachments unless the share explicitly includes the `attachments/` folder.
- Changing visibility from `workspace` to `private` revokes workspace read access but preserves existing explicit shares until revoked.

### 7.3 Linked-object permission inheritance

- A link from a mail artifact to a note/meeting/kanban/file does not grant permission to either object.
- Effective access is always the intersection of the caller's permissions on the mail artifact and the target object.
- A private mail linked to a workspace note remains private; the note renderer shows only the link title and a permission gate.

### 7.4 Admin boundaries

- Workspace admins can list and manage mail accounts and import jobs in their workspace.
- Admins can read `admin_archive` artifacts but cannot read private user mail without an explicit share or ownership transfer.
- Tenant isolation (ADR-0031) applies to all mail tables and sidecars.

## 8. IMAP integration

### 8.1 Connection settings

- Protocol: IMAP4rev1/STARTTLS or IMAPS on port 993.
- `host`, `port`, `username`, and encrypted password or OAuth token are stored per account.
- `tls_verify` defaults to true; disabling it logs a security warning.

### 8.2 Credential storage

- Passwords and OAuth tokens are encrypted with `RUSTSHARE_SECRET_ENCRYPTION_KEY` (AES-256-GCM) before storage, consistent with `docs/security-model.md`.
- Credentials are never returned by list/get endpoints.
- OAuth support is optional for the initial release; the schema reserves `oauth_provider` and `oauth_refresh_token_ref`.

### 8.3 Folder listing and selected import

- `LIST` returns selectable mailboxes; hidden system folders may be omitted.
- Selected import fetches full RFC 822 sources for the chosen UIDs and creates artifacts synchronously or via a short-lived job.

### 8.4 Archive jobs

- Jobs define `folders`, `since`, `before`, `keywords`, `sender`, and `has_attachments` filters.
- Jobs run incrementally and persist a UID cursor per folder in `mail_import_jobs.cursor`.
- Running the same job again skips already-imported UIDs by tracking `(account_id, folder, uid)` in a deduplication projection.

### 8.5 Retry and failure handling

- Transient IMAP errors are retried with exponential backoff up to a configured maximum.
- Permanent errors (authentication failure, TLS failure) mark the account in `last_error_*` and pause the job.
- Individual message parse failures are recorded in `failures` without aborting the whole job.
- Jobs can be cancelled via `POST /api/v1/mail/jobs/{job_id}/cancel`.

## 9. Security and privacy

### 9.1 Secret handling

- No plaintext passwords in database, sidecars, or logs.
- IMAP credentials use the same encrypted secret store as share passwords and OIDC secrets (`docs/security-model.md`).

### 9.2 Audit events

All security-relevant Mail module actions produce audit events (see section 13).

### 9.3 HTML sanitization

- Stored HTML bodies are sanitized before persistence and again before rendering if the sanitizer version changes.
- Sanitization removes scripts, event handlers, unsafe CSS, and `javascript:` URLs, consistent with `docs/specs/security-and-permissions.md`.
- External images and remote content are blocked by default; the UI may offer an opt-in with a warning.

### 9.4 Attachment scanning

- Virus/malware scanning is **not** implemented in core RustShare.
- The API accepts an optional `scan_result` field on import for future integration with external scanners.
- Untrusted attachment downloads are served with `Content-Disposition: attachment` and appropriate MIME headers.

### 9.5 Size and rate limits

- Per-message size limit follows `MAX_UPLOAD_SIZE_MB` (default 5000 MB) or a Mail-specific `RUSTSHARE_MAIL_MAX_MESSAGE_SIZE_MB`.
- Per-account import jobs are rate-limited to prevent IMAP server abuse.
- Public share download limits follow existing share rate limits.

### 9.6 Consent and redaction

- Users must explicitly connect external accounts; connection creation is audited.
- Owners can delete imported artifacts and their attachments; deletion is soft-deleted then purged per workspace retention policy.
- Export of mail artifacts includes the original `source.eml` and sidecars; owners can export their own private mail.

### 9.7 Deletion/export

- Deleting an artifact removes the sidecar folder and marks blobs for garbage collection if unreferenced.
- Export returns a ZIP containing `source.eml`, `.rustshare.json`, `body.txt`, and `attachments/`.

## 10. UI/UX

The Mail module is rendered through the Module Registry (ADR-0017, ADR-0018). It appears as:

- **Sidebar:** "Mail" icon below enabled module navigation when `module.ui.sidebar.enabled` is true.
- **Dashboard:** a module card showing recent imported messages and account/import-job status.
- **Module page:** `/modules/mail` with tabs for Accounts, Import, Imported Messages, and Job Status.

### 10.1 Views

- **Accounts:** list connected IMAP accounts, test connection, edit label, delete account.
- **Import:** upload `.eml`, browse IMAP folders, select messages, configure archive jobs.
- **Imported messages:** searchable list with subject, sender, date, folder source, and attachment indicator.
- **Detail view:** header, sender/recipients, date, sanitized HTML or plain-text body, attachment list, link/share actions.
- **Links panel:** create and remove links to Notes, Meetings, Kanban cards, and Files.
- **Share panel:** internal shares and public links; warn that sharing may expose message content.
- **Job status:** progress, cursor, failures, cancel/retry.

### 10.2 Empty states

- No accounts: prompt to connect an account or upload `.eml`.
- No imported messages: explain source modes and privacy defaults.

## 11. Integration with Notes, Meetings, Kanban, Files

### 11.1 Link semantics

- A mail link is a directional association: "this message is related to that object."
- Links are stored in `mail_links` and mirrored in the target object's sidecar where applicable (e.g., a note's `.rustshare.json` may list `related_mail`).
- Links do not copy content and do not change permissions.

### 11.2 Conversion flows

- **Mail to note:** create a new note pre-filled with sanitized subject, sender, date, and quoted body; include a link back to the mail artifact. The note is owned by the converter.
- **Mail to meeting:** create a meeting note from a selected message; subject becomes meeting title suggestion; body becomes context.
- **Mail to kanban card:** create a card with title/subject and link; attachments may be copied to the card's `attachments/` folder if the user has permission.
- **Mail to file:** attachments are already files; no extra conversion needed.

### 11.3 Attachment inheritance

- Converting a mail artifact to another object copies attachment file references only when the user explicitly selects attachments and has read access.
- Attachments copied to a new object are new RustShare files; the originals remain governed by the mail artifact's permissions.

### 11.4 No automatic exposure

- Private mail is never exposed through linked objects.
- Module renderers must check effective access before embedding mail snippets or attachment previews.

## 12. AI / RAG boundaries

Mail indexing follows the permission-aware RAG rules in ADR-0020.

### 12.1 Indexing policies

Per mail artifact or workspace policy:

| Policy | Behavior |
|--------|----------|
| `disabled` | No indexing; not searchable by AI. |
| `metadata_only` | Index subject, sender, recipients, date, folder; not body or attachments. |
| `selected_messages` | Index metadata + body for explicitly selected messages. |
| `workspace_approved` | Index workspace-visible or explicitly approved mail only. |

### 12.2 Permission enforcement

- Every indexed chunk must include `tenant_id`, `workspace_id`, `message_id`, `read_acl`, `visibility`, `acl_hash`, and `acl_version` (ADR-0020).
- Retrieval must pre-filter by ACL metadata before chunks enter the model context.
- Permission changes enqueue an ACL projection update for all chunks belonging to the artifact.

### 12.3 Citations

- AI answers that use mail content must cite the message ID and a human-readable reference (subject, sender, date).
- Citations link to the artifact detail view if the user has read access.

### 12.4 Deletion and revocation

- Deleting an artifact or revoking access must remove or exclude its chunks from retrieval.
- Stale chunks (older `acl_version`) are filtered out at retrieval time.

## 13. Audit events

Mail module actions append events to the durable event log, consistent with `docs/security-model.md`.

| Action | Event type | Actor | Payload |
|--------|------------|-------|---------|
| IMAP account connected | `mail.account.connected` | User | account_id, host, tls_verify |
| IMAP account disconnected/deleted | `mail.account.disconnected` | User | account_id |
| Account credential updated | `mail.account.credentials_updated` | User | account_id |
| Import job created | `mail.import.created` | User | job_id, account_id, filters |
| Import job completed | `mail.import.completed` | System | job_id, counts |
| Import job failed | `mail.import.failed` | System | job_id, error |
| Message imported | `mail.message.imported` | User/job | message_id, source_mode, account_id |
| Message deleted | `mail.message.deleted` | User | message_id |
| Message shared | `mail.message.shared` | User | message_id, share_id, permission |
| Message unshared | `mail.message.unshared` | User | message_id, share_id |
| Message linked | `mail.message.linked` | User | message_id, target_type, target_id |
| Message unlinked | `mail.message.unlinked` | User | message_id, link_id |
| Message exported | `mail.message.exported` | User | message_id |

## 14. Implementation phases

Summarized from Epic-0147. Each phase delivers a testable increment. Issue #147 remains open as an epic; implementation must happen through smaller follow-up issues.

| Phase | Focus | Deliverables and tests |
|-------|-------|------------------------|
| 0 | Specification and Architecture | This epic, the boundaries ADR, and this spec; module registry entry for `mail`; artifact folder layout; sidecar schema. |
| 1 | Mail Artifact Foundation with `.eml` upload/import | Upload endpoint, MIME parsing, artifact creation, body extraction, attachment storage, basic detail view, tests. |
| 2 | Linking to RustShare Objects | Link mail to notes, Kanban cards, and meeting notes; attachments as RustShare file artifacts; share/unshare; conversion flows; permission inheritance tests. |
| 3 | IMAP Selected Import | Encrypted account configuration, connection test, folder/message listing, selected-message import, UID deduplication, import job status. |
| 4 | Archive Jobs | Folder/date-range archive jobs, incremental cursor, retention policy support, retry/failure handling, job status UI. |
| 5 | Search and Permission-Aware Indexing | Full-text search over imported mail, optional indexing policies, permission-aware retrieval hooks, source citations. |
| 6 | Webmail-Like Enhancements | Threaded view, reply/forward draft integration, richer mailbox browsing, optional outbound email support (future). |

## 15. Acceptance Criteria

- [ ] The file `docs/specs/mail-module.md` exists and follows repository spec conventions.
- [ ] The spec uses only "Mail", "RustShare Mail", and "Mail module" as product names.
- [ ] Data model includes `mail_accounts`, `mail_messages`, `mail_message_parts`, `mail_attachments`, `mail_links`, and `mail_import_jobs` with JSON examples.
- [ ] Mail artifact format preserves RFC 822/MIME source, normalized metadata, text/HTML bodies, and attachments as RustShare files.
- [ ] API surface covers upload, account management, folder/message listing, selected import, detail, attachments, links, shares, job status, and search.
- [ ] Storage strategy places durable data in content-addressed S3/RustFS blobs and sidecars, with PostgreSQL as a projection.
- [ ] Permission model defaults to private and defines workspace/project/admin_archive with explicit sharing and no automatic exposure via links.
- [ ] IMAP integration specifies TLS verification, encrypted credentials, app password/OAuth support, folder import, archive jobs, and retry behavior.
- [ ] Security section requires no plaintext passwords, HTML sanitization, attachment scanning placeholder, size/rate limits, and deletion/export behavior.
- [ ] UI/UX describes dashboard, accounts, import, message list, detail, links, share, and job-status views.
- [ ] Integration section defines link semantics, conversion flows, and attachment inheritance without exposing private content.
- [ ] AI/RAG section defines indexing policies, permission-aware chunk metadata, citations, and deletion/revocation removal.
- [ ] Audit events table covers account, import, message, share, link, and export actions.
- [ ] Implementation phases 0-6 are summarized with deliverables and test expectations.
