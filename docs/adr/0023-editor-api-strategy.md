# ADR-0023: Editor API Strategy — Canonical File APIs for MVP

## Status

Accepted

## Context

The RustShare editor contract (`docs/contracts/editor-api-contract.md`) originally proposed dedicated `/api/editor/documents` routes for document get, save, attachment upload, attachment list, and attachment delete. At the same time, RustShare already has mature file, folder, and note APIs:

- `GET /api/v1/files/{id}` — file metadata
- `PUT /api/v1/files/{id}` — file update with `If-Match` optimistic locking
- `POST /api/v1/files/{id}/edit` — file content edit with base64 payload
- `POST /api/v1/files/upload` — file upload
- `GET /api/v1/folders/{id}/contents` — folder contents listing
- `DELETE /api/v1/files/{id}` — file deletion
- `GET /api/v1/notes/{id}` — note metadata and content
- `PUT /api/v1/notes/{id}` — note update

The frontend currently uses these existing routes plus adapter helpers; no `/api/editor/documents` routes are implemented. The consistency gap analysis (HP-01) flagged this ambiguity as blocking safe attachment and rich-document work.

We must decide:

1. Are existing file APIs sufficient for the editor MVP?
2. If not, what dedicated APIs are needed and when?
3. How do attachment safety, revision/conflict behavior, and permissions map?

## Decision

**Existing RustShare file APIs are canonical for the editor MVP.**

Dedicated `/api/editor/documents` routes are deferred until cross-module document composition or automatic attachment handling justifies the extra abstraction layer.

### Mapping

| Editor concept | Backend implementation |
|---|---|
| Document get | `GET /api/v1/files/{id}` or `GET /api/v1/notes/{id}` |
| Content save | `PUT /api/v1/files/{id}` (`If-Match`) or `POST /api/v1/files/{id}/edit`; `PUT /api/v1/notes/{id}` for notes |
| Revision / baseRevision | File `current_version` + `If-Match` header |
| Revision conflict | `FileError::VersionConflict` → HTTP 409 |
| Attachment upload | `POST /api/v1/files/upload` with `parent_folder_id` |
| Attachment list | `GET /api/v1/folders/{id}/contents` |
| Attachment delete | `DELETE /api/v1/files/{id}` |
| Permission check | Existing `PermissionResolver` on every file/folder route |

## Rationale

1. **Architecture alignment.** ADR-0020 (Markdown canonical) and ADR-0021 (file-backed attachments) establish that documents are files. Adding a separate editor API would create indirection without adding capability.
2. **Existing coverage.** The backend already implements optimistic locking (`If-Match`), version history, permission resolution, tenant isolation, and hidden-file exclusion for file and folder listings.
3. **Frontend reality.** The frontend already calls file/note APIs for editor operations; introducing new routes would require frontend migration with no user-facing benefit.
4. **Testability.** File API contract tests (versioning, tenant isolation, permission denial) already cover the security-critical paths. We need mapping tests, not new backend code.
5. **Deferral is safe.** If a future module needs automatic attachment sanitization, batch operations, or document-level metadata merging, a thin editor service can be added then without breaking existing behavior.

## Consequences

### Positive

- No new backend routes to implement, test, and maintain for MVP.
- Editor security inherits file API guarantees (tenant isolation, permission checks, path safety).
- Consistent with file-centric product identity.
- Frontend can ship immediately using documented mapping.

### Negative / Risks

- Frontend must perform attachment filename sanitization before upload.
- Attachment listing relies on the backend folder contents query excluding hidden files; this must be verified per listing endpoint.
- Document "save" semantics differ slightly between raw files (`If-Match`) and notes (note update body). The frontend adapter must handle both.
- If multiple modules later need editor-specific behavior, retrofitting a unified editor API may require migration.

## Scope Boundaries

### In scope (MVP)

- Document get/save via file and note APIs.
- Attachment upload/list/delete via file and folder APIs.
- Optimistic locking via file versioning (`If-Match`).
- Hidden file exclusion in listings.
- Explicit mapping documentation and contract tests.

### Out of scope (deferred)

- `POST /api/editor/documents` and related routes.
- Backend automatic attachment filename sanitization middleware.
- Document-level revision counter separate from file versions.
- Batch attachment upload/delete.
- Attachment metadata reordering or editing API.
- Standalone editor JSON cache read/write endpoint.

## Acceptance Criteria

- [x] One explicit editor API strategy is documented in `docs/contracts/editor-api-contract.md`.
- [x] Strategy explicitly states what is deferred.
- [x] Attachment and revision/conflict behavior expectations are mapped to existing contracts.
- [x] Backend contract tests verify the mapping (`editor_file_api_mapping_contract.rs`).
- [x] Stub tests exist for deferred dedicated editor routes.

## Related

- ADR-0019: Shared Rich Markdown Editor
- ADR-0020: Canonical Markdown with Optional Editor JSON Cache
- ADR-0021: File-Backed Attachments and Portable Asset References
- Contract: Editor API (`docs/contracts/editor-api-contract.md`)
- Contract: Attachment (`docs/contracts/attachment-contract.md`)
- Spec: Attachments and Assets (`docs/specs/attachments-and-assets.md`)
