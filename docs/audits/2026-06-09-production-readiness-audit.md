# RustShare Production Readiness Audit

**Date:** 2026-06-09
**Auditor:** Kimi Code CLI
**Scope:** Backend server, crates, infrastructure, tests, documentation, operational readiness
**Commit:** `main` branch (post-OpenAPI merge)

---

## Executive Summary

RustShare is a well-architected file-sharing and sync platform with strong foundational security patterns (Argon2id, AES-256-GCM secret encryption, rate limiting, proxy-aware IP extraction). However, **several critical gaps remain before the software can be considered production-grade**. The most urgent issues are a **login timing attack vulnerability**, **upload endpoints buffering entire files into memory**, **unconfigured database connection pooling**, **missing graceful shutdown**, **no metrics/observability**, and **unbounded data retention**.

| Area | Rating | Key Risk |
|------|--------|----------|
| Security (AuthZ/Secrets) | NEEDS_IMPROVEMENT | Plaintext bootstrap password logged; login timing attack; hardcoded webhook fallback |
| Error Handling & Logging | NEEDS_IMPROVEMENT | No JSON logging, no request spans/IDs, PII in logs, no panic hook |
| Configuration Management | NEEDS_IMPROVEMENT | No centralized config struct, no hot-reload, weak defaults |
| Database / SQLx | CRITICAL | Connection pool untuned (default 10 conns); event store/projection desync risk; no retry logic |
| API Handlers / Middleware | CRITICAL | Uploads buffer entire files in memory; unbounded list endpoints; no CORS |
| Dependency Management | CRITICAL | rustls-webpki RUSTSEC-2026-0098/0104/0099; RSA Marvin Attack; duplicate crate bloat |
| Tests & Coverage | NEEDS_IMPROVEMENT | No coverage tooling; weak DB isolation; unimplemented test server |
| Documentation | NEEDS_IMPROVEMENT | Broken doc references; incomplete OpenAPI |
| Operational Readiness | CRITICAL | No graceful shutdown; no metrics; unbounded data retention; disk space crisis |
| Automated Checks | NEEDS_IMPROVEMENT | Clippy errors in tests; cargo deny warnings; cargo audit advisories |

---

## Critical Issues (Fix Before Production)

### 1. Login Timing Attack (User Enumeration)
**File:** `backend/server/src/handlers/auth.rs` (validate_credentials)
**Severity:** CRITICAL

When an email is not found, the handler returns after `find_user_by_email` + `record_login_failure`. When the email exists but the password is wrong, it performs an **Argon2id verification** (computationally expensive) + `record_login_failure`. The timing difference is measurable and allows attackers to enumerate registered email addresses.

**Fix:** Always perform a dummy Argon2 verify (against a static dummy hash) when the user is not found, so both code paths take similar time.

### 2. File Uploads Buffered Entirely in Memory
**Files:** `backend/server/src/handlers/files.rs:56`, `upload.rs:248`, `public_shares.rs:544`
**Severity:** CRITICAL

`upload_file` uses `field.bytes().await` which buffers the **entire file into RAM**. With `DefaultBodyLimit::disable()` on the route and a 2GB global limit, a few concurrent large uploads will OOM the server. Public share and vault sync uploads have the same problem.

**Fix:** Stream multipart data in chunks to temporary files on disk, then stream from disk to object storage.

### 3. Database Connection Pool Untuned
**File:** `backend/server/src/bootstrap.rs:68-79`
**Severity:** CRITICAL

`PgPoolOptions::new()` is used with **zero production tuning**. sqlx defaults to `max_connections=10`. Under production load this will exhaust immediately. No `acquire_timeout`, `idle_timeout`, or `max_lifetime` is set.

**Fix:** Add `max_connections`, `acquire_timeout`, `idle_timeout`, `max_lifetime`. Expose via env vars (`DB_POOL_MAX_CONNECTIONS`, etc.).

### 4. Event Store / Projection Desync Risk
**File:** `backend/crates/core/src/services/file_service.rs:453-462`
**Severity:** CRITICAL

Events are appended to the event store *before* the projection table is updated. If the server crashes between these two independent queries, the event log and projection tables become permanently inconsistent. No outbox pattern or shared transaction is used.

**Fix:** Wrap event append + projection update in a single database transaction, or implement an Outbox pattern with a background reconciler.

