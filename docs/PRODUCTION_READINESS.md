# Production Readiness

> **Status:** Production-readiness gap closure complete — Workstreams A–F  
> **Branch:** `production-readiness-gap-closure`  
> **Last updated:** 2026-06-17

This document summarizes what is production-ready, what remains experimental, and what operators must do before running RustShare in production. It reflects the work completed in the `production-readiness-gap-closure` branch.

---

## 1. What Is Production-Ready

| Area | Confidence | Notes |
|------|------------|-------|
| Web file-sharing core (upload, download, share, folder CRUD) | High | Streaming upload/download for large objects; size limits enforced. |
| Authentication & sessions | High | Secure cookies default to `Secure`; admin routes require `AdminUser`; session revocation APIs exist. |
| Multi-tenant isolation | High | Repository-level `tenant_id` filtering for files, folders, shares, notifications, vaults, and share links; `X-Tenant-ID` support for anonymous public routes. |
| Webhook security | High | HMAC-SHA256 signature verification; HTTPS-only webhook registration. |
| CI/CD & secrets hygiene | High | Hardcoded secrets removed from workflows; per-run generated secrets; `secret-scan` gate. |
| Code quality & test coverage | High | Ignored backend tests fixed or removed; clippy clean across all targets; cargo audit advisories addressed. |
| Backup, restore, and recovery | High | Bundled scripts for backup, restore, verification, and isolated restore drills; runbooks exist. |
| Request observability | Medium-high | Request-scoped correlation IDs (`X-Request-ID`) propagated through tracing spans. |

### Production-Ready Features

- File and folder CRUD, move, rename, delete, restore, and version history.
- Internal user-to-user sharing and group sharing.
- Public file/folder share links with optional passwords and expiry.
- Upload-only public folder links.
- Real-time WebSocket events.
- Markdown notes with editor, autosave, and public sharing.
- Notification inbox.
- Async replication foundation with operator health/summary endpoints.
- Prometheus `/metrics` endpoint with optional bearer-token protection.
- Health (`/health`) and readiness (`/health/ready`) probes.

---

## 2. What Is Still Experimental

| Area | Status | Guidance |
|------|--------|----------|
| Mobile clients | Not ready | Do not include in a production launch claim. |
| Desktop app (`apps/desktop/`) | Early prototype | Not production-ready. |
| Zero-PostgreSQL / RustFS metadata backend | Migration roadmap | `postgres` is the supported production backend. Stages `dual_write`, `rustfs_reads`, and `rustfs` are migration/experimental. |
| Deep observability dashboards | Partial | Prometheus metrics and documented thresholds exist, but curated Grafana dashboards are not shipped. |
| OIDC production validation | Partial | Implemented and tested locally; validate end-to-end with your chosen IdP before relying on it. |
| Virus scanning | Out of scope | Integrate a post-upload scanner via external hooks if required. |

---

## 3. Workstreams A–E Summary

| Workstream | Focus | Key Deliverables |
|------------|-------|------------------|
| **A — Security Hardening** | Auth, injection, secret handling | Chat webhook HMAC-SHA256 signature verification; HTTPS-only webhook registration; `Content-Disposition` control-character sanitization; secure session cookie defaults (`Secure=true` by default); `AdminUser` extractor enforced on all admin routes; bootstrap admin password written to a secure file, never logged. |
| **B — Multi-Tenant Isolation** | Cross-tenant access boundaries | `tenant_id` filtering added to repository queries for files, folders, shares, notifications, vaults, and permission resolver; public share token resolution tenant-scoped via `X-Tenant-ID` header and share-session JWT claims; no-op RLS middleware removed. |
| **C — Large-Object Streaming** | Memory-safe transfers | `ObjectStore::get_stream` for streaming downloads; multipart uploads streamed to temporary files then to object storage; automatic temp-file cleanup; upload size limits aligned (`MAX_UPLOAD_SIZE_MB`, `MAX_PUBLIC_UPLOAD_SIZE`, `MAX_CHUNK_SIZE`); low-memory integration tests. |
| **D — CI/CD & Deployment Hardening** | Secret hygiene in automation | Hardcoded secrets removed from GitHub Actions; per-run generated secrets via `openssl rand`; `secret-scan` job in CI and pre-commit; `docs/CI_SECRETS.md` and `docs/DEPLOYMENT.md` updated with required secrets and rotation guidance. |
| **E — Code Quality & Test Gaps** | Reliability and coverage | Ignored backend tests re-enabled, fixed, or removed with justification; clippy clean across all targets; cargo audit advisories addressed; request-scoped correlation IDs with validation and tests. |

---

## 4. Residual Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| No external penetration test completed | Unknown exploitable issues | Dependency auditing (`cargo audit`, `cargo deny`), secret scanning, clippy `-D warnings`, contract tests, and code review. Schedule an external pentest before a broad launch. |
| Password login does not scope by tenant | Cross-tenant email collisions could misroute login | Documented residual risk; email uniqueness across tenants is currently an operational assumption. Future work will add explicit tenant scoping to login. |
| RLS middleware removed | One less defense-in-depth layer | Repository-level tenant filtering is the active control and is tested by contract tests. RLS may be reintroduced only with connection pinning or `before_acquire` `SET` semantics. |
| OIDC not validated against every target IdP | SSO failures in production | Follow the [OIDC Production Validation Checklist](2026-03-21-oidc-production-validation-checklist.md) with your IdP before launch. |
| Replication health alerting not wired to a pager | Degraded replication may go unnoticed | Operator endpoints and CLI health checks exist; documented thresholds in [Alerting And Incident Thresholds](2026-03-21-alerting-and-incident-thresholds.md). Wire Prometheus alerts to your paging stack. |
| Centralized Grafana dashboards absent | Slower incident response | Use `/metrics`, `/health/ready`, and application logs until dashboards are added. |
| Mobile/desktop clients unfinished | Product scope mismatch | Launch the web product only; treat mobile/desktop as a later phase. |
| User-facing trash/restore absent | Accidental deletions require backup restore | Documented recovery via restore drill in [Backup/Restore Runbook](runbooks/backup-restore.md). |

