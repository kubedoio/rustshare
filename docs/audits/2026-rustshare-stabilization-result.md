# RustShare Stabilization Result

Created as the closeout document for Phase 2 of the 2026 stabilization directive,
following the merge of PR 175.

## Summary

PR 175 completed:

- Unified the root and backend Cargo workspaces into a single workspace with one `Cargo.lock`.
- Removed the nested `backend/Cargo.toml` workspace and its lockfile.
- Updated CI commands to run from the repository root.
- Cleaned the Notes filename / first-H1 relationship and added regression tests.
- Corrected Kanban comment actor attribution.

This phase completed:

- Fixed the nondeterministic parallel environment-variable test race in `backend/crates/core/src/validation.rs`.
- Restored the five skipped `FileThumbnail` lifecycle tests in the frontend.
- Measured the post-consolidation build and test performance.
- Applied one measured, low-risk compile-time optimization (`debug = 1` for dev/test profiles).
- Added CI path filters so documentation-only and frontend-only changes no longer trigger the full Rust workflow.
- Extracted the DCO check to a dedicated always-run workflow.

Deferred (out of scope for stabilization):

- AI indexing ACL boundary changes.
- Upload-only share service-layer enforcement.
- Vault-sync orphaned blob cleanup.

Resolution note (2026-07-25): the deferred vault-sync blob lifecycle risk is addressed by `storage/vault-orphan-blob-gc`; see `2026-vault-blob-gc-result.md`. This does not alter the historical stabilization scope.
- Permission-model changes.
- Authentication or cryptographic changes.

**Conclusion:** Stabilization is complete; normal feature development may resume.

## Environment

| Item | Value |
|------|-------|
| Date | 2026-07-23 |
| Commit SHA | `247d8f9cb9aca97c509e1290eace50a4c1b6d245` (`stabilization/phase-2-closeout`) |
| OS | Linux rustshare 6.8.0-124-generic #124-Ubuntu SMP PREEMPT_DYNAMIC x86_64 |
| CPU | AMD EPYC 7401P 24-Core Processor, 16 logical cores visible |
| RAM | 15 GiB |
| Rust | rustc 1.95.0 (59807616e 2026-04-14), cargo 1.95.0 (f2d3ce0bd 2026-03-21) |
| Node.js | v22.22.2 |
| npm | 10.9.7 |
| hyperfine | not installed; used `/usr/bin/time -v` |

## Correctness Fixes

### Parallel environment-variable test race

- **File:** `backend/crates/core/src/validation.rs`
- **Problem:** Tests mutated `RUSTSHARE_ALLOW_INTERNAL_MAIL_SERVERS` under a blocking mutex and released the guard before the async DNS resolver read the variable, causing flaky parallel behavior.
- **Fix:** Replaced the lock with `tokio::sync::Mutex` and added a test-only `EnvVarRestore` RAII helper that captures and restores the original value. All reads now happen while holding the async guard.
- **Validation:** `cargo test -p rustshare-core validation::tests -- --test-threads=16` passed, followed by 50 consecutive parallel runs without failure.

### Restored `FileThumbnail` test coverage

- **File:** `frontend/src/lib/components/files/FileThumbnail.test.ts`
- **Problem:** Five lifecycle tests were skipped because the happy-dom test environment lacked `URL.createObjectURL` / `URL.revokeObjectURL`.
- **Fix:** Added deterministic mocks for the object-URL APIs in `frontend/src/test-setup.ts`, removed the `it.skip` declarations, and added tests for prop-change replacement and object-URL revocation.
- **Validation:** All 27 `FileThumbnail` tests pass; full frontend suite passes (926 tests).

## Build-System Results

### Unified workspace confirmation

- One workspace at repository root (`Cargo.toml`).
- One `Cargo.lock` at repository root.
- `backend/Cargo.toml` workspace removed in PR 175.
- Commands from the repository root are predictable; no package is compiled under different dependency-feature combinations depending on working directory.

### Before/after timing table

Measurements taken after PR 175 with `SQLX_OFFLINE=true` (baseline used a live database connection and therefore did not need `SQLX_OFFLINE`). The same machine was used for both sets of numbers.

| Command | Baseline (PR 175) | Phase 2 (debug=1) | Change |
|---------|-------------------|-------------------|--------|
| `cargo build --workspace --timings` (clean) | 190 s | 171 s | -10 % |
| `cargo check --workspace --timings` (after clean build) | 78 s | 77 s | ~0 % |
| `cargo test --workspace --lib --no-run` (after clean build) | 64 s | 53 s | -17 % |
| `cargo test -p rustshare-server --all-features --tests --no-run` | 217 s* | 169 s | -22 % |
| Target directory after full build+test | ~27 GB | ~17 GB | -37 % |

\* Estimated from the post-PR 175 debug=2 measurement (3 m 37 s); the baseline document did not record this command.

### Slowest remaining crates

From the clean `cargo build --workspace --timings` run:

