# RustShare Bug and Regression Inventory

Created as part of Phase 1 of the 2026 stabilization directive.

## How to read this inventory

Each entry includes severity, reproduction, affected component, whether data/permissions/compatibility are involved, existing test coverage, proposed minimal fix, and whether it belongs to this stabilization milestone.

Severity levels:

- **Critical**: data loss, security boundary violation, or clean deployment failure.
- **High**: significant correctness, security, or maintainability risk.
- **Medium**: localized bug, inconsistency, or missing validation.
- **Low**: documentation drift, dead code, or cosmetic issue.

---

## 1. Nested Cargo workspaces with diverging lockfiles and feature sets

- **Severity**: High
- **Component**: Build / workspace configuration (`Cargo.toml`, `backend/Cargo.toml`, `Cargo.lock`, `backend/Cargo.lock`)
- **Reproduction**:
  1. Run `cargo tree --workspace` from repository root.
  2. Run `cd backend && cargo tree --workspace`.
  3. Compare outputs and lockfiles (`diff Cargo.lock backend/Cargo.lock` shows 842 lines / 38 hunks).
- **Affected areas**:
  - Backend crates are members of both root and `backend/` workspaces.
  - `tokio` features differ: root uses a minimal set; backend uses `full`.
  - `sqlx` features differ: root keeps `default` + `any`; backend uses `default-features = false` + `macros` + `migrate`.
  - `jsonwebtoken` crypto provider differs: root uses `rust_crypto` (ring); backend uses `aws_lc_rs`.
  - `reqwest` `stream` feature enabled only in root.
  - 53 transitive dependencies resolved in multiple versions.
- **Data / permissions / compatibility involved?**: Yes — the same server crate can be built with different TLS/JWT crypto providers and different SQLx feature sets depending on working directory. This is a compatibility and supply-chain risk.
- **Existing test coverage**: None specifically for workspace consistency.
- **Proposed minimal fix**: Consolidate to a single canonical workspace. Remove `backend/Cargo.toml` `[workspace]` section and centralize `workspace.dependencies` in the root `Cargo.toml`. Use one `Cargo.lock`.
- **Milestone?**: Yes — Phase 2 of this stabilization.

---

## 2. Notes title/H1 separation — behavior fixed, docs and dead code stale

- **Severity**: Low (behavior already correct; docs/code drift)
- **Component**: Notes backend (`backend/server/src/services/note_service.rs`) and implementation docs (`docs/implementation/okf-notes-implementation-map.md`)
- **Reproduction**:
  1. Read `docs/implementation/okf-notes-implementation-map.md` lines 63–83 — it still claims `save_note` extracts H1 and renames the bundle folder.
  2. Read `backend/server/src/services/note_service.rs:2609` — `extract_h1_title` is dead code marked `#[allow(dead_code)]`.
- **Affected areas**: Documentation accuracy, dead-code removal.
- **Data / permissions / compatibility involved?**: No.
- **Existing test coverage**:
  - Backend integration test `backend/tests/notes_test.rs:669` (`contract_save_note_does_not_rename_bundle_folder_on_h1_change`) — currently `#[ignore]` (requires DB+S3).
  - Frontend tests `frontend/src/lib/editor/components/MarkdownDocumentPage.test.ts` and `frontend/src/lib/components/modules/NotesModuleView.test.ts`.
- **Proposed minimal fix**:
  1. Update `docs/implementation/okf-notes-implementation-map.md` to describe current behavior.
  2. Remove dead `extract_h1_title` and the no-op `let _ = is_folder_backed;` line.
  3. Confirm frontend unit tests and backend integration test still pass.
- **Milestone?**: Yes — small cleanup within stabilization.

---

## 3. Clean first-deployment S3 bucket bootstrap — already resolved

