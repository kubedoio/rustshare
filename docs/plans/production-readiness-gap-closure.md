# Production Readiness Gap Closure Plan

**Status:** Approved — implementation in progress  
**Branch:** `production-readiness-gap-closure`  
**Worktree:** `.worktrees/production-readiness-gap-closure`  
**Quality bar:** Industrial grade. Every change must be tested, reviewed, and committed.

## Goal

Close the highest-risk production readiness gaps identified in the 2026-06-09 production-readiness audit and focused code review. The scope is intentionally limited to six workstreams so each can be implemented, reviewed, and verified independently.

## Cross-Cutting Rules

- Work only in `.worktrees/production-readiness-gap-closure`.
- Do not push to remotes.
- Write tests before or alongside implementation changes.
- Run `SQLX_OFFLINE=true cargo check --workspace` and the relevant test suites after each workstream.
- Update `CHANGELOG.md` for user-visible or operator-visible changes.
- Keep changes minimal and focused on the workstream.

---

## Workstream A: Security Hardening

**Theme:** Close authentication, injection, and secret-handling gaps.

### A.1 Verify chat webhook signatures

**Files:**
- `backend/crates/core/src/services/chat_integration.rs`
- `backend/crates/crypto/src/webhook_signature.rs`
- `backend/server/src/handlers/chat_integration.rs`
- `backend/crates/storage/src/chat_integration_impl.rs`

**Current state:** ✅ Already implemented — `process_incoming_event` now verifies the signature over the raw request body before deserialization and returns `SignatureVerificationFailed` on missing/invalid signatures. The HTTP handler returns 401 for missing/invalid signatures.

**Required behavior:**
- Verify the implementation is complete and add any missing unit/integration tests.
- Ensure test coverage for valid, invalid, missing, and tampered signatures.

### A.2 Escape Content-Disposition filenames

**Files:**
- `backend/server/src/handlers/public_shares.rs`

**Current state:** ✅ Partially implemented — `build_content_disposition` escapes quotes and adds `filename*=UTF-8''`.

**Required behavior:**
- Harden against newline, carriage-return, and control characters in filenames.
- Add tests for filenames containing `"`, `\n`, `\r`, backslash, and Unicode.

### A.3 Reject HTTP chat webhooks in production

**Files:**
- `backend/crates/core/src/services/chat_integration.rs`
- `backend/server/src/handlers/chat_integration.rs`

**Current state:** Chat webhook URLs can be plain `http://`.

**Required behavior:**
- Add a configuration flag or environment-driven policy that rejects non-HTTPS webhook URLs unless explicitly allowed (e.g., `RUSTSHARE_ALLOW_HTTP_WEBHOOKS=true` for local dev only).
- Return a clear error when an HTTP URL is rejected.
- Default to requiring HTTPS.
- Add tests for both modes.

### A.4 Secure session cookie defaults

**Files:**
- `backend/crates/auth/src/web_session.rs`
- `backend/server/src/bootstrap.rs`

**Current state:** `SESSION_COOKIE_SECURE` defaults to `false`.

**Required behavior:**
- Default `SESSION_COOKIE_SECURE` to `true`.
- Allow explicit opt-out only via `RUSTSHARE_SESSION_COOKIE_SECURE=false`.
- Ensure `HttpOnly` and `SameSite` are set correctly.
- Add tests for cookie flags.

### A.5 Enforce admin authentication on admin routes

**Files:**
- `backend/server/src/handlers/admin/*.rs`
- `backend/server/src/routes.rs`

**Current state:** Some admin handlers may not consistently require `AdminUser`.

**Required behavior:**
- Audit all admin routes and ensure they use the `AdminUser` extractor.
- Add tests that anonymous and non-admin requests are rejected.

### A.6 Remove plaintext bootstrap admin password from logs

**Files:**
- `backend/server/src/bootstrap.rs`

**Current state:** ✅ Already implemented — password is written to a secure file with `0600` permissions and only the file path is logged.

**Required behavior:**
- Verify the existing implementation with a test or script check that the password does not appear in stdout/stderr logs.

---

## Workstream B: Multi-Tenant Isolation

**Theme:** Ensure repository queries enforce tenant boundaries and RLS context reflects the authenticated tenant.

