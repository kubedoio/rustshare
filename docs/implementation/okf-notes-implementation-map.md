# OKF-native Notes Module — Implementation Map

> Audit for GitHub issue #118. This document is read-only planning material; no source code or behavior has been changed.

## 1. ADR / spec summary

> **Note:** The two files explicitly requested in the issue — `docs/adr/0019-notes-as-okf-documents.md` and `docs/adr/0020-okf-notes-reconciliation-and-rag-safety.md` — do **not** currently exist in the repository. The closest existing ADRs are `0019-shared-rich-markdown-editor.md` and `0020-canonical-markdown-with-editor-cache.md`; their summaries are included here. If the OKF-specific ADRs are created later, this map should be updated to reference them.

| Document | Status | Key points relevant to Notes |
| --- | --- | --- |
| `docs/adr/0016-file-backed-template-modules.md` | Accepted | Defines Notes as a file-backed module. Predefined module key `notes`, renderer `notes`, root path `/Notes` (legacy) or `/Workspace/Notes` (canonical). Artifacts are folders/files + metadata sidecars. |
| `docs/adr/0017-template-registry-and-admin-governance.md` | Accepted | Module and Template registries persisted as system metadata. Default modules must be ensured on startup without overwriting admin changes. Admin routes `/admin/modules` and `/admin/templates`. |
| `docs/adr/0018-webui-module-navigation-and-dashboard-integration.md` | Accepted | Sidebar/dashboard/module-page are data-driven from module registry. Notes sidebar order 30, dashboard order 10, card title "Notes", primary action `create-from-template` using `template_default_note`. |
| `docs/adr/0019-shared-rich-markdown-editor.md` | Proposed / candidate | Shared `MarkdownDocumentPage`, `RichMarkdownEditor`, `RichMarkdownViewer`, attachment panel, autosave, export. Markdown remains canonical; attachments are real files. |
| `docs/adr/0020-canonical-markdown-with-editor-cache.md` | Proposed / candidate | `index.md`/`.md` is source of truth; optional `index.editor.json` is cache only. Required Markdown compatibility list provided. |
| `docs/adr/0021-file-backed-attachments-and-portability.md` | Accepted | Attachments live in `attachments/` next to the document; inline images use relative Markdown paths for portability. |
| `docs/adr/0029-filename-heading-separation.md` | Accepted | **File identity must be separate from H1.** Changing the first H1 must not rename the file/folder; renaming must not rewrite the H1. |
| `docs/specs/admin-modules-and-templates.md` | Draft / implementation ready | Required admin routes, module/template table columns, actions, validation rules, empty states, and approved icon registry. |

### H1 / filename separation status
`NoteService::save_note` no longer renames the note bundle folder when the first H1 changes. File identity and the first H1 are independent, matching `docs/adr/0029-filename-heading-separation.md`.

---

## 2. Current note creation flow

### Backend
- Entry: `POST /api/v1/notes` → `backend/server/src/handlers/notes.rs::create_note`
- Service: `backend/server/src/services/note_service.rs::NoteService::create_note`
  1. Default title `"Untitled Note"` if none provided.
  2. Ensures parent folder `/Workspace/Notes` via `ensure_target_folder` / `ensure_workspace_folder`.
  3. Generates a collision-safe **folder** name (`unique_folder_name`) from the title.
  4. Creates the bundle folder with subfolders: `attachments`, `drawings`, `exports`, `_rustshare`.
  5. Uploads `note.md` inside the bundle.
  6. Uploads `_rustshare/manifest.json` (best-effort).
  7. Creates legacy-compatible sidecar `{file_name}.rustshare.json` via `save_metadata`.
- The API returns the `note.md` file id; the display title is stored in metadata/manifest, not the filename.

### Frontend
- `frontend/src/lib/components/modules/NotesModuleView.svelte::handleNewNote`
  1. Picks a unique local title (`Untitled Note`, `Untitled Note 2`, …) by scanning the current list.
  2. Calls `createNote({ title, content: "# ${title}\n\n" })` from `frontend/src/lib/api/notes.ts`.
  3. On success, navigates to `/modules/notes/${result.id}`.
- The legacy redirect `/notes/{id}` → `/modules/notes/{id}` lives in `frontend/src/routes/(app)/notes/[id]/+page.svelte` and `+page.ts`.