### 5. Missing Graceful Shutdown
**File:** `backend/server/src/main.rs:136-141`
**Severity:** CRITICAL

`axum::serve(...).await?` has **no SIGTERM/SIGINT handling**. Background workers (replication, trash cleanup, WebSocket rooms) are spawned as detached `tokio::spawn` tasks with no shutdown signal. Kubernetes rolling updates will terminate connections mid-request and orphan background jobs.

**Fix:** Use `tokio::signal` + `axum::serve(...).with_graceful_shutdown(...)`. Propagate shutdown to all background workers via `tokio::sync::broadcast` or `CancellationToken`.

### 6. No Metrics / Observability Endpoint
**Severity:** CRITICAL

There is no `/metrics` endpoint (Prometheus), no OpenTelemetry, no request latency histograms, no error-rate counters. The only observability is unstructured `tracing` text logs.

**Fix:** Add `metrics-rs` + `metrics-exporter-prometheus`. Instrument key handlers with `histogram!` for latency and `counter!` for errors.

### 7. rustls-webpki Security Advisories
**Severity:** CRITICAL

`cargo audit` reports:
- `RUSTSEC-2026-0098`: Name constraints for URI names incorrectly accepted
- `RUSTSEC-2026-0104`: Reachable panic in CRL parsing
- `RUSTSEC-2026-0099`: Name constraints accepted for wildcard certificates

All affect `rustls-webpki 0.101.7` via `rustls 0.21.12` → `hyper-rustls 0.24.2` → `aws-smithy-http-client`.

**Fix:** Upgrade AWS SDK dependencies or force `rustls-webpki >= 0.103.12` via `[patch.crates-io]` or `cargo update`.

### 8. Unbounded Data Retention
**Severity:** CRITICAL

No cleanup is implemented for:
- Audit / security events (`user_security_events`)
- Share access logs
- Replication job history
- Old file versions (accumulate indefinitely)
- Expired user sessions
- Expired OIDC login states
- Expired device pair requests
- Expired share links
- Webhook delivery logs

**Fix:** Implement background cleanup workers for each table with configurable retention periods.

### 9. Disk Space Exhaustion (Build Artifacts)
**Severity:** CRITICAL (Operational)

The build server ran out of disk space during this audit (`/dev/sdc 100%`). `target/` directories consumed ~95 GB. This indicates either very large debug builds or accumulation of old artifacts. CI/build machines must have adequate space and cleanup policies.

**Fix:** Add `cargo clean` to CI post-build, use `CARGO_TARGET_DIR` with rotation, or enable `sccache` with size limits.

---

## High Priority Issues

### 10. Plaintext Bootstrap Admin Password Logged
**File:** `backend/server/src/bootstrap.rs:512-517`
**Severity:** HIGH

The auto-generated bootstrap admin password is logged in **plaintext** to the console at startup. This will be captured by container logs, systemd journals, and log aggregation systems.

**Fix:** Remove the password from logs entirely. If it must be displayed, write it to a secure file with restricted permissions (`0600`) and log only the file path.

### 11. Hardcoded Chat Webhook Secret Fallback
**File:** `backend/server/src/bootstrap.rs:379`
**Severity:** HIGH

`RUSTSHARE_CHAT_WEBHOOK_SECRET` falls back to `"change-me-in-production"`.

**Fix:** Remove the fallback. Require the env var or fail startup with a clear error.

### 12. JWT Missing Issuer/Audience Validation
**File:** `backend/crates/auth/src/jwt.rs:66,79`
**Severity:** HIGH

`Validation::default()` is used, which does NOT validate the `iss` claim, even though tokens set `iss: "rustshare"`. No `aud` claim exists. No refresh token mechanism means 24-hour hardcoded expiry forces re-authentication.

**Fix:** Add `iss` validation, add `aud` claim, make expiry configurable, implement refresh tokens.

### 13. Readiness Probe Exposes Internal Errors
**File:** `backend/server/src/handlers/health.rs:78,87,101,103,147,157`
**Severity:** HIGH

The readiness endpoint returns raw `sqlx::Error` and S3 error strings in the JSON response body, potentially leaking connection details, hostnames, or credentials.

**Fix:** Return generic status messages ("database unavailable", "storage unavailable") and log the detailed error server-side only.

