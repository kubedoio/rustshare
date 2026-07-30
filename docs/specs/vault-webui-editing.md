# Specification: Safe WebUI Editing for Obsidian-Compatible Vaults

Issue: #121  
Status: Draft / MVP implementation spec  
Owner: RustShare Core Team  
Suggested file path: `docs/specs/vault-webui-editing.md`

---

## 1. Purpose

This specification defines the first safe implementation of WebUI editing for Obsidian-compatible vaults in RustShare.

RustShare already has a Vault Sync API and a frontend vault view. The current WebUI can show vault metadata and manifest entries, but it does not yet provide a safe way to open, edit, and save Markdown files from the browser.

The goal of this spec is to define a bounded MVP that makes Markdown vault files editable through the RustShare WebUI without corrupting sync state, bypassing permissions, or silently overwriting newer content.

---

## 2. Problem

Vaults are currently useful for sync and display, but the WebUI is read-only.

Users need to:

- open Markdown files from a vault
- make small edits from the browser
- save changes safely
- avoid conflicts with sync clients
- understand whether a vault is read-only or editable
- avoid accidental writes to imported or externally managed vaults

Without an explicit write policy and revision-aware save behavior, WebUI editing could create unsafe outcomes:

- a synced vault could be modified unexpectedly
- stale browser content could overwrite newer local edits
- imported vaults could become writable by accident
- permissions could become unclear
- future indexing/RAG provenance could become unreliable

---

## 3. Goal

Implement safe MVP WebUI editing for eligible Markdown files inside Obsidian-compatible vaults.

The MVP must support:

- selecting a Markdown file from a vault manifest
- loading its content into the WebUI
- editing content only when the vault write policy allows WebUI edits
- saving content through revision-aware backend behavior
- rejecting stale saves instead of silently overwriting newer content
- showing clear frontend states for:
  - read-only vault
  - editable file
  - unsupported file type
  - unsaved changes
  - save success
  - save conflict
  - write denied
- preserving existing vault sync behavior

---

## 4. Non-Goals

This MVP must not implement:

- full Obsidian plugin compatibility
- full Obsidian-style editor behavior
- live collaborative editing
- offline editing
- automatic merge/conflict resolution
- binary file editing
- rich WYSIWYG editing
- full Markdown preview parity
- sync-client rewrite
- automatic write access for all vaults
- automatic AI/RAG indexing policy changes
- unrelated changes to Notes, Files, Mail, Kanban, or RustChat integrations

A simple text/textarea-based Markdown editor is acceptable for the MVP.

---

## 5. Current Implementation Assumptions

Before implementation, verify the actual repository state.

Expected existing pieces:

- `backend/server/src/handlers/vault_sync.rs`
- Vault Sync API with:
  - create/list/get vault
  - manifest retrieval
  - file upload
  - file download
  - file delete
  - file rename
  - device registration/revocation
  - optimistic revision locking
- frontend vault route similar to:
  - `frontend/src/routes/(app)/vaults/[vaultId]/+page.svelte`
- frontend currently displays:
  - vault metadata
  - manifest file list
- frontend currently lacks:
  - file content view
  - Markdown editor
  - save button
  - write policy awareness
  - conflict UI

The implementation must inspect the repository and adapt names/routes/types to the real code.

---

## 6. Product Rules

### 6.1 Conservative default

Vaults must not become writable by default.

Default write behavior:

```text
read_only
```

A vault must explicitly allow WebUI editing before the save flow is enabled.

### 6.2 Explicit write policy

The system must distinguish between at least these conceptual modes:

```text
read_only
web_editing_enabled
sync_client_only
```

Meaning:

| Policy | Meaning |
|---|---|
| `read_only` | WebUI may show files but cannot save changes. |
| `web_editing_enabled` | WebUI may edit eligible files with revision checks. |
| `sync_client_only` | Sync clients may update vault files, but WebUI editing is disabled. |

If the existing model already has equivalent fields, reuse them.

If no policy exists, add the smallest safe field/migration required.

### 6.3 File eligibility

Editable in MVP:

```text
.md
.markdown
```