---

## 3. Current note save flow

### Frontend
- `frontend/src/routes/(app)/modules/[key]/[id]/+page.svelte` handles `on:save` from `MarkdownDocumentPage`.
- For `key === 'notes'` it calls `notesApi.update(id, { content, attachments })` (`PUT /api/v1/notes/{id}`).
- Folder-backed notes convert attachment API URLs back to relative paths before saving.
- The editor is initialized in `edit` mode for notes (`mode = key === 'notes' ? 'edit' : 'read'`).

### Backend
- `backend/server/src/handlers/notes.rs::save_note` → `NoteService::save_note`.
- Steps:
  1. `file_service.edit_file(file_id, …, "overwrite")` updates `note.md`.
  2. Loads existing sidecar (or fallback).
  3. Applies optional `color` and `attachments`.
  4. Regenerates `excerpt` and `updated_at`.
  5. Preserves the existing title and bundle name; **does not** rename the folder or derive the title from the first H1.
  6. `save_metadata` writes:
     - `{file.name}.rustshare.json` sidecar.
     - `_rustshare/manifest.json` if folder-backed.

---

## 4. Current rename behavior

### Explicit rename (`POST /api/v1/notes/{id}/rename`)
- `NoteService::rename_note`:
  - Loads sidecar by old name first.
  - If folder-backed: renames the **parent folder** to a collision-safe version of the new title.
  - If legacy single-file: renames the `.md` file.
  - Updates sidecar title and rewrites both sidecar and manifest.

### H1-driven rename (implicit, on save)
- `NoteService::save_note` does **not** derive the title from the first H1.
- Changing the first H1 only changes document content; the file/folder name and metadata title remain unchanged.

### Frontend rename UI
- `NotesModuleView.svelte` (list) and `modules/[key]/[id]/+page.svelte` (detail) both call `renameNote(id, { title })`.
- The prompt defaults to `(metadata.title || name).replace(/\.md$/i, '')`.

---

## 5. Current manifest behavior

### Location
Folder-backed notes write `_rustshare/manifest.json` inside the note bundle.

### Content (written by `NoteService`)
```json
{
  "type": "rustshare.note",
  "version": 1,
  "id": "<note.md file id>",
  "title": "<display title>",
  "main": "note.md",
  "created_at": "<rfc3339>",
  "updated_at": "<rfc3339>",
  "attachments": [
    { "file_id": "…", "name": "…", "mime_type": "…", "size": 0, "created_at": "…" }
  ],
  "drawings": [],
  "exports": []
}
```

### How it is written
- On create: uploaded directly after `note.md`.
- On save: `save_metadata` finds the `_rustshare` folder and either `edit_file` (if manifest exists) or `upload_file` (if missing).
- On load: `load_metadata` prefers manifest `title` for folder-backed notes, then falls back to `{file}.rustshare.json`, then legacy `meta/notes/{id}.json`.

### Sidecar
- Visible sidecar: `{file.name}.rustshare.json` (e.g. `note.md.rustshare.json`) stored next to `note.md`.
- Legacy hidden sidecar: `meta/notes/{file_id}.json` in object storage.

---

## 6. Current frontend title source

| Surface | Title source |
| --- | --- |
| Note detail page header | `item.metadata.title \|\| item.name` (`modules/[key]/[id]/+page.svelte`) |
| Note list item | `(note.metadata.title \|\| note.name).replace(/\.md$/i, '')` (`NotesModuleView.svelte`) |
| Dashboard "Latest Notes" widget | `note.name` from module summary (`LatestNotesWidget.svelte`) |
| Rename prompt | `(metadata.title \|\| name).replace(/\.md$/i, '')` |

The backend derives `NoteSummary.name` for folder-backed notes from the **parent folder name**; `metadata.title` comes from the sidecar/manifest. Therefore the visible title is currently a mixture of folder name and metadata title, not the H1 directly.

---

## 7. Current admin module / template registry shape

### Database tables

**`modules`** (`backend/migrations/20260429220001_create_modules_table.sql` + `20260430000001_add_module_ui_config.sql`)
```text
id, module_key (unique), display_name, description, enabled, root_path,
renderer, default_template, icon, schema_version, permissions (jsonb),
ai_indexing (jsonb), audit (jsonb), ui_config (jsonb), created_at, updated_at, tenant_id
```