### 14. No CORS Configuration
**File:** `backend/server/src/main.rs:73-127`
**Severity:** HIGH

No CORS middleware is configured. While the SPA is served same-origin, mobile clients and third-party integrations will be blocked.

**Fix:** Add `tower-http::cors` middleware with configurable allowed origins.

### 15. Pagination Missing on Key List Endpoints
**Files:** `files.rs:771`, `files.rs:877`, `files.rs:983`, and many module list handlers
**Severity:** HIGH

`list_files`, `list_starred_items`, `list_deleted_items`, `list_notes`, `list_decisions`, `list_meetings`, etc. return **unbounded result sets**.

**Fix:** Add pagination (limit/offset or cursor-based) to all list endpoints.

### 16. `record_login_failure` Race Condition
**File:** `backend/crates/storage/src/metadata.rs:697-771`
**Severity:** HIGH

Reads security config, checks existing row, then updates/inserts in three separate round-trips **without a transaction**. Concurrent failed logins for the same IP can lose counts or skip blocking.

**Fix:** Wrap in a database transaction.

### 17. Background Blob Cleanup Ignores Errors
**File:** `backend/server/src/handlers/admin/users.rs:647-651`
**Severity:** HIGH

Deletion failures are silently dropped (`let _ = object_store.delete(&key).await;`). This can leave orphaned storage objects.

**Fix:** Log errors and implement a retry/dead-letter mechanism.

---

## Medium Priority Issues

### 18. CSRF Protection Weak
**File:** `backend/server/src/middleware/csrf.rs:14-41`
**Severity:** MEDIUM

CSRF check is a static header `X-Rustshare-Csrf: 1`, not a token-based double-submit cookie. Provides no defense against same-origin attacks.

**Fix:** Implement a proper double-submit cookie pattern with a cryptographically random token.

### 19. No Centralized Config Struct
**File:** `backend/server/src/bootstrap.rs` (scattered `std::env::var`)
**Severity:** MEDIUM

Configuration is loaded ad-hoc via `std::env::var` throughout `bootstrap.rs`. No `envy`/`config` crate usage. No aggregated "missing config" report.

**Fix:** Define a `Config` struct with `serde` + `envy`, validate all fields at startup, and print a single error report.

### 20. unwrap() in Production Storage Code
**File:** `backend/crates/storage/src/metadata.rs:187,652`
**Severity:** MEDIUM

`serde_json::to_value(...).unwrap()` can panic. While infallible in practice for these types, it should use `?` or `map_err`.

### 21. PII in Logs
**Files:** `auth.rs:334`, `rate_limit.rs:197-200`
**Severity:** MEDIUM

Seed user creation logs username and email. Rate limit logs log full client IPs.

**Fix:** Redact or hash PII in production logs.

### 22. Session Cookie Secure Default
**File:** `backend/crates/auth/src/web_session.rs:119`
**Severity:** MEDIUM

`SESSION_COOKIE_SECURE` defaults to `false`, meaning cookies are not marked `Secure` by default.

**Fix:** Default to `true` and require explicit opt-out.

### 23. Incomplete OpenAPI Coverage
**File:** `backend/server/src/openapi.rs:21-254`
**Severity:** MEDIUM

Only ~20 paths are documented. Many stable `/api/v1/` routes are missing.

**Fix:** Add all stable routes and schemas to the `#[openapi(...)]` macro.

### 24. Missing Referenced Documentation
**File:** `docs/PRODUCTION_READINESS.md`
**Severity:** MEDIUM

References 8+ documents that do not exist (alerting thresholds, backup runbook, OIDC validation checklist, etc.).

**Fix:** Create the missing documents or remove broken references.

### 25. Duplicate Dependencies
**Severity:** MEDIUM

`cargo tree -d` shows many duplicates: `aws-smithy-http`, `base64`, `elliptic-curve`, `p256`, `ring`, `rustls`, `idna`. Increases binary size and compile times.

**Fix:** Align dependency versions via workspace deps or `[patch.crates-io]`.

### 26. Unused Dependencies
**File:** `backend/server/Cargo.toml:48,63`
**Severity:** LOW

`nonzero_ext` and `yrs` are declared but not used anywhere in backend source.

**Fix:** Remove unused dependencies.