Optional if already safe and simple:

```text
.txt
```

Read-only in MVP:

```text
images
PDFs
office documents
archives
binary files
unknown extensions
large files over the configured limit
```

### 6.4 Revision-aware save

A save must include the revision/version that the frontend originally loaded.

Required behavior:

```text
Client loads file with revision R1.
Another actor updates the file to revision R2.
Client tries to save using expected revision R1.
Backend rejects the save.
Frontend keeps the user's unsaved text visible and shows a conflict message.
```

Silent overwrite is not allowed.

### 6.5 Path safety

Backend must reject:

```text
../ traversal
absolute paths
empty paths
paths outside vault root
paths with unsafe normalization
```

### 6.6 No content leakage in logs

Backend and frontend logs must not print full file content.

---

## 7. Write Policy Model

### 7.1 Minimal model

A minimal implementation may add a field to the vault metadata such as:

```text
write_policy
```

Allowed values:

```text
read_only
web_editing_enabled
sync_client_only
```

If the repository already has a different name, use the existing convention.

### 7.2 Default

New or existing vaults should default to:

```text
read_only
```

Unless the current application already has a safe explicit user action for enabling WebUI editing.

### 7.3 Policy transitions

MVP may avoid building a full admin UI for changing write policy.

Acceptable MVP options:

1. expose policy from backend and keep all existing vaults read-only
2. add a simple backend/admin mechanism if the repo already has admin settings
3. document how policy is configured for now

Do not create a complex management UI unless necessary.

### 7.4 Authorization

Even if the write policy allows editing, the user must still have permission to modify the vault.

Effective write permission requires both:

```text
user has write access
AND
vault write_policy == web_editing_enabled
```

---

## 8. Backend Contract

Prefer existing Vault Sync endpoints if they already provide the necessary behavior.

The backend must support these operations conceptually:

### 8.1 Get vault file content

```http
GET /api/vault-sync/v1/vaults/{vault_id}/files/{path}
```

Response shape, adapted to existing API conventions:

```json
{
  "vaultId": "vault_123",
  "path": "folder/note.md",
  "content": "# Note\n\nBody",
  "contentType": "text/markdown",
  "revision": "rev_123",
  "updatedAt": "2026-07-07T12:00:00Z",
  "editable": true,
  "writePolicy": "web_editing_enabled"
}
```

### 8.2 Save vault file content

```http
PUT /api/vault-sync/v1/vaults/{vault_id}/files/{path}
```

Request shape:

```json
{
  "content": "# Note\n\nUpdated body",
  "expectedRevision": "rev_123"
}
```

Response shape:

```json
{
  "vaultId": "vault_123",
  "path": "folder/note.md",
  "revision": "rev_124",
  "updatedAt": "2026-07-07T12:05:00Z"
}
```

### 8.3 Conflict response

When the revision does not match:

```http
409 Conflict
```

Suggested response:

```json
{
  "error": "conflict",
  "message": "The file changed since you opened it. Reload before saving.",
  "currentRevision": "rev_124"
}
```

### 8.4 Write-denied response

When policy or permission denies editing:

```http
403 Forbidden
```

Suggested response:

```json
{
  "error": "write_denied",
  "message": "This vault is read-only from the WebUI."
}
```

### 8.5 Unsupported file response

When file is not eligible for editing:

```http
415 Unsupported Media Type
```

or existing project error format.

Suggested response:

```json
{
  "error": "unsupported_file_type",
  "message": "Only Markdown files are editable in the WebUI MVP."
}
```

---

## 9. Backend Implementation Requirements

Backend implementation must:

1. Authenticate the request.
2. Authorize vault access.
3. Check write permission for save requests.
4. Check vault write policy.
5. Normalize and validate path.
6. Restrict editable file types.
7. Enforce file size limit before loading into memory.
8. Load content with revision metadata.
9. Save only when `expectedRevision` matches current revision.
10. Return new revision on success.
11. Reject stale revision with conflict response.
12. Preserve existing vault manifest behavior.
13. Preserve existing sync-client behavior.
14. Avoid logging file content.
15. Add tests.

---