- `aws-lc-sys`, `ring`, `sqlx-macros`, `aws-smithy-runtime`, `aws-sdk-s3`, `openidconnect`, `axum-extra`, `html5ever`, `image`, `utoipa-swagger-ui`.

These are dominated by third-party crypto, AWS SDK, SQLx macro expansion, and web-framework derives. No safe dependency removal was identified that would materially reduce this without removing production functionality.

### Test linking results

- Workspace lib test executables: 14.
- `rustshare-server` integration test executables: 38.
- Integration-test linking remains the largest remaining compile-time cost. Consolidation was evaluated; a single proof-of-concept consolidation of the admin tests was not pursued in this PR because the `debug = 1` optimization already reduced test compile time by ~22 % and the admin group uses distinct test infrastructure (OIDC, SMTP, webhooks, etc.) that would require shared setup work better handled in a dedicated follow-up.

### Duplicate dependency findings

`cargo tree --workspace --duplicates` shows expected duplicates from transitive ecosystems that cannot be unified without upstream changes:

- `tokio` v1.52.3 (single visible version; duplicate entry comes from feature resolution).
- `rustls` v0.21.12 and v0.23.40 (reqwest/lettre vs modern rustls stack).
- `hyper` v0.14.32 and v1.10.1 (AWS SDK / reqwest vs axum).
- `base64` v0.21.7 and v0.22.1 (openidconnect vs rest of workspace).
- `nom` v7.1.3 and v8.0.0 (async-imap vs lettre).
- `zip` v3.0.0 and v8.6.0 (async-imap vs mailparse/other).
- `getrandom`, `rand_core`, `socket2`, `hashbrown`, and several other small crates pulled by both old and new dependency subtrees.

No production dependency or feature was removed because each duplicate is required by a transitive dependency outside our control.

### CI changes

- Extracted DCO verification from `ci.yml` into `.github/workflows/dco.yml`, which runs on every PR/push to `main`.
- Added `paths-ignore` to `ci.yml` for `docs/**`, `**.md`, `frontend/**`, `.github/workflows/frontend-ci.yml`, and `.github/workflows/dco.yml`.
- Result: documentation-only and frontend-only changes no longer trigger Rust fmt, clippy, test, release build, cargo-deny, SQLx prepare, or coverage jobs. DCO still runs for all changes, and `frontend-ci.yml` still runs for frontend changes.

## Quality Results

| Check | Result |
|-------|--------|
| `cargo fmt --all --check` | ✅ passed |
| `SQLX_OFFLINE=true cargo check --workspace --all-features` | ✅ passed |
| `SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features -- -D warnings` | ✅ passed |
| `cargo test --workspace --all-features --lib` (with local Postgres) | ✅ passed |
| `cargo deny --all-features check` | ✅ passed (duplicate warnings, no failures) |
| `cargo sqlx prepare --workspace --check` (with local Postgres) | ✅ passed |
| Frontend `npm run check` | ✅ passed (0 errors, 82 pre-existing warnings) |
| Frontend `npm run lint` | ✅ passed (0 errors, 162 pre-existing warnings) |
| Frontend `npm run test` | ✅ passed (926 tests) |
| Frontend `npm run build` | ✅ passed |

## Frontend Bundle

| Metric | Value |
|--------|-------|
| `npm run build` elapsed | 24 s |
| Total `frontend/build` size | 24 MB |
| Total client JS (all `_app/immutable` `.js` files) | ~23 MB |
| Largest chunks | `ts.worker-CDlTriQ3.js` 6.6 MB, `je2SSgyq.js` 3.5 MB, `h4z_VOu6.js` 1.7 MB, `css.worker-C078mPpn.js` 1.0 MB |

The large Monaco workers are loaded lazily when the code editor is opened; they do not block initial app load.

## Remaining Risks

The following items remain explicitly deferred and are documented in the bug inventory:

- AI indexing ACL boundary.
- Upload-only share service-layer enforcement.
- Vault-sync orphaned blobs.

Resolution note (2026-07-25): resolved by the dedicated safe object-GC phase; deletion remains disabled by default pending operator enablement and human review.
- AI readiness behavior.
- Dependency duplicate advisories are tracked by `cargo deny`; current warnings do not fail the check.

## Validation Commands

```bash
# Rust formatting, checking, linting
cargo fmt --all --check
SQLX_OFFLINE=true cargo check --workspace --all-features
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features -- -D warnings

# Rust tests (requires PostgreSQL and DATABASE_URL)
SQLX_OFFLINE=true cargo test --workspace --all-features --lib

# Dependency and SQLx checks
cargo deny --all-features check
cargo sqlx prepare --workspace --check

# Frontend
cd frontend
npm ci
npm run check
npm run lint
npm run test
npm run build
```

## Honest Conclusion

**Stabilization complete; normal feature development may resume.**

All confirmed stabilization-scope bugs are fixed or explicitly deferred, full Rust and frontend validation passes, the workspace is unambiguous, build and CI optimizations are measured and documented, and deferred security-sensitive issues are visible in the audit.