### B.1 Add tenant filtering to repository queries

**Files:**
- `backend/crates/infrastructure/src/repositories/permission_resolver.rs`
- Other repository files as needed (`file_repository.rs`, `folder_repository.rs`, `share_repository.rs`, `user_repository.rs`, `notification_repository.rs`)

**Current state:** Many repository methods accept only an object ID and do not filter by `tenant_id`.

**Required behavior:**
- Pass `tenant_id` into repository methods that query tenant-scoped data.
- Update SQLx queries to include `WHERE tenant_id = $N`.
- Regenerate `.sqlx` offline query metadata (`cargo sqlx prepare` or `SQLX_OFFLINE=true cargo check`).
- Add contract tests proving user/tenant B cannot read tenant A's objects.

### B.2 Set RLS context per request

**Status:** Removed — no-op middleware deleted.

**Files:**
- `backend/server/src/bootstrap.rs`
- `backend/server/src/middleware/*.rs`
- `backend/server/src/state.rs`

**Current state:** The previous `tenant_context` middleware acquired a pool connection, ran `SET app.current_tenant_id` / `SET app.current_user_id`, and returned the connection to the pool *before* the inner handler ran. Because handlers check out separate connections, the settings were never visible to handler queries, making the middleware an ineffective security control.

**Decision:** Remove the no-op middleware and its empty integration test. Repository-level tenant filtering (added in B.1 and B.3) remains the active and primary defense against cross-tenant access. PostgreSQL RLS can be reintroduced later only if it can be applied on the same connection that executes handler queries (e.g. per-request connection pinning or explicit `SET` on every acquired connection via `before_acquire`).

### B.3 Add tenant-isolation contract tests

**Files:**
- `backend/tests/contracts/tenant_isolation_contract.rs` (new)

**Required behavior:**
- Create fixtures for two tenants with one user each.
- For files, folders, shares, and notifications, assert that tenant B cannot get, list, update, or delete tenant A's objects.
- Assert that cross-tenant share links are rejected.

---

## Workstream C: Large-Object Streaming

**Theme:** Stop loading entire objects into memory for upload and download.

### C.1 Stream downloads from object storage

**Files:**
- `backend/crates/storage/src/object_store.rs`
- `backend/server/src/handlers/files.rs`
- `backend/server/src/handlers/public_shares.rs`

**Current state:** `object_store.get(key)` collects the full S3 body into `Bytes`.

**Required behavior:**
- Add `get_stream(key) -> impl Stream<Item = Result<Bytes>>` or return an `aws_sdk_s3::operation::get_object::GetObjectOutput` body stream.
- Update download handlers to return `StreamBody`/`Body` without buffering the whole file.
- Preserve Content-Type and Content-Length where available.
- Add tests with a large synthetic object and confirm low memory usage.

### C.2 Stream multipart uploads to temporary files

**Files:**
- `backend/server/src/handlers/files.rs`
- `backend/server/src/handlers/upload.rs`
- `backend/server/src/handlers/public_shares.rs`
- `backend/crates/core/src/services/upload_service.rs`

**Current state:** `field.bytes().await` buffers the entire upload into RAM.

**Required behavior:**
- Stream multipart fields to temporary files on disk in chunks.
- Stream from the temporary file to object storage.
- Clean up temp files on success and on error.
- Enforce a configurable max upload size.
- Add tests for small and large uploads, aborted uploads, and cleanup.

### C.3 Add upload-streaming integration tests

**Files:**
- `backend/tests/upload_streaming_test.rs` (new)

**Required behavior:**
- Upload a multi-MB file and assert the server does not hold it fully in memory (e.g., via memory instrumentation or by exercising the streaming path).
- Test resume/aborted behavior and temp-file cleanup.

---

## Workstream D: CI/CD & Deployment Hardening

**Theme:** Remove hardcoded secrets and tighten CI security.

### D.1 Remove hardcoded secrets from GitHub Actions

**Files:**
- `.github/workflows/integration-tests.yml`
- `.github/workflows/ci.yml`
- `.github/workflows/frontend-ci.yml`

