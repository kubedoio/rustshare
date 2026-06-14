# Contract: Editor API

## Strategy

**Existing RustShare file APIs are canonical.** The editor does not use dedicated `/api/editor/documents` routes for MVP.

The frontend consumes existing file, note, and folder endpoints through adapter helpers. A future dedicated editor API may be introduced only when module-specific composition (automatic attachment handling, document-level metadata merging, or cross-module templates) justifies the extra layer.

## API Mapping

| Editor operation | Canonical backend route | Notes |
|---|---|---|
| Get document | `GET /api/v1/files/{id}` or `GET /api/v1/notes/{id}` | For folder-backed documents the note route returns composed metadata; for raw Markdown files the file route returns the file record. Download content via `GET /api/v1/files/{id}/content` or `GET /api/v1/files/{id}/preview`. |
| Save content | `PUT /api/v1/files/{id}` with `If-Match` header, or `POST /api/v1/files/{id}/edit` | Optimistic locking uses the file version number (`current_version`). `If-Match: 3` maps to `baseRevision: 3`. Conflict returns `VersionConflict` (HTTP 409). For notes use `PUT /api/v1/notes/{id}`. |
| Upload attachment | `POST /api/v1/files/upload` with `parent_folder_id` set to the document's `attachments/` folder | The frontend must sanitize filenames before upload. Backend rejects path traversal, hidden names, and invalid characters. |
| List attachments | `GET /api/v1/folders/{attachments_folder_id}/contents` | Returns files inside the attachments folder. Hidden files (`.rustshare.json`, `.editor.json`, names starting with `.rustshare`) must be excluded by the backend listing logic. |
| Delete attachment | `DELETE /api/v1/files/{attachment_id}` | Standard file deletion. Trashes the file; use `DELETE /api/v1/files/{id}/permanent` for permanent removal if policy allows. |

## Revision / Conflict Behavior

- The file versioning system (`current_version`, `If-Match`) satisfies the editor's `baseRevision` requirement.
- A save with a stale `If-Match` value returns HTTP 409 (`VersionConflict`).
- The frontend adapter must surface this as `revision_conflict`.
- There is no separate document-level revision counter beyond file version numbers.

## Permission Model

- Read permission on the document file or note grants read access to attachments.
- Write permission on the document file or note grants upload and save access.
- The backend `PermissionResolver` enforces these checks on every file/folder route.

## What Is Not Implemented Yet

The following are explicitly out of scope until a future implementation pass:

1. **Dedicated `/api/editor/documents` routes** – deferred until cross-module document composition is required.
2. **Automatic attachment filename sanitization service** – frontend sanitizes today; a backend middleware may be added later.
3. **Document-level revision other than file versioning** – no separate editor revision counter for MVP.
4. **Batch attachment operations** – upload/delete one at a time via file APIs.
5. **Attachment reordering or metadata update API** – not supported; attachments are files.
6. **Editor JSON cache read/write API** – `index.editor.json` is written directly by module services if needed; no standalone endpoint.

## Errors

The editor adapter must translate backend errors as follows:

| Backend error | Editor contract error |
|---|---|
| `FileError::NotFound` / HTTP 404 | `not_found` |
| `FileError::PermissionDenied` / HTTP 403 | `forbidden` |
| `FileError::ValidationError` / HTTP 400 | `validation_error` |
| `FileError::PathTraversal` / HTTP 400 | `path_traversal` |
| `FileError::PayloadTooLarge` / HTTP 413 | `upload_too_large` |
| `FileError::UnsupportedMediaType` / HTTP 415 | `unsupported_file_type` |
| `FileError::VersionConflict` / HTTP 409 | `revision_conflict` |
| Generic internal error / HTTP 500 | `save_failed` |

## Acceptance Criteria

- [x] Existing file/note/folder routes can satisfy all MVP editor operations.
- [x] Mapping from editor concepts to file API concepts is documented above.
- [x] Revision conflict behavior is testable via file versioning contract tests.
- [x] Attachment path safety is testable via folder/file contract tests.
- [x] Future dedicated editor API contract exists as stub/pending tests (see `backend/tests/contracts/editor_file_api_mapping_contract.rs`).