**`templates`** (`backend/migrations/20260429220002_create_templates_table.sql` + `20260501000001_add_system_template_to_templates.sql` + `20260511000001_add_module_config_to_templates.sql`)
```text
id, template_key (unique), name, module_key -> modules(module_key), version,
description, ui_config (jsonb), folder_structure (jsonb), default_files (jsonb),
metadata_schema (jsonb), renderer, visibility_policy, ai_indexing_policy (jsonb),
audit_logging_policy (jsonb), module_config (jsonb), created_by -> users(id),
created_at, updated_at, enabled, system_template, tenant_id
```

### How defaults are loaded
- `backend/server/src/bootstrap.rs::init_app` calls:
  1. `module_service.ensure_default_modules(default_tenant_id)`
  2. `template_service.ensure_default_templates(default_tenant_id)`
- `module_service.rs` defines `default_modules()` with the canonical `/Workspace/Notes` root, renderer `notes`, default template `template_default_note`, icon `sticky-note`, enabled `true`.
- `template_service.rs` defines `template_default_note` with folder structure `[attachments, drawings, exports, _rustshare]` and default files `note.md` + `_rustshare/manifest.json`.
- Both are idempotent: existing rows are not overwritten except for system-template schema updates.

### Frontend registry
- `frontend/src/lib/modules/registry.ts` keeps a static `PREDEFINED_MODULES` list and a Svelte store.
- `refreshModules()` fetches `GET /api/v1/modules` and merges server config over the predefined definition using `moduleConfigToDefinition` and `normalizeModuleUiConfig`.
- Admin UI fetches `/api/v1/admin/modules` and `/api/v1/admin/templates` via `frontend/src/lib/api/admin-modules.ts`.

---

## 8. Existing test coverage & how to run

### Backend
| Test file | What it covers | How to run |
| --- | --- | --- |
| `backend/tests/notes_test.rs` | Create, read, save, rename, delete, list, visibility, bundle structure, H1 rename, attachment security, cross-tenant isolation, standalone `.md` compatibility. Requires Postgres + S3. | `cd backend && SQLX_OFFLINE=true cargo test --test notes_test -- --ignored` |
| `backend/tests/module_service_test.rs` | Default module creation, idempotency, canonical root paths. Requires Postgres. | `cd backend && SQLX_OFFLINE=true cargo test --test module_service_test -- --ignored` |
| `backend/server/src/services/module_service.rs` (inline `#[cfg(test)]`) | `normalize_module_ui_config`, icon validation, root-path validation, default definitions. | `cd backend && SQLX_OFFLINE=true cargo test --workspace` |
| `backend/crates/core/src/domain/module.rs` / `template.rs` | Serialization contracts, legacy alias parsing. | part of `cargo test --workspace` |

### Frontend
| Test file | What it covers | How to run |
| --- | --- | --- |
| `frontend/src/lib/api/notes.test.ts` | Pagination / unbounded list behavior of `listNotes`. | `cd frontend && npm run test` |
| `frontend/src/lib/components/modules/NotesModuleView.test.ts` | Create-note navigation, routing, attachment/drawing counts. | `cd frontend && npm run test` |
| `frontend/src/routes/(app)/modules/[key]/page.test.ts` | Module page shell (if present). | `cd frontend && npm run test` |

### Full stack
- Backend build/check: `cd backend && SQLX_OFFLINE=true cargo check --workspace`
- Frontend build/check: `cd frontend && npm run check && npm run build`

---

## 9. Proposed files to change (by subagent)

The following breakdown assumes 8 focused subagents. Each should make minimal, backward-compatible changes and add/update tests.

### Subagent 1 — Backend note model & metadata contract
- `backend/server/src/services/note_service.rs`
  - `NoteMetadata`, `Note`, `NoteSummary` structs.
  - `load_metadata` / `save_metadata` / `delete_metadata`.
- `backend/crates/core/src/domain/template.rs`
  - Default note template files (`template_default_note`).
- `backend/migrations/` (only if new metadata fields are required)
- Tests: `backend/tests/notes_test.rs`, inline service tests.

