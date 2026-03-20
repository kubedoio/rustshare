# Production Deployment

This file is now a short redirect document.

Older versions of this file described a historical deployment phase with a separate frontend runtime, JWT-centric browser auth, and an already-live production rollout. Those statements are no longer the repository source of truth.

## Use These Documents Instead

- Current project state: [STATUS.md](/Users/scolak/Projects/x/rustshare/STATUS.md)
- Frontend-specific status: [FRONTEND_STATUS.md](/Users/scolak/Projects/x/rustshare/FRONTEND_STATUS.md)
- Production hardening checklist: [PRODUCTION_READINESS.md](/Users/scolak/Projects/x/rustshare/PRODUCTION_READINESS.md)
- Backup and restore runbook: [docs/2026-03-20-backup-restore-runbook.md](/Users/scolak/Projects/x/rustshare/docs/2026-03-20-backup-restore-runbook.md)
- Restore drill checklist: [docs/2026-03-20-restore-drill-checklist.md](/Users/scolak/Projects/x/rustshare/docs/2026-03-20-restore-drill-checklist.md)
- Replication observability: [docs/2026-03-20-replication-observability.md](/Users/scolak/Projects/x/rustshare/docs/2026-03-20-replication-observability.md)
- Rate limiting and public-share protection: [docs/2026-03-20-rate-limit-hardening.md](/Users/scolak/Projects/x/rustshare/docs/2026-03-20-rate-limit-hardening.md)

## Current Deployment Model

The intended current deployment model is:

- Axum backend as the application runtime
- compiled SvelteKit SPA served by Axum
- API routes under `/api/...`
- WebSocket on `/api/ws`
- PostgreSQL for metadata
- RustFS-compatible object storage as primary file storage
- asynchronous background replication tracked in the database
- nginx or another reverse proxy in front when needed

## Current Reality

RustShare is not documented here as “already live in production.” The honest current position is:

- the web file-sharing product is late-MVP / pre-release and close to a careful launch
- the broader product, especially mobile sync/photos, is not complete
- operational tooling has improved materially, but real restore drills and deeper alerting still matter before strong production claims