## 10. Frontend Behavior

The frontend must provide a simple but safe editing workflow.

### 10.1 File list

Existing file list remains visible.

Each file should clearly indicate whether it is:

```text
editable Markdown
read-only Markdown
unsupported read-only file
folder
```

### 10.2 Opening a file

When a user selects a file:

- if eligible Markdown/text:
  - load content and revision
- if unsupported:
  - show read-only unsupported message
- if too large:
  - show safe size-limit message
- if permission denied:
  - show clear permission message

### 10.3 Editing

If editable:

- show a Markdown text editor or textarea
- track dirty state
- show Save button
- optionally show Reload button
- warn before losing unsaved changes:
  - browser refresh/tab close triggers a `beforeunload` prompt while the editor is dirty
  - in-app navigation away from the page asks for confirmation ("You have unsaved changes. Leave without saving?")
  - switching to a different file in the manifest asks for confirmation before discarding unsaved edits

If read-only:

- show content in read-only mode if safe
- do not show Save button
- show reason:
  - vault read-only
  - sync-client-only
  - no write permission
  - unsupported file type

### 10.4 Save button

Save button disabled when:

```text
file is read-only
no content is loaded
no revision is loaded
there are no changes
save is in progress
```

### 10.5 Save success

On save success:

- update local revision to returned revision
- clear dirty state
- show success message
- refresh manifest metadata if necessary

### 10.6 Conflict

On conflict:

- do not discard local unsaved content; the editor text remains untouched until the user explicitly reloads
- show a conflict panel:
  - "A newer server revision exists (rev N)."
