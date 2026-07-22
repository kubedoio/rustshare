# RustShare Stabilization Refactoring Plan

Created as part of the 2026 stabilization directive.

## Goals

1. Establish one canonical Cargo workspace and one `Cargo.lock`.
2. Remove dead code and stale documentation from the Notes title/H1 fix.
3. Fix small, isolated frontend and backend bugs identified in the inventory.
4. Reduce redundant compilation in CI without weakening validation.
5. Document before/after measurements honestly.

## Phase 0 — Baseline

- [x] Record environment and commit SHA in `docs/audits/2026-rustshare-stabilization-baseline.md`.
- [ ] Complete clean/warm build, check, test-compile, and frontend timing measurements.
- [ ] Record results in the baseline doc.

## Phase 1 — Inventory

- [x] Create `docs/audits/2026-rustshare-bug-inventory.md`.
- [x] Verify Notes title/H1 behavior: already correct in code; docs and dead code are stale.
- [x] Verify deployment S3 bootstrap: already correct; backend auto-creates bucket idempotently.
- [x] Rank TODO/FIXME markers and skipped tests.
- [x] Document CI duplication opportunities.

## Phase 2 — Workspace and Dependency Cleanup

### 2.1 Consolidate to a single root workspace

- Remove `[workspace]` from `backend/Cargo.toml` (delete the file or replace with a package manifest that is itself a member; the simplest path is deletion).
- Delete `backend/Cargo.lock`.
- Update root `Cargo.toml` `workspace.dependencies` to be the single source of truth:
  - `tokio`: keep the current root feature set (`rt-multi-thread`, `macros`, `sync`, `time`, `net`, `io-util`, `fs`, `signal`); expand only if compilation proves it necessary.
  - `sqlx`: use `default-features = false` and add `macros`, `migrate` to match backend needs.
  - `jsonwebtoken`: use `default-features = false, features = ["aws_lc_rs"]` to match the production backend choice.
  - `reqwest`: keep `json`, `multipart`, `stream`, `rustls-tls`.
  - Add any missing workspace deps that backend crates currently resolve directly.
- Ensure all backend member manifests continue to resolve `workspace = true` deps from root.
- Run `cargo check --workspace` and `cargo test --workspace --lib --no-run` from root; fix any feature-related compilation errors.

### 2.2 Update active documentation

- `AGENTS.md`: replace `cd backend && …` with root-level commands.
- `CONTRIBUTING.md`: same.
- `docs/development.md`: same.
- `docs/agent-guides/testing.md` and `docs/agent-guides/code-quality.md`: same.
- `docs/DEPENDENCY_MANAGEMENT.md`: same.
- `backend/README.md` and `backend/TESTING.md`: update to root-level commands or note that backend is part of the root workspace.
- Do **not** rewrite historical plans/audits.

### 2.3 Update CI workflows

- `.github/workflows/ci.yml`: run `cargo` from repository root instead of `cd backend`. Use `-p rustshare-server` / `-p rustshare-core` etc. where targeted builds are needed.
- `.github/workflows/integration-tests.yml`: same.
- `.github/workflows/dependencies.yml`: same; update `cargo deny --manifest-path` to root `Cargo.toml`.
- Consolidate PostgreSQL/sqlx-cli setup into a reusable composite action if it reduces duplication.

## Phase 3 — Notes Title Cleanup

- Update `docs/implementation/okf-notes-implementation-map.md` sections 3 and 4 to describe current `save_note` behavior (no H1 extraction, no bundle rename on save).
- Remove dead `extract_h1_title` function from `backend/server/src/services/note_service.rs`.
- Remove the no-op `let _ = is_folder_backed;` line in `save_note`.
- Confirm existing tests compile and pass:
  - `frontend/src/lib/editor/components/MarkdownDocumentPage.test.ts`
  - `frontend/src/lib/components/modules/NotesModuleView.test.ts`
  - `backend/tests/notes_test.rs::contract_save_note_does_not_rename_bundle_folder_on_h1_change` (requires DB+S3; run ignored if infra available).

## Phase 4 — Small Isolated Bug Fixes

### 4.1 Kanban hard-coded actor

- Replace `actor: 'current-user'` in `frontend/src/lib/components/modules/KanbanModuleView.svelte` with the authenticated user from the auth store.
- Add/update a regression test if the component has test coverage.

### 4.2 FileThumbnail skipped tests

- Re-enable or rewrite the five skipped tests in `frontend/src/lib/components/files/FileThumbnail.test.ts`.
- If `onMount` cannot be exercised directly, move the fetch/thumbnail logic into a testable helper or use Svelte testing-library lifecycle helpers.

### 4.3 Other inventory items

- Defer security-sensitive items (AI ACL placeholder, upload-only share service-layer enforcement, vault-sync orphaned blobs) to dedicated follow-up milestones with human review.

## Phase 5 — Compile-Time and CI Optimization

- After workspace consolidation, re-run measurements and compare against baseline.
- Evaluate whether test-binary consolidation is needed; the current plan defers this unless measurements show linking dominates.
- Add path filters to `ci.yml` where safe (frontend-only, docs-only, backend-only changes).
- Consolidate duplicated PostgreSQL/sqlx-cli setup in CI.
- Document fast development commands:
  - `cargo check -p <pkg>`
  - `cargo test -p <pkg> --lib`
  - `cargo clippy -p <pkg> -- -D warnings`

## Phase 6 — Validation

- Run from repository root:
  - `cargo fmt --all --check`
  - `SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - `SQLX_OFFLINE=true cargo test --workspace --lib`
  - `SQLX_OFFLINE=true cargo test --workspace --lib --no-run`
- Frontend:
  - `npm run check`
  - `npm run lint`
  - `npm run test`
  - `npm run build`
- Record final measurements in `docs/audits/2026-rustshare-stabilization-result.md`.

## Phase 7 — Final Audit

- Create `docs/audits/2026-rustshare-stabilization-result.md` with baseline vs. final measurements, bugs fixed, refactors completed, dependencies changed, workspace changes, CI changes, known remaining problems, and deferred items.
- Update `CHANGELOG.md` for user-visible fixes (Notes title cleanup, Kanban actor fix).