**Current state:** JWT secrets, encryption keys, AWS credentials, and admin passwords are hardcoded in workflow files.

**Required behavior:**
- Replace hardcoded values with `${{ secrets.XXX }}` references or generate secure random values in a setup step.
- Document required secrets in `.github/workflows/README.md` or `docs/CI_SECRETS.md`.
- Ensure CI still passes with the new approach (e.g., generate dev keys on the fly).

### D.2 Add secret-scanning / audit gate

**Files:**
- `.github/workflows/ci.yml`
- `.pre-commit-config.yaml` if present

**Required behavior:**
- Add a CI step that scans for high-entropy strings and known secret patterns.
- Block merges if secrets are detected.
- Do not add real secrets to test fixtures; use deterministic fake values if needed.

### D.3 Document required production secrets

**Files:**
- `docs/DEPLOYMENT.md`
- `.env.example`

**Required behavior:**
- List every secret/env var required for production.
- Provide rotation guidance.
- Mark dev-only overrides.

---

## Workstream E: Code Quality & Test Gaps

**Theme:** Fix ignored tests, clippy warnings, and coverage holes.

### E.1 Re-enable and fix ignored tests

**Files:**
- All `#[ignore]` tests in the backend workspace.

**Current state:** ~370 backend tests are marked `#[ignore]`.

**Required behavior:**
- For each ignored test, either fix it, replace it, or delete it with justification.
- Do not simply remove `#[ignore]` without making the test meaningful and passing.
- Document any tests that remain ignored and why.

### E.2 Fix clippy warnings across all targets

**Files:**
- Backend workspace.

**Current state:** `cargo clippy --all-targets --all-features` fails.

**Required behavior:**
- Run clippy and fix warnings/errors in library, binary, and test code.
- Aim for `cargo clippy --all-targets --all-features -- -D warnings` to pass.

### E.3 Address cargo audit advisories

**Files:**
- `backend/Cargo.toml`
- `backend/Cargo.lock`
- `backend/.cargo/audit.toml`

**Current state:** rustls-webpki and RSA advisories are reported.

**Required behavior:**
- Upgrade or patch affected dependencies.
- Re-run `cargo audit` until no relevant advisories remain.
- If an advisory cannot be fixed, document the risk and mitigation.

### E.4 Add request-scoped tracing/correlation IDs

**Files:**
- `backend/server/src/main.rs`
- `backend/server/src/middleware/*.rs`

**Required behavior:**
- Generate a request ID at the edge and propagate it through logs.
- Add a Tower layer or middleware that sets the request ID in tracing spans.
- Add tests that verify the ID is present in response headers.

---

## Workstream F: Documentation & Operational Consistency

**Theme:** Keep docs accurate and aligned with the implemented changes.

### F.1 Update PRODUCTION_READINESS.md

**Files:**
- `docs/PRODUCTION_READINESS.md`

**Required behavior:**
- Mark completed checklist items.
- Update risk statements based on closed gaps.
- Remove or fix broken references.

### F.2 Create missing operational runbooks

**Files:**
- `docs/2026-03-21-alerting-and-incident-thresholds.md` (create or fix)
- `docs/2026-03-21-oidc-production-validation-checklist.md` (create or fix)
- `docs/backup-restore.md` (create or fix)
- `docs/DEPLOYMENT.md` (update)

**Required behavior:**
- Create or complete each referenced document.
- Ensure content matches current code and deployment paths.

### F.3 Document security changes

**Files:**
- `docs/security-model.md`
- `CHANGELOG.md`

**Required behavior:**
- Document webhook signature verification, cookie security, admin auth enforcement, and tenant isolation.
- Add CHANGELOG entries for user/operator-visible changes.

---

## Verification Checklist (Final)

- [ ] `SQLX_OFFLINE=true cargo check --workspace` passes.
- [ ] `SQLX_OFFLINE=true cargo test --workspace` passes (with no new ignored tests).
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo audit` passes or documented.
- [ ] `cargo deny check` passes or documented.
- [ ] Frontend `npm run check` and `npm run test` pass.
- [ ] `docker compose config` validates.
- [ ] All new/changed behavior has tests.
- [ ] CHANGELOG.md updated.
- [ ] Docs updated and references fixed.
