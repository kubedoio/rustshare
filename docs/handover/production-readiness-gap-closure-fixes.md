# Handover: Production-Readiness Findings Fixes

## Branch

- **Local branch:** `production-readiness-gap-closure-fixes`
- **Upstream branch:** `origin/production-readiness-gap-closure-fixes`
- **Worktree:** `.worktrees/production-readiness-gap-closure`
- **Based on:** `production-readiness-gap-closure` (PR #114)

## Goal

Fix the critical security, correctness, and compatibility findings identified in the pre-landing review of `production-readiness-gap-closure` (Workstreams A–F). The branch already contains the original workstream implementation; this handover tracks the remediation work.

## Completed Work

### ✅ Task 1 — Share session JWT compatibility
- Added `#[serde(default)]` to `ShareSessionClaims::tenant_id` so legacy share-session JWTs missing the field deserialize to `Uuid::nil()` instead of failing.
- Added unit tests for missing-field default, round-trip, serialization, and expired-token rejection.
- Commit: `c14a7956`

### ✅ Task 2 — Public share endpoint tenant header handling
- Made `X-Tenant-ID` optional for public share endpoints (`/api/v1/public/share/{token}/session`, `/info`, folder contents, folder file download, folder upload).
- Added unscoped share-token lookup in `MetadataStore` (`get_share_by_token_unscoped`) because `shares.share_token` is globally unique.
- When `X-Tenant-ID` is omitted, tenant is derived from the share; when provided, it is verified against the share's tenant.
- Legacy share-session JWTs with `tenant_id == Uuid::nil()` resolve the effective tenant from the share.
- Added unit tests for missing/matching/mismatched tenant on session creation, share info, and folder contents; plus handler-level extractor tests.
- Commit: `05128687`

### ✅ Task 3 — API version bump
- Bumped OpenAPI spec version from `1.0.0` to `2.0.0` in `backend/server/src/openapi.rs`.
- Regenerated `docs/contracts/rustshare-api-openapi.json`.
- Added `openapi_spec_version_is_2_0_0` integration test.
- Updated `CHANGELOG.md`.
- Commits: `4480d2e9`, `e87ee638`

### ✅ Task 4 — Webhook payload compatibility check
- Investigated: the payload shape was always `IncomingChatEvent`; the apparent change was the addition of raw-body HMAC signature verification (Workstream A security hardening). No payload-shape fix required.

### ✅ Task 5 — Tenant-scope password login + email index
- Added optional `tenant_id` to `LoginRequest` with `#[serde(default)]`.
- Added tenant-scoped, case-insensitive lookup `find_user_by_email_and_tenant` to `MetadataStore`.
- Added `count_users_by_email` for unscoped ambiguity detection.
- `validate_credentials` uses scoped lookup when `tenant_id` is provided; when omitted, it falls back to unscoped lookup but rejects ambiguous emails (same email in multiple tenants) to preserve isolation.
- Scoped `ensure_optional_seed_user` to `default_tenant_id`.
- Added migration `20260617160001_add_users_email_lower_index.sql`:
  - Drops global `users_email_key` constraint.
  - Adds per-tenant unique index `users_email_tenant_id_key ON users(LOWER(email), tenant_id)`.
  - Documents data-risk assumptions for existing duplicates.
- Added tests for tenant-scoped, backward-compatible, wrong-tenant, ambiguous-email, and case-insensitive login.
- Commit: `c58c6c26`

### ✅ Task 6 — Webhook SSRF protection and replay-age check
- Moved SSRF URL validator to shared core crate (`rustshare_core::services::validate_chat_webhook_url`).
- Rejects localhost, loopback, private IPv4, link-local, multicast, CGNAT (`100.64.0.0/10`), unique-local IPv6, IPv4-mapped/compatible IPv6, and unspecified addresses.
- Added 5-second DNS resolution timeout.
- Re-runs validation in `HttpWebhookDispatcher::dispatch` to mitigate DNS rebinding.
- Disabled reqwest redirect following for webhook dispatches.
- Changed `ChatIntegrationError::InvalidWebhookUrl` to an opaque unit variant to prevent information leakage.
- Added replay-age check with configurable `RUSTSHARE_WEBHOOK_MAX_AGE_SECONDS` (default 300s); rejects future and stale timestamps as `SignatureVerificationFailed`.
- Documented env var in `backend/.env.example`, `docs/security-model.md`, and `CHANGELOG.md`.
- Added coverage for no-redirect webhook client behavior, IPv4-compatible IPv6 addresses, unspecified addresses, and invalid `RUSTSHARE_WEBHOOK_MAX_AGE_SECONDS` values.
- Commits: `2adf79b4`, `652e3380`

### ✅ Task 7 — Upload service correctness
- Replaced resumable upload completion's full-file `Vec` assembly with object-store adapter streaming through a temporary file while computing the final SHA-256.
- Changed final chunk assembly to write once to the content-addressed `blobs/{sha256}` key.
- Fixed `Content-MD5` handling in the upload handler so MD5 headers are verified as MD5 before the SHA-256 chunk path runs.
- Re-verified current folder write permission before authenticated resumable upload completion.
- Changed public folder upload authorization to use the current share record's permissions instead of stale share-session JWT permissions.
- Added conditional chunk object writes and conditional chunk-info writes for duplicate `(session_id, chunk_index)` protection.
- Merged upload-session chunk bitmasks on repository updates so concurrent uploads of different chunks do not drop progress.
- Added targeted MD5 handler tests.

### ✅ Task 9 — Chat unfurl authorization
- Changed private user-share unfurl authorization to require the requesting user to match `recipient_user_id` on a share scoped to the requesting tenant.
- Added tests proving non-recipients are denied and recipients are allowed.

### ✅ Task 10 — Password-protected share metadata leak
- Stopped `get_public_share_info` from loading file/folder metadata for password-protected shares before session creation.
- Public share `/info` now returns a generic protected-share response for protected links with no filename, folder name, size, or MIME type.
- Added regression coverage for protected public share info.

### ✅ Task 8 — Permission resolver cache source and folder ancestry
- Changed the permission resolver cache to store full `PermissionResult` values instead of permissions only.
- Source-aware permission resolution now preserves cached `Owner`, `DirectShare`, `GroupShare`, `Inherited`, and `None` sources with share IDs where applicable.
- Folder ancestry aggregation now selects the highest active user share instead of the first matching share.
- Added regression coverage for cached group-share sources and highest inherited user-share selection.

### ✅ Task 11 — Repository tenant_id filtering
- Verified infrastructure file and folder repositories already scope `get_by_id` queries by `tenant_id`.
- Verified `PermissionResolverRepository` delegates file/folder lookups through those tenant-scoped repository methods.
- Added explicit wrong-tenant regression assertions to the infrastructure file and folder repository tests.

### ✅ Task 12 — Object store integrity and bucket creation
- Added SHA-256 verification for content-addressed `blobs/{sha256}` object uploads from memory and paths.
- Added SHA-256 verification for `get` downloads and streamed `get_stream` downloads, with stream integrity mismatches reported after EOF.
- Made startup bucket creation explicit through `RUSTSHARE_OBJECT_STORE_AUTO_CREATE_BUCKET`; default is disabled for production safety.
- Kept Docker Compose local bootstrap opted in by default.
- Added object-store unit tests for matching/mismatched blob hashes and streamed mismatch reporting.

### ✅ Task 13 — Code cleanup
- Centralized object-store upload request construction for normal and conditional writes.
- Replaced repeated object-store literals with named constants.

## Known Gaps / Work Remaining

## Pending Tasks (Critical Findings Still Open)

The original review identified 18 critical/secondary findings. Tasks 1-13 from this remediation handover are complete.

## Tooling / Verification Commands

```bash
cd /Users/scolak/Projects/x/rustshare/.worktrees/production-readiness-gap-closure/backend

# Format / lint
cargo fmt --check
SQLX_OFFLINE=true cargo clippy --all-targets --all-features -- -D warnings

# Check (offline SQLx)
SQLX_OFFLINE=true cargo check --workspace

# Lib tests (need DATABASE_URL for auth handler tests)
SQLX_OFFLINE=true cargo test --workspace --lib -- --skip login_timing_attack_resistance

# If DATABASE_URL is available:
DATABASE_URL="postgresql:///rustshare_test?host=/tmp&user=scolak" cargo test --workspace --lib -- --skip login_timing_attack_resistance

# OpenAPI freshness
RUSTSHARE_UPDATE_OPENAPI=1 cargo test --test openapi_export_test -p rustshare-server
```

## Test Database Setup

```bash
createdb rustshare_test_fixes
DATABASE_URL="postgresql:///rustshare_test_fixes?host=/tmp&user=scolak" cargo sqlx migrate run
```

## Notes for the Next Owner

1. The branch uses the `.worktrees/production-readiness-gap-closure` worktree. Any new work should happen there.
2. All commits must include a DCO sign-off (`git commit -s`).
3. Run `cargo fmt --check` and clippy before each commit.
4. The original PR is #114 (`production-readiness-gap-closure`). This new branch is for the remediation fixes and can be merged back into the original PR branch or reviewed separately.
5. Task 6 has uncommitted hardening fixes already applied and committed as `652e3380`; add the missing tests before considering it fully done.
6. Tasks 1-13 in this handover are complete; continue with any new PR review feedback or CI findings.
