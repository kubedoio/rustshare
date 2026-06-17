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
- Commits: `2adf79b4`, `652e3380`

## Known Gaps / Work Remaining

Task 6 fixes were committed, but the quality reviewer noted missing test coverage for the latest hardening additions. The next owner should add tests for:

- Redirect-based SSRF bypass (public IP redirecting to `127.0.0.1` is rejected by no-redirect client).
- IPv4-compatible IPv6 addresses (`::127.0.0.1`, `::10.0.0.1`, etc.).
- Unspecified addresses (`0.0.0.0`, `::`).
- `RUSTSHARE_WEBHOOK_MAX_AGE_SECONDS` validation (negative / zero rejected).

## Pending Tasks (Critical Findings Still Open)

The original review identified 18 critical/secondary findings. The following have not been started:

### 🔴 Task 7 — Upload service correctness
**Files:** `backend/crates/core/src/services/upload_service.rs`, `backend/server/src/handlers/upload.rs`
**Issues:**
- Upload completion materializes the whole file into memory (`assemble_chunks` builds a `Vec` sized by `total_size`). Should stream chunks directly to object store.
- Content-MD5 check is broken: handler sends MD5, service computes SHA-256, so all MD5 uploads fail.
- Public-share upload checks stale JWT permissions instead of querying the current share record.
- Upload completion does not re-verify folder write permission.
- Concurrent chunk uploads race on `(session_id, chunk_index)`; needs unique constraint or optimistic lock.

### 🔴 Task 8 — Permission resolver cache source and folder ancestry
**Files:** `backend/crates/core/src/services/permission_resolver.rs`
**Issues:**
- Cached permission results always return `PermissionSource::DirectShare`.
- Folder ancestry aggregation uses `.find` and picks an arbitrary user share instead of aggregating the highest permission across all shares.

### 🔴 Task 9 — Chat unfurl authorization
**File:** `backend/crates/core/src/services/chat_integration.rs`
**Issue:** Link unfurl authorizes by share creator (`share.created_by`). It should check `recipient_user_id` and `tenant_id` for private/user shares.

### 🔴 Task 10 — Password-protected share metadata leak
**Files:** `backend/server/src/handlers/public_shares.rs`, `backend/crates/core/src/services/share_service.rs`
**Issue:** `/api/v1/public/share/{token}/info` returns filename, size, and MIME without requiring the password.

### 🔴 Task 11 — Repository tenant_id filtering
**Files:** `backend/crates/infrastructure/src/repositories/*`
**Issue:** Some file/folder lookups filter by `owner_id` instead of `tenant_id`, breaking multi-tenant isolation.

### 🟡 Task 12 — Object store integrity and bucket creation
**Files:** `backend/crates/infrastructure/src/object_store.rs` (or equivalent)
**Issues:**
- Uploads/downloads lack integrity checksum verification against object store.
- Object store auto-creates bucket on startup; this should be configurable or removed.

### 🟡 Task 13 — Code cleanup
**Files:** Various handlers
**Issues:**
- Duplicated helper code and bare literals across handlers.
- Extract shared helpers and constants.

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
6. The most impactful remaining work is Task 7 (upload service) and Task 8 (permission resolver), both of which affect data integrity and security.