- offer safe actions:
  - copy my changes (writes the local editor text to the clipboard)
  - download my version (downloads the local editor text under the file's name)
  - reload server version (discards local edits after a confirmation prompt and refetches)
- do not auto-merge in MVP
- if the server revision turns out to hold content identical to the editor content (SHA-256 match), silently adopt the server revision and treat the state as saved instead of showing a conflict

### 10.7 Failed save

On network/backend failure:

- keep unsaved content visible
- show error message
- allow retry

---

## 11. Permission and Security Requirements

The implementation must ensure:

- authenticated access only
- no edit without permission
- no edit when policy is read-only
- no edit for unsupported files
- no path traversal
- no silent overwrite
- no full file body in logs
- no bypass of vault/device access model
- no accidental exposure of private vault content
- no unexpected AI/RAG indexing policy changes

If audit infrastructure exists, emit audit events for:

```text
vault_file_webui_opened
vault_file_webui_saved
vault_file_webui_save_conflict
vault_file_webui_write_denied
```

If audit infrastructure is not ready, document this as a follow-up.

---

## 12. Error Handling Matrix

| Case | Backend behavior | Frontend behavior |
|---|---|---|
| Vault not found | 404 | Show "Vault not found" |
| File not found | 404 | Show "File not found" |
| No read permission | 403 | Show permission message |
| No write permission | 403 | Disable save / show write denied |
| Read-only policy | 403 | Show vault is read-only |
| Unsupported file type | 415 or project equivalent | Show read-only unsupported message |
| File too large | 413 or project equivalent | Show size-limit message |
| Stale revision | 409 with structured body (`current_rev`, `server_sha256`, `resolution`) | Keep unsaved content, show conflict panel with copy/download/reload actions; silently adopt the revision when server content is identical |
| Network failure | error | Keep unsaved content, allow retry |
| Save success | 200/204 | Clear dirty state, update revision |

---

## 13. Test Plan

### 13.1 Backend tests

Required backend tests:

```text
load Markdown file returns content and revision
save Markdown succeeds when write policy allows it
save is denied when vault is read-only
save is denied when user lacks write permission
stale revision is rejected with conflict
path traversal is rejected
unsupported file type save is rejected
large file handling follows configured limit
existing sync-client upload/download behavior still works
```

### 13.2 Frontend tests or manual checks

If frontend automated tests exist, add tests for:

```text
Markdown file opens in editor
read-only vault does not show enabled save flow
dirty state appears after editing
save success clears dirty state
conflict error is displayed
unsupported file is not editable
```

If frontend test infrastructure is not available, document manual validation:

```text
1. Open writable vault.
2. Open Markdown file.
3. Edit and save.
4. Confirm content persists after reload.
5. Open read-only vault.
6. Confirm save is not available.
7. Simulate stale revision.
8. Confirm conflict message and unsaved text preservation.
9. Open unsupported file.
10. Confirm read-only unsupported state.
```

---

## 14. Implementation Phases

### Phase 0 — Spec and inspection

- Add this spec.
- Inspect current Vault Sync API.
- Identify whether existing endpoints can support WebUI editing.

### Phase 1 — Backend safety foundation

- Expose/read write policy.
- Add or reuse file-content load endpoint.
- Add or reuse revision-aware save endpoint.
- Enforce file eligibility and path safety.
- Add backend tests.

### Phase 2 — Frontend MVP editor

- Open Markdown files from vault manifest.
- Display content.
- Enable editing only when allowed.
- Add Save button and dirty state.
- Handle success, conflict, and denied states.

### Phase 3 — Validation and polish

- Run backend/frontend checks.
- Add manual validation notes.
- Document follow-up work.

---

## 15. Acceptance Criteria

This issue is acceptable when:

- [ ] A safe WebUI vault editing spec exists.
- [ ] Markdown files can be opened from the vault WebUI.
- [ ] Markdown files can be edited only when vault write policy allows it.
- [ ] Read-only vaults remain read-only.
- [ ] Sync-client-only vaults do not expose WebUI save.
- [ ] Non-Markdown/binary files are not editable.
- [ ] Save requires expected revision.
- [ ] Stale saves are rejected.
- [ ] Conflict does not discard unsaved browser content.
- [ ] Path traversal is rejected.
- [ ] Existing vault sync behavior is not broken.
- [ ] Backend tests cover write policy and stale revision behavior.
- [ ] Frontend shows clear save/conflict/write-denied states.
- [ ] No unrelated product modules are changed.

---

## 16. Follow-Up Work

Possible follow-up issues:

```text
vaults: add admin UI for vault write policy
vaults: add conflict diff view for WebUI editing
vaults: add richer Markdown editor
vaults: add audit events for WebUI vault edits
vaults: add frontend automated tests for vault editor
vaults: add Markdown preview mode
vaults: add configurable editable file extensions
vaults: add large-file safe preview mode
```

---

## 17. Review Questions

Before merging implementation, answer:

1. Did any existing vault become writable by default?
2. Can a user write outside the vault root?
3. Can stale WebUI content overwrite newer sync-client content?
4. Are binary files protected from editing?
5. Does frontend preserve unsaved content after save failure?
6. Does this break existing vault sync clients?
7. Are write-denied and conflict states understandable?
8. Are logs free of full file content and secrets?
9. Is AI/RAG behavior unchanged?
10. Is the PR focused only on #121?

---

## 18. PR Guidance

Suggested branch:

```text
feat/vault-webui-safe-editing
```

Suggested PR title:

```text
feat(vaults): add safe WebUI editing for Markdown files
```

Suggested PR reference:

```md
Refs #121
```

Use `Fixes #121` only if all acceptance criteria are satisfied.

Suggested PR body:

```md
Refs #121

## Summary

- Added safe WebUI editing for eligible Markdown vault files.
- Added/used explicit vault write policy.
- Added revision-aware save behavior.
- Added conflict and write-denied handling.
- Added tests and documentation.

## Scope

This is the MVP for Markdown WebUI editing.
It does not implement collaborative editing, binary editing, rich WYSIWYG editing, full Obsidian plugin behavior, or automatic AI indexing.

## Validation

- [ ] cargo fmt --check
- [ ] cargo clippy --workspace --all-features --all-targets -- --deny warnings
- [ ] cargo test --workspace
- [ ] frontend checks/build
- [ ] manual WebUI validation

## Safety Notes

- Read-only vaults remain read-only.
- WebUI writes require explicit write policy.
- Saves use expected revision to prevent silent overwrite.
- Non-Markdown/binary files remain read-only.
- Existing sync-client behavior is preserved.
```
