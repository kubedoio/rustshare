# Production Readiness

This document is the current production-hardening checklist for RustShare. It replaces older notes that described a separate frontend runtime, JWT-only browser auth, or a fully finished production rollout.

## Current Position

RustShare is close to production-ready for a careful, file-sharing-focused launch, but it is not “done everywhere.”

Current confidence by area:

- Web file-sharing product: high
- Core auth/session/runtime model: high
- Async replication foundation: medium-high
- Operator recovery/runbooks: medium-high
- Mobile sync/photos product: not ready
- Deep alerting / long-term observability: partial but specified

## Runtime Checklist

### Architecture

- [x] All backend routes live under `/api/...`
- [x] Primary versioned API routes exist under `/api/v1/...`
- [x] Axum serves the compiled SvelteKit SPA for non-API routes
- [x] WebSocket endpoint is available on `/api/ws`
- [x] The production-style runtime does not require a separate Node.js frontend server

### Authentication and Sessions

- [x] Primary web auth uses secure HTTP-only cookie sessions
- [x] Session records are persisted server-side
- [x] CSRF protection is enforced for cookie-authenticated browser mutations
- [x] OIDC login flow groundwork exists
- [x] Password login can be enabled or disabled by configuration
- [ ] OIDC production rollout has been exercised against the chosen identity provider

### File Sharing Product

- [x] File and folder CRUD
- [x] Upload, download, move, rename, delete, restore, and version history
- [x] Internal user-to-user sharing
- [x] Public file links
- [x] Public folder links
- [x] Upload-only public folder links
- [x] Shared-with-me views
- [x] Notification inbox and unread badge
- [x] Realtime user-visible events over WebSocket
- [x] First-class markdown notes with editor, autosave, and public sharing

## Storage and Replication Checklist

- [x] Primary file writes go to RustFS-compatible object storage
- [x] Upload success is decoupled from cross-node replication
- [x] Replication state is tracked in the database
- [x] Replication worker processes queued jobs asynchronously
- [x] Retry and degraded/failure states are tracked
- [x] Replication summary and target-health operator endpoints exist
- [ ] End-to-end degraded-replication incident drills have been run on real infrastructure
- [ ] Alerting is wired to replication-health thresholds

## Security Checklist

- [x] Passwords use Argon2id hashing
- [x] Share links support password protection and expiry
- [x] Rate limiting is enabled for high-risk auth and public-share routes
- [x] Reverse-proxy deployment path is documented
- [x] Session cookies are HttpOnly and server-managed
- [x] Browser auth does not rely on localStorage JWTs
- [ ] HTTPS/TLS termination must be configured for production
- [ ] Secrets must be rotated out of default local-development values
- [ ] External security review / penetration testing has not been completed yet

## Recovery Checklist

- [x] Backup bundle script exists
- [x] Restore script exists
- [x] Backup bundle verification script exists
- [x] Post-restore smoke script exists
- [x] Isolated restore-drill runner exists
- [x] Backup and restore runbook exists
- [x] Restore drill checklist exists
- [ ] A real restore drill against an actual backup artifact should still be performed and recorded
- [x] RPO/RTO targets formally defined in backup-restore runbook (< 24h RPO, < 2h RTO)

## Observability Checklist

- [x] Health endpoints exist
- [x] Readiness endpoint exists
- [x] Replication summary endpoint exists
- [x] Replication target-health endpoint exists
- [x] CLI replication health helper exists
- [x] Alerting thresholds and Prometheus rules documented
- [x] Error tracking / incident paging documented with PagerDuty/OpsGenie examples
- [ ] Centralized metrics dashboards are still partial (Grafana setup required)

### Operational Readiness Endpoints

RustShare exposes two distinct health endpoints so operators can separate **process liveness** from **dependency readiness**.

#### `GET /health` — Liveness Probe

Lightweight. Returns `200 OK` when the Axum process is running.

```json
{
  "status": "ok"
}
```

Use this for Kubernetes liveness probes or load-balancer health checks that only need to know whether the binary has started.

#### `GET /health/ready` — Readiness Probe

Returns `200 OK` when all **required** runtime dependencies are healthy. Returns `503 Service Unavailable` when any required dependency is unhealthy.

Response body (stable, machine-readable JSON):

```json
{
  "status": "ready",
  "components": {
    "database": { "status": "healthy" },
    "object_storage": { "status": "healthy" },
    "event_delivery": { "status": "healthy" },
    "auth_session": { "status": "healthy" },
    "ai": { "status": "disabled" }
  }
}
```

Component definitions:

| Component | Required? | Meaning |
|-----------|-----------|---------|
| `database` | **Yes** | Metadata projection database is reachable and queryable. |
| `object_storage` | **Yes** | S3/RustFS bucket is accessible. |
| `event_delivery` | **Yes** | Event-store database is reachable **and** the in-memory broadcaster channel is open. |
| `auth_session` | **Yes** | JWT manager can round-trip a token **and** the `user_sessions` table is queryable. |
| `ai` | No | AI / semantic-search service is present. Reports `disabled` when `RUSTSHARE_AI_ENABLED` is off or the service is not initialized; a disabled AI component **does not** fail readiness. |

If a component is unhealthy, the response includes an `error` field:

```json
{
  "status": "not_ready",
  "components": {
    "object_storage": {
      "status": "unhealthy",
      "error": "object storage check failed: connection refused"
    }
  }
}
```

Use `/health/ready` for Kubernetes readiness probes or load-balancer membership decisions.

Reference planning docs:

- [OIDC Production Validation Checklist](2026-03-21-oidc-production-validation-checklist.md)
- [Alerting And Incident Thresholds](2026-03-21-alerting-and-incident-thresholds.md)
- [Backup and Restore Runbook](2026-03-20-backup-restore-runbook.md)
- [Backup and Restore Guide](backup-restore.md)
- [Deployment Guide](DEPLOYMENT.md)

## Deployment Checklist

### Before launch

- [ ] Replace local-development secrets
- [ ] Configure production OIDC values if SSO is required at launch
- [ ] Configure TLS and production reverse proxy settings
- [ ] Validate backup, restore, verify, and smoke scripts in the target environment
- [ ] Run current validation suite
- [ ] Verify critical user journeys: login, upload, internal share, public link, upload-only link, restore, and replication recovery

### Recommended validations

```bash
cd backend
cargo check --workspace
cargo test --workspace

cd frontend
npm run check
npm test

cd ..
docker compose config
```

## Honest Remaining Risks

- Mobile client work is still outstanding, so the broader product vision is not yet launch-ready.
- Anonymous/public uploads still need stronger attribution semantics in audit trails.
- Some older repository docs outside the current runbook set may still reflect previous architecture phases.
- Replication observability is meaningfully better than before, but not yet at a full “mature ops dashboard + automated alerts” level.

## Launch Recommendation

Reasonable recommendation today:

**Proceed only with a careful web-first launch or pilot, not with a broad “finished platform” claim.**

That means:

- launch the web file-sharing product first
- keep scope narrow
- verify backups/restores on real infrastructure
- treat mobile as the next product phase, not part of the current completion claim