### Subagent 2 — Backend create / save / rename lifecycle
- `backend/server/src/services/note_service.rs`
  - `create_note`, `save_note`, `rename_note`, `duplicate_note`, `move_note`, `delete_note`.
- `backend/server/src/handlers/notes.rs`
  - All handler functions.
- `backend/server/src/routes.rs`
  - `note_routes()` and `note_public_routes()`.

### Subagent 3 — Backend module / template registry & defaults
- `backend/server/src/services/module_service.rs`
  - `default_modules()`, `ensure_default_modules()`, `normalize_module_ui_config()`.
- `backend/server/src/services/template_service.rs`
  - `ensure_default_templates()`, note template definition, `create_from_template()`.
- `backend/server/src/bootstrap.rs`
  - Seeding order.
- `backend/server/src/handlers/admin/modules.rs` and `handlers/admin/templates.rs`
- `backend/crates/core/src/domain/module.rs` / `template.rs` (if shape changes).
- Tests: `backend/tests/module_service_test.rs`, inline unit tests.

### Subagent 4 — Frontend API client & types
- `frontend/src/lib/api/notes.ts`
  - Create/save/rename/move/delete/duplicate/list functions.
- `frontend/src/lib/api/types.ts`
  - `Note`, `NoteMetadata`, `NoteAttachment`, `NoteSummary`.
- `frontend/src/lib/api/admin-modules.ts`
  - Module/template admin types if registry fields change.
- Tests: `frontend/src/lib/api/notes.test.ts`, `admin-modules.ts` tests if any.

### Subagent 5 — Frontend module list & routing
- `frontend/src/lib/components/modules/NotesModuleView.svelte`
  - List/grid rendering, search/filter, new-note flow.
- `frontend/src/routes/(app)/modules/[key]/+page.svelte`
- `frontend/src/routes/(app)/modules/[key]/ModulePageRenderer.svelte`
- `frontend/src/lib/modules/registry.ts`
  - Predefined Notes module definition, `refreshModules()` merge logic.
- `frontend/src/lib/modules/modulePages.ts`
  - `getModuleObjectHref` for notes.
- Tests: `NotesModuleView.test.ts`, `modules/[key]/page.test.ts`.

### Subagent 6 — Frontend editor / title behavior
- `frontend/src/lib/editor/components/MarkdownDocumentPage.svelte`
  - Title display, save dispatch, rename/menu actions.
- `frontend/src/routes/(app)/modules/[key]/[id]/+page.svelte`
  - Derive `title` from metadata vs. H1, attachment handling, save post-processing.
- Decide whether the editor should show a separate editable filename independent of the first H1 (per ADR-0029).
- Tests: new or existing editor/component tests.

### Subagent 7 — Admin module / template UI
- `frontend/src/routes/admin/modules/+page.svelte`
- `frontend/src/routes/admin/modules/[key]/edit/+page.svelte`
- `frontend/src/routes/admin/templates/+page.svelte`
- `frontend/src/routes/admin/templates/new/+page.svelte`
- `frontend/src/routes/admin/templates/[key]/edit/+page.svelte`
- `frontend/src/lib/modules/iconRegistry.ts` / `APPROVED_MODULE_ICONS` if new icons needed.

### Subagent 8 — Integration tests & test plan
- `backend/tests/notes_test.rs`
- `backend/tests/module_service_test.rs`
- `frontend/src/lib/api/notes.test.ts`
- `frontend/src/lib/components/modules/NotesModuleView.test.ts`
- Add OKF-specific scenarios (manifest round-trip, title/filename separation, registry defaults).

---

## 10. Concise test plan

1. **Unit / contract tests (no external services)**
   - `normalize_module_ui_config` preserves `modulePage` alias and adds canonical `page` keys.
   - Module/template serialization contracts (snake_case fields, legacy aliases).

