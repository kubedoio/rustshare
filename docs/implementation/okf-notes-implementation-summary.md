# OKF-native Notes Module — Implementation Summary

> GitHub issue #118

## Summary of changed files

### Backend — OKF frontmatter core
- `backend/crates/core/src/okf.rs` — module re-exports.
- `backend/crates/core/src/okf/frontmatter.rs` — parser/serializer, `OkfNoteFrontmatter`, `RustshareFrontmatter`, merge/round-trip helpers, unit tests.
- `backend/crates/core/Cargo.toml` — added `serde_yaml` dependency.
- `backend/crates/core/src/lib.rs` — registered `okf` module.

### Backend — Notes behavior
- `backend/server/src/services/note_service.rs` — OKF-native create/save/rename/duplicate/move/delete, stable `rustshare.id`, H1-driven folder rename removed, reconciliation, conflict resolution, ACL payload builder, `NoteConflict`, tests.
- `backend/server/src/handlers/notes.rs` — response structs include `okf_id` and `conflict`.

### Backend — Admin module/template registry
- `backend/crates/core/src/domain/module.rs` — OKF config support.
- `backend/crates/core/src/domain/template.rs` — serialization contract updates.
- `backend/server/src/services/module_service.rs` — notes module defaults to `okf-note` renderer, `okf-markdown` document format, `template_default_okf_note`, OKF + AI indexing policies.
- `backend/server/src/services/template_service.rs` — new `template_default_okf_note` with OKF frontmatter and bundle structure.
- `backend/server/src/handlers/admin/modules.rs` / `templates.rs` — expose new fields through existing JSONB columns.

### Backend — RAG / indexing ACL contract
- `backend/crates/core/src/services/ai/indexing.rs` — `NoteAclPayload`, `AclSearchFilter`, ACL-aware `index_note`, `search_with_acl`, `update_note_acl`, `remove_note_chunks`, tests.
- `backend/crates/core/src/services/ai/embedding.rs` — `Send` future return for object-safe sink usage.
- `backend/server/src/services/note_index_sink.rs` — `NoteIndexSink` trait, `NoOpNoteIndexSink`, `ContentIndexerNoteSink`.
- `backend/server/src/bootstrap.rs` — wires shared `ContentIndexer` into both `AiService` and `NoteService`.

### Backend tests
- `backend/tests/notes_test.rs` — OKF creation, stable id, H1 no-rename, explicit rename, external reconciliation, duplicate-id detection.
- `backend/tests/module_service_test.rs` — module/template registry defaults, OKF frontmatter shape validation.

### Frontend
- `frontend/src/lib/api/types.ts` — added `okf_id`, `conflict`, `acl_hash`, `acl_version` to note types.
- `frontend/src/lib/editor/adapter/frontmatter.ts` — split/wrap OKF frontmatter helper.
- `frontend/src/lib/editor/adapter/frontmatter.test.ts` — unit tests.
- `frontend/src/lib/editor/components/MarkdownDocumentPage.svelte` — hides frontmatter in rich mode, raw-Markdown toggle, preserves frontmatter on save.
- `frontend/src/routes/(app)/modules/[key]/[id]/+page.svelte` — title from metadata/name, conflict banner, subtitle from H1/excerpt.
- `frontend/src/lib/modules/registry.ts`, `workspaceSurface.ts` — predefined notes module updated to `okf-note` / `okf-markdown`.
- `frontend/src/routes/admin/modules/+page.svelte`, `[key]/edit/+page.svelte` — display OKF config, AI indexing source, permission-aware flag.
- `frontend/src/routes/admin/templates/+page.svelte` — shows OKF note template details.
- `frontend/src/lib/api/admin-modules.test.ts`, `frontend/src/routes/admin/modules/page.test.ts`, `NotesModuleView.test.ts`, `MarkdownDocumentPage.test.ts` — updated/new tests.

### Documentation
- `docs/implementation/okf-notes-implementation-map.md` — audit map.
- `docs/implementation/okf-notes-implementation-summary.md` — this file.
- `docs/adr/0019-notes-as-okf-documents.md`
- `docs/adr/0020-okf-notes-reconciliation-and-rag-safety.md`
- `CHANGELOG.md` — added entry under `[Unreleased]`.

## Test results

- `cd backend && SQLX_OFFLINE=true cargo check --workspace` — passes.
- `cd backend && SQLX_OFFLINE=true cargo clippy --all-features -- -D warnings` — passes.
- `cd backend && SQLX_OFFLINE=true cargo test --workspace --lib --bins` — passes (all crates, including new OKF frontmatter, note service, and ACL indexing tests).
- `cd backend && SQLX_OFFLINE=true cargo test --test notes_test --no-run` — compiles.
- `cd backend && SQLX_OFFLINE=true cargo test --test module_service_test --no-run` — compiles.
- `cd frontend && npm run check` — 0 errors.
- `cd frontend && npm run lint` — 0 errors, 137 pre-existing warnings.
- `cd frontend && npm run test` — 76 test files passed, 840 tests passed, 5 skipped.

Integration tests under `backend/tests/` are marked `#[ignore]` because they require Postgres + S3, matching the existing repo convention.

## Known limitations

1. **Conflict resolution UI is basic.** The backend supports `PreferYaml`, `PreferFolder`, and `Custom` resolutions; the frontend banner is informational and exposes the conflict state. A full conflict-resolution workflow can be added later.
2. **`resource` workspace id uses tenant id.** There is no separate workspace entity yet; the resource URI uses `tenant_id` as the workspace identifier.

## Follow-up issues

1. Enhance frontend conflict resolution with action buttons that call `resolve_note_conflict`.