- **Severity**: Low (issue already fixed)
- **Component**: Deployment / object storage (`docker-compose.yml`, `backend/crates/storage/src/object_store.rs`, `backend/server/src/bootstrap.rs`)
- **Reproduction**:
  1. Review GitHub issue #154.
  2. Verify `docker-compose.yml` no longer starts a temporary MinIO client container.
  3. Verify `ensure_bucket_exists` in `object_store.rs:427` is idempotent and handles `BucketAlreadyExists` / `BucketAlreadyOwnedByYou`.
- **Affected areas**: Clean first-start behavior.
- **Data / permissions / compatibility involved?**: No — behavior is correct.
- **Existing test coverage**: No dedicated regression test for bootstrap idempotency.
- **Proposed minimal fix**: Document the resolution in this inventory; no code change required. Optionally add a unit test for `ensure_bucket_exists` idempotency (requires mocking the S3 client).
- **Milestone?**: No code change needed; verified and documented.

---

## 4. AI indexing ACL placeholder (group/share read access not resolved)

- **Severity**: High (security boundary)
- **Component**: AI / search / indexing (`backend/crates/core/src/services/ai/indexing.rs:40`)
- **Reproduction**: Read the TODO marker and surrounding permission-filter logic.
- **Affected areas**: Search / RAG result visibility.
- **Data / permissions / compatibility involved?**: Yes — group and share-based read access may not be enforced in AI search results.
- **Existing test coverage**: Unknown; likely insufficient.
- **Proposed minimal fix**: Resolve the principal correctly using the existing permission resolver, add regression tests, and include a security-impact statement. **This is a security-sensitive change and requires human review per `AGENTS.md`.**
- **Milestone?**: No — defer to a dedicated AI/security milestone. Treating this as a stabilization bug would expand scope into a sensitive area.

---

## 5. Upload-only share enforcement only at HTTP handler, not service layer

- **Severity**: Medium (security boundary)
- **Component**: Sharing / uploads (`backend/tests/contracts/public_upload_only_contract.rs:512`, related handlers/services)
- **Reproduction**: Review the contract test and the service-layer share permission checks.
- **Affected areas**: Upload-only shares.
- **Data / permissions / compatibility involved?**: Yes — a bypass or internal caller could list folder contents.
- **Existing test coverage**: Contract test exists (`public_upload_only_contract.rs`).
- **Proposed minimal fix**: Move the upload-only restriction into the shared service/domain layer and add service-layer tests. **Security-sensitive; requires human review.**
- **Milestone?**: Potentially yes if small and well-scoped; but because it touches permissions, human review is required before merge.

---

## 6. Contended vault-sync writes can leave orphaned blobs

- **Severity**: Medium
- **Component**: Vault synchronization (`backend/crates/core/src/services/vault_sync_service.rs:262`)
- **Reproduction**: Review the TODO marker and conflict-resolution path.
- **Affected areas**: Object-storage usage for vault sync.
- **Data / permissions / compatibility involved?**: Data lifecycle — orphaned blobs accumulate.
- **Existing test coverage**: Unknown.
- **Proposed minimal fix**: Add a background GC or deterministic cleanup; large enough that it should be a feature milestone.
- **Milestone?**: No — defer to vault-sync hardening milestone.

---

## 7. Kanban card comments hard-code `actor: 'current-user'`

- **Severity**: Low / Medium
- **Component**: Frontend / Kanban (`frontend/src/lib/components/modules/KanbanModuleView.svelte:595`)
- **Reproduction**: Inspect the comment-creation path in the Kanban view.
- **Affected areas**: Activity attribution / audit trail.
- **Data / permissions / compatibility involved?**: Permissions — activity may be attributed to the wrong user.
- **Existing test coverage**: Unknown.
- **Proposed minimal fix**: Use the authenticated user from the auth store instead of the hard-coded string, add a regression test.
- **Milestone?**: Yes — small isolated bug fix.

---

## 8. AI/index readiness health check is a stub

