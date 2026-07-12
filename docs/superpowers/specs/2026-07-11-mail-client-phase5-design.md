# Mail Client Phase 5 Design

## Goal

Deliver a minimal, read-only mail client inside RustShare: users can list imported mail messages, open a message viewer that shows headers, a sanitized HTML or plain-text body, and attachments, download the raw `.eml` source, and see existing links to other workspace objects.

## Scope

### In scope
- Backend read endpoints for message parts, raw source, and attachment metadata.
- Server-side HTML sanitization for `text/html` body parts.
- A `MailMessageViewed` audit event emitted when a message body or source is read.
- A mail-specific dashboard summary mode that counts imported messages.
- Frontend module registration (`mail` icon, `mail-list` renderer).
- `MailModuleView` message list.
- Dedicated message detail page under `/modules/mail/messages/{messageId}`.
- Frontend API client (`frontend/src/lib/api/mail.ts`).

### Out of scope
- IMAP account/folder setup UI (backend endpoints already exist).
- Import/archive job creation UI (backend endpoints already exist).
- Compose, reply, or forward (SMTP client is Phase 6).
- Full-text/semantic search or RAG indexing (separate phase).
- Workspace-level sharing or visibility changes; mail remains `Private` to the owner.

## Architecture

### Backend

New metadata query and service implementations live in the existing mail service stack. All new handlers follow the pattern in `backend/server/src/handlers/mail.rs`:

1. `require_mail_enabled(&state, tenant_id)`
2. Load and ownership-check the message via `MailService::get_message`.
3. Fetch the requested blob/part/attachment from the metadata store or object store.
4. For HTML body parts, sanitize with `ammonia` before streaming the response.
5. Emit `MailMessageViewed` when a body part or source is served.

New HTTP surface:

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/api/v1/mail/messages/{id}/parts` | List `MailMessagePart` metadata. |
| GET | `/api/v1/mail/messages/{id}/parts/{part_id}` | Stream a body part; HTML is sanitized. |
| GET | `/api/v1/mail/messages/{id}/source` | Download the original `.eml`. |
| GET | `/api/v1/mail/messages/{id}/attachments` | List `MailAttachment` metadata. |

Attachments reuse the existing file service: each `MailAttachment` stores a `file_id`, so preview/download uses `/api/v1/files/{file_id}/preview` and `/api/v1/files/{file_id}/content`.

The module summary for `mail` will query `mail_messages` directly instead of counting files under `/Workspace/Mail`.

### Frontend

The mail client is registered as a standard module:

1. Add `mail` to `APPROVED_MODULE_ICONS` / `iconRegistry.ts` and `ModuleIcon.svelte`.
2. Add a `mail` entry to `PREDEFINED_MODULES` in `registry.ts` with renderer `mail-list`.
3. Map `mail-list` to `MailModuleView` in `ModulePageRenderer.svelte`.
4. Update `modulePages.ts` so mail message links route to `/modules/mail/messages/{messageId}`.
5. Create `MailModuleView.svelte` (list) and `+page.svelte` under `modules/mail/messages/[messageId]` (detail).
6. Add `frontend/src/lib/api/mail.ts` wrappers.

The detail view fetches message metadata, parts, and attachments, picks the best body part (`text/html` preferred, `text/plain` fallback), and renders it. HTML is rendered with `{@html sanitizedHtml}` where `sanitizedHtml` comes from the backend sanitizer and is run through the existing client-side `sanitizeHtml` helper as defense-in-depth.

## Data Flow

1. User enables the `mail` module and clicks **Mail** in the sidebar.
2. `/modules/mail` renders `MailModuleView`, which calls `listMailMessages`.
3. User clicks a message row; navigation goes to `/modules/mail/messages/{id}`.
4. Detail page loads message metadata, parts list, and attachment list in parallel.
5. Detail page picks a body part and calls the content endpoint.
6. Content is sanitized and rendered; attachments are shown as cards that open `FilePreviewModal`.

## Error Handling

- `require_mail_enabled` returns 403 if the module is disabled.
- `MailService::get_message` returns 404 if the message does not exist or is not owned by the caller.
- Missing object-store blobs return 500 with a generic message.
- Sanitization failures return 500; the frontend falls back to the plain-text part if available.
- All frontend errors use `ErrorState` and `toastStore`.

## Safety and Security

This phase touches two safety-boundary areas: **mail read visibility** and **HTML rendering of untrusted email content**.

- Every read path validates tenant and owner. Cross-tenant access is rejected with 403.
- HTML parts are sanitized server-side with `ammonia` (to be added to `backend/server/Cargo.toml`). The allow-list removes scripts, event handlers, forms, `<style>` tags, and `javascript:`/`data:` URLs.
- Client-side `sanitizeHtml` is kept as defense-in-depth.
- Attachments are never served from the raw blob URL; they flow through the existing authenticated file endpoints.
- A `MailMessageViewed` audit event is emitted on body/source reads for traceability.
- No sharing/visibility changes are made; mail remains `Private`.

## Testing

### Backend
- Handler tests for `GET /parts`, `GET /parts/{id}`, `GET /source`, `GET /attachments`.
- 404 for unknown messages/parts.
- 403 for cross-tenant and module-disabled cases.
- Assert that raw HTML containing `<script>` is returned without the script after sanitization.
- Assert `MailMessageViewed` event is appended.
- OpenAPI freshness test passes after regenerating the contract.

### Frontend
- Registry tests: `mail` appears in predefined modules and the renderer map.
- `MailModuleView` renders a list of messages and navigates on click.
- Mail API client maps backend types correctly.
- Detail page renders sanitized HTML and falls back to plain text.

### Integration / CI
- `cargo fmt --check`
- `SQLX_OFFLINE=true cargo clippy --all-features -- -D warnings`
- `SQLX_OFFLINE=true cargo test --all-features --lib`
- `SQLX_OFFLINE=true cargo test --workspace --lib`
- `cd frontend && npm run check && npm run lint && npm run test`
- `cargo sqlx prepare --workspace --check`

## Dependencies

- Add `ammonia` to `backend/server/Cargo.toml` for HTML sanitization.
- Frontend reuses existing `isomorphic-dompurify` via `sanitizeHtml`.

## Files Expected to Change

### Backend
- `backend/server/src/handlers/mail.rs`
- `backend/server/src/services/mail_service.rs`
- `backend/server/src/routes.rs`
- `backend/server/src/openapi.rs`
- `backend/server/src/services/module_service.rs`
- `backend/server/Cargo.toml`
- `backend/crates/core/src/events/types.rs`
- `backend/crates/storage/src/metadata.rs`
- `docs/contracts/rustshare-api-openapi.json`
- `backend/.sqlx/` (after `cargo sqlx prepare`)

### Frontend
- `frontend/src/lib/modules/iconRegistry.ts`
- `frontend/src/lib/components/dashboard/ModuleIcon.svelte`
- `frontend/src/lib/modules/registry.ts`
- `frontend/src/routes/(app)/modules/[key]/ModulePageRenderer.svelte`
- `frontend/src/lib/modules/modulePages.ts`
- `frontend/src/lib/api/mail.ts`
- `frontend/src/lib/api/types.ts` (new mail response types)
- `frontend/src/lib/components/modules/MailModuleView.svelte`
- `frontend/src/routes/(app)/modules/mail/messages/[messageId]/+page.svelte`
- New tests in `frontend/tests/` following existing module view patterns.