2. **Backend integration tests (Postgres + S3)**
   - `create_note` creates a folder bundle with `note.md`, `_rustshare/manifest.json`, and sidecar.
   - `save_note` updates Markdown, sidecar, manifest `updated_at`, and excerpt.
   - **Title/filename separation:** changing H1 in content does **not** rename the folder; explicit `rename_note` does.
   - `rename_note` renames the bundle folder, updates sidecar title, preserves `public_share_id`.
   - `list_notes` derives display name from manifest/metadata for folder-backed notes and from filename for standalone `.md` files.
   - `delete_note` removes the entire bundle folder for folder-backed notes and only the file/sidecar for legacy notes.
   - `toggle_visibility` creates/revokes public share index.
   - Cross-tenant access is denied for get/save/rename/delete/list.
   - Attachment uploads reject unsafe names (`..`, `\`, `index.editor.json`).

3. **Registry tests**
   - `ensure_default_modules` creates canonical `/Workspace/Notes` root.
   - `ensure_default_templates` creates `template_default_note`.
   - Enabling/disabling a module updates visibility without deleting files.

4. **Frontend tests**
   - `NotesModuleView` creates a note and navigates to `/modules/notes/{id}`.
   - List renders title from `metadata.title` or `name`, strips `.md`, shows attachment/drawing counts.
   - Detail page loads note content, saves via autosave, and preserves relative attachment paths.
   - Admin module edit form round-trips UI config (sidebar, dashboard widget, page).

5. **Full-stack smoke tests**
   - `scripts/final-launch-smoke.sh` after `docker compose up -d`.
   - Manual: create note → edit H1 → verify folder name unchanged → rename via UI → verify folder and manifest title updated → disable/enable Notes module → verify files preserved.

6. **Backward-compatibility checks**
   - Existing standalone `.md` files in `/Workspace/Notes` still appear in list and save without folder rename.
   - Legacy `meta/notes/{id}.json` sidecars still load if present.
   - Legacy `/Notes` root path remains readable but new writes go to `/Workspace/Notes`.

---

## Appendix: key file index

| Path | Role |
| --- | --- |
| `backend/server/src/services/note_service.rs` | Core note CRUD, metadata, manifest, H1 rename logic |
| `backend/server/src/handlers/notes.rs` | HTTP API for notes |
| `backend/server/src/routes.rs` | Route registration (`note_routes`, `note_public_routes`, `module_routes`, `admin_routes`) |
| `backend/server/src/services/module_service.rs` | Module registry, defaults, summaries |
| `backend/server/src/services/template_service.rs` | Template registry, default note template, instantiation |
| `backend/server/src/handlers/admin/modules.rs` | Admin module handlers |
| `backend/server/src/handlers/admin/templates.rs` | Admin template handlers |
| `backend/server/src/handlers/modules.rs` | User-facing module handlers |
| `backend/server/src/bootstrap.rs` | Seeds default modules/templates on startup |
| `backend/crates/core/src/domain/module.rs` | `Module` domain struct |
| `backend/crates/core/src/domain/template.rs` | `Template`, `TemplateDefaultFile`, `CreateFromTemplateRequest` |
| `backend/tests/notes_test.rs` | Note integration tests |
| `backend/tests/module_service_test.rs` | Module registry integration tests |
| `frontend/src/lib/api/notes.ts` | Note API client |
| `frontend/src/lib/api/admin-modules.ts` | Admin module/template API client |
| `frontend/src/lib/api/types.ts` | Shared TypeScript types |
| `frontend/src/lib/components/modules/NotesModuleView.svelte` | Notes list/grid view |
| `frontend/src/routes/(app)/modules/[key]/+page.svelte` | Module page shell |
| `frontend/src/routes/(app)/modules/[key]/ModulePageRenderer.svelte` | Renderer dispatch |
| `frontend/src/routes/(app)/modules/[key]/[id]/+page.svelte` | Note detail / editor wrapper |
| `frontend/src/lib/editor/components/MarkdownDocumentPage.svelte` | Shared document editor page |
| `frontend/src/lib/modules/registry.ts` | Frontend predefined modules + server merge |
| `frontend/src/lib/modules/workspaceSurface.ts` | UI config normalization |
| `frontend/src/lib/modules/modulePages.ts` | Module path → folder id / href resolution |
| `frontend/src/routes/admin/modules/+page.svelte` | Admin modules list |
| `frontend/src/routes/admin/modules/[key]/edit/+page.svelte` | Admin module edit |
| `frontend/src/routes/admin/templates/+page.svelte` | Admin templates list |
| `frontend/src/routes/admin/templates/new/+page.svelte` | New template form |
| `frontend/src/routes/admin/templates/[key]/edit/+page.svelte` | Edit template form |

---

*Map generated by focused repo audit. No source code, tests, or behavior were modified.*