- **Severity**: Low
- **Component**: Health checks (`backend/server/src/handlers/health.rs:134`)
- **Reproduction**: Inspect `ComponentHealth::healthy()` usage in readiness probe.
- **Affected areas**: Operational readiness reporting.
- **Data / permissions / compatibility involved?**: No.
- **Existing test coverage**: Unknown.
- **Proposed minimal fix**: Replace stub with a real check or remove AI from readiness if not actionable; document decision.
- **Milestone?**: Maybe — low priority, defer if not trivial.

---

## 9. `FileThumbnail` tests skipped due to `onMount` not running in happy-dom

- **Severity**: Low
- **Component**: Frontend / files (`frontend/src/lib/components/files/FileThumbnail.test.ts`)
- **Reproduction**: Run `npm run test`; observe 5 skipped tests.
- **Affected areas**: Thumbnail download, generation, failure, and a11y paths.
- **Data / permissions / compatibility involved?**: No.
- **Existing test coverage**: Tests exist but are skipped.
- **Proposed minimal fix**: Either configure tests to run `onMount` (e.g., use `render` + `tick` / lifecycle helpers) or replace the skipped assertions with tests that exercise the component logic directly.
- **Milestone?**: Yes — small test-quality fix if time permits; otherwise document as deferred.

---

## 10. Large number of ignored Rust integration tests

- **Severity**: Medium
- **Component**: Backend test suite
- **Reproduction**: `grep -R '#\[ignore' backend/` returns 374 occurrences in 46 files.
- **Affected areas**: Integration coverage for DB+S3-dependent paths.
- **Data / permissions / compatibility involved?**: No.
- **Existing test coverage**: The tests exist but are not run in normal `cargo test --lib`.
- **Proposed minimal fix**: Do not unilaterally remove `#[ignore]` — these require infrastructure. Instead, ensure CI runs them (`cargo test --all-features -- --ignored`) and that local integration-test instructions are clear. This is already partially true (`integration-tests.yml`).
- **Milestone?**: No — keep existing ignore policy; verify CI runs ignored integration tests.

---

## 11. CI duplicated compilation and setup

- **Severity**: Medium
- **Component**: GitHub Actions (`.github/workflows/ci.yml`, `.github/workflows/dependencies.yml`, `.github/workflows/integration-tests.yml`)
- **Reproduction**: Compare Rust-compiling jobs across workflows.
- **Affected areas**: CI duration, cache efficiency.
- **Data / permissions / compatibility involved?**: No.
- **Existing test coverage**: N/A.
- **Proposed minimal fix**: Consolidate overlapping jobs, extract PostgreSQL/sqlx-cli setup into a composite action, add path filters to `ci.yml`, distinguish fast PR validation from full validation. See Phase 6 of the stabilization directive.
- **Milestone?**: Yes — Phase 6.

---

## Stabilization-scope summary

| # | Item | Severity | Milestone action |
|---|------|----------|------------------|
| 1 | Nested workspaces / diverging lockfiles | High | Phase 2 — consolidate |
| 2 | Notes title docs / dead code | Low | Fix within stabilization |
| 3 | S3 bucket bootstrap | Low | Verified, no change |
| 4 | AI indexing ACL placeholder | High | Defer — security-sensitive |
| 5 | Upload-only share service-layer enforcement | Medium | Consider if small; human review required |
| 6 | Vault-sync orphaned blobs | Medium | Defer |
| 7 | Kanban hard-coded actor | Low / Medium | Fix within stabilization |
| 8 | AI readiness stub | Low | Defer or trivial fix |
| 9 | Skipped FileThumbnail tests | Low | Fix if time permits |
| 10 | Ignored Rust integration tests | Medium | Verify CI runs them |
| 11 | CI duplication | Medium | Phase 6 — optimize |

---

## Notes on scope discipline

- Items 4, 5, 6, and 8 touch security, permissions, or data-lifecycle boundaries. Per `AGENTS.md`, these require explicit regression tests and a security note, and should be reviewed by a human before merge. They are intentionally deferred or treated as optional in this stabilization to avoid scope creep into sensitive areas.
- The required Notes bug and the required Deployment check are both already correct in current code; the stabilization work here is documentation cleanup, dead-code removal, and verification.