---

## 5. Operator Checklist

### 5.1 Secrets

- [ ] Run `./scripts/pre-flight.sh` to generate strong production secrets.
- [ ] Replace every placeholder in `.env` before starting the stack.
- [ ] Store `.env` in a secrets manager or encrypted vault; never commit it.
- [ ] Rotate the following on a schedule or after any suspected compromise:
  - `JWT_SECRET` — invalidates existing sessions; plan a maintenance window.
  - `RUSTSHARE_SECRET_ENCRYPTION_KEY` — requires re-encryption of existing data; back up the old key until re-encryption is complete.
  - `POSTGRES_PASSWORD` — update `DATABASE_URL` and restart.
  - `RUSTFS_ROOT_PASSWORD` / `STORAGE_ACCESS_KEY` / `STORAGE_SECRET_KEY` — rotate together and update S3 clients.
  - `OIDC_CLIENT_SECRET` — follow your IdP's rotation policy.
  - `RUSTSHARE_CHAT_WEBHOOK_SECRET` — rotate and re-register webhooks.
  - `METRICS_API_TOKEN` — rotate if `/metrics` is exposed.
- [ ] Disable dev-only overrides such as `RUSTSHARE_ALLOW_HTTP_WEBHOOKS` and `RUSTSHARE_METADATA_BACKEND=localfs` in production.

See [Deployment Guide](DEPLOYMENT.md) and [Security Incident Runbook](runbooks/security-incident.md) for rotation procedures.

### 5.2 Backups

- [ ] Enable daily automated backups:
  ```bash
  0 2 * * * cd /opt/rustshare && ./scripts/backup-stack.sh /mnt/backups/rustshare >> /var/log/rustshare-backup.log 2>&1
  ```
- [ ] Verify every backup with `./scripts/verify-backup-bundle.sh <backup-dir>`.
- [ ] Run a restore drill at least monthly:
  ```bash
  ./scripts/run-restore-drill.sh /mnt/backups/rustshare/<latest>
  ```
- [ ] Replicate weekly backups off-site (S3, rsync, tape).
- [ ] Define and enforce a retention policy (daily 7 days, weekly 4 weeks, monthly 12 months is a sensible default).

See [Backup/Restore Runbook](runbooks/backup-restore.md).

### 5.3 Monitoring

- [ ] Configure liveness probe on `/health`.
- [ ] Configure readiness probe on `/health/ready`.
- [ ] Scrape Prometheus metrics from `/metrics` (use `METRICS_API_TOKEN` if exposed).
- [ ] Aggregate application logs (Docker logging driver → Loki/CloudWatch/ELK).
- [ ] Alert on:
  - Auth failure spikes (possible brute force).
  - Replication failure / target-unhealthy states.
  - Object storage unreachable.
  - Database connection errors.
  - 5xx rate increases.
- [ ] Review [Alerting And Incident Thresholds](2026-03-21-alerting-and-incident-thresholds.md).

### 5.4 Upgrades

- [ ] Read the `[Unreleased]` section of `CHANGELOG.md` before upgrading.
- [ ] Take a backup before any upgrade.
- [ ] Test the upgrade in an isolated restore-drill environment first.
- [ ] Run `./scripts/final-launch-smoke.sh` after the upgrade.
- [ ] Have a rollback plan: keep the previous Docker image tag and a known-good backup.

### 5.5 Pre-Launch Validation

- [ ] Run `SQLX_OFFLINE=true cargo check --workspace` and `cargo test --workspace`.
- [ ] Run `cargo clippy --all-targets --all-features -- -D warnings`.
- [ ] Run `cargo audit` and `cargo deny check`.
- [ ] Run frontend `npm run check` and `npm run test`.
- [ ] Validate `docker compose config`.
- [ ] Verify critical user journeys: login, upload, internal share, public link, upload-only link, restore, replication recovery.

---

## 6. Launch Recommendation

**Proceed with a careful, web-first launch or pilot.** RustShare's core file-sharing runtime, tenant isolation, security controls, and operational recovery story are production-grade for a self-hosted deployment. Do not market mobile, desktop, or a "finished platform" claim until those workstreams are completed and validated.

---

## See Also

- [Security Model](security-model.md)
- [System Architecture](architecture.md)
- [Deployment Guide](DEPLOYMENT.md)
- [CI/CD Secrets Reference](CI_SECRETS.md)
- [Backup/Restore Runbook](runbooks/backup-restore.md)
- [Security Incident Runbook](runbooks/security-incident.md)
- [Alerting And Incident Thresholds](2026-03-21-alerting-and-incident-thresholds.md)
- [OIDC Production Validation Checklist](2026-03-21-oidc-production-validation-checklist.md)