### 27. Feature Flag Bloat
**File:** `/srv/data02/projects/rustshare/Cargo.toml:30`
**Severity:** LOW

`tokio = { features = ["full"] }` pulls many unneeded features. `sqlx` has `"migrate"` enabled globally.

**Fix:** Narrow `tokio` features. Move `sqlx migrate` to a build/dev dependency.

---

## Automated Check Results

| Check | Status | Notes |
|-------|--------|-------|
| `cargo fmt --check` | PASS | No formatting issues |
| `cargo clippy --workspace --lib --bins` | FAIL (1 error) | `unused import: get` in `main.rs:51` |
| `cargo clippy --all-targets --all-features` | FAIL | Multiple test errors (unused vars, bool_assert_comparison, items_after_test_module) |
| `cargo audit` | FAIL | RUSTSEC-2026-0098, RUSTSEC-2026-0104, RUSTSEC-2026-0099 (rustls-webpki), RUSTSEC-2023-0071 (rsa), RUSTSEC-2026-0173 (proc-macro-error2 unmaintained) |
| `cargo deny check` | WARN | Multiple duplicate crate warnings, license-not-encountered warnings |
| `cargo test --workspace` | FAIL | Linker bus error (OOM during compilation) + clippy errors in test code |
| Disk space | CRITICAL | 100% full during audit; cleaned 95 GB from `target/` |

---

## Recommendations by Timeline

### Before Production Launch (Week 1-2)
1. Fix login timing attack (dummy hash for missing users).
2. Stream file uploads to disk instead of buffering in memory.
3. Tune database connection pool with env-var-exposed settings.
4. Wrap event append + projection updates in transactions.
5. Implement graceful shutdown with `tokio::signal`.
6. Add `/metrics` endpoint (Prometheus) for RED metrics.
7. Patch `rustls-webpki` advisories.
8. Implement data retention cleanup workers.
9. Remove plaintext bootstrap password from logs.
10. Remove hardcoded chat webhook secret fallback.

### Near-Term (Month 1)
11. Add `iss`/`aud` validation to JWT; implement refresh tokens.
12. Sanitize readiness probe error messages.
13. Add CORS middleware.
14. Add pagination to all unbounded list endpoints.
15. Fix `record_login_failure` race condition with transactions.
16. Add request-scoped tracing with correlation IDs.
17. Switch to JSON logging in production.
18. Add code coverage reporting to CI (`cargo llvm-cov`).
19. Complete OpenAPI path documentation.
20. Create missing operational runbooks.

### Ongoing
21. Audit and remove unused dependencies.
22. Narrow `tokio` and `sqlx` feature flags.
23. Implement load/stress tests for upload/download.
24. Add database transaction isolation for integration tests.
25. Implement `setup_test_server()` for HTTP-level e2e tests.
26. Define and document RPO/RTO targets; run a real restore drill.
27. Add alerting webhooks (PagerDuty/OpsGenie) to health endpoints.
28. Implement proper double-submit CSRF tokens.
29. Add a centralized `Config` struct with validation.
30. Monitor and control build artifact disk usage.

---

## Positive Findings (What Works Well)

- **Strong authentication extractors**: `AuthenticatedUser` and `AdminUser` extractors properly validate sessions, JWTs, and disabled status per-request.
- **Password hashing**: Argon2id with secure random salts via `SaltString::generate(&mut OsRng)`.
- **Rate limiting**: Per-IP token-bucket with proxy-aware client IP extraction and anti-spoofing.
- **Secret encryption**: OIDC/SMTP secrets encrypted at rest with AES-256-GCM.
- **Weak default rejection**: Known weak JWT secrets and encryption keys are blocked at startup.
- **SQL injection prevention**: Consistent use of parameterized queries via `sqlx::query!`.
- **Health probes**: Comprehensive readiness checks for DB, storage, events, auth, and AI.
- **Backup/restore tooling**: Excellent scripts (`backup-stack.sh`, `restore-stack.sh`, `pre-flight.sh`, `final-launch-smoke.sh`).
- **Docker hardening**: Multi-stage builds, non-root user, healthchecks, resource limits in compose.
- **CI/CD**: Multi-arch builds, SBOM generation, artifact attestation, Trivy scanning.
- **ADR culture**: 30+ architecture decision records show thoughtful design history.
