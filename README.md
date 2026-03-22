# RustShare

RustShare is a lightweight file-sharing and sync platform built with Rust and SvelteKit. The current product focus is a Nextcloud-lite / Seafile-lite experience: secure file and folder sharing, public links, internal collaboration, asynchronous replication, and a small operational surface.

## Current Architecture

- Backend: Rust + Axum
- Frontend: SvelteKit SPA built with `@sveltejs/adapter-static`
- Runtime: Axum serves the compiled SPA and all `/api/...` routes
- Auth: secure HTTP-only cookie sessions for the web app, with OIDC groundwork in place
- Realtime: WebSocket updates on `/api/ws`
- Database: PostgreSQL
- Primary file storage: RustFS-compatible object storage (`rustfs` service in Docker Compose)
- Replication: strictly asynchronous background replication tracked in the database

## What Works Today

- File and folder CRUD
- Upload, download, move, rename, delete, restore, and version history
- Internal user-to-user sharing
- Public file links
- Public folder links
- Upload-only public folder links
- Realtime notifications and replication-state updates over WebSocket
- Notification inbox with unread counts
- Shared-with-me flows with dedicated shared resource routes
- Replication worker, target-health summaries, and operator helper scripts
- Backup, restore, backup verification, and post-restore smoke scripts
- Docker-based local and production-style deployment

## Current Maturity

RustShare should currently be described as:

**Late MVP / pre-release**

The web file-sharing product is close to launchable with careful operational discipline. The broader product vision is not complete yet, especially around mobile sync/photos, desktop client maturity, and deeper production observability.

Current roadmap note:

- the aligned standalone mobile foundation exists, but active mobile product work is postponed while launch hardening remains the top priority
- the current Docker-based web pilot profile has now completed environment sign-off with a conditional Web-First Pilot Gate result

## Quick Start

### Prerequisites

- Docker and Docker Compose
- Rust toolchain for local backend development
- Node.js for local frontend development

### Start the stack

```bash
docker compose up -d
```

### Check health

```bash
curl http://localhost/health
```

### Access the app

- App: `http://localhost`
- Backend health: `http://localhost:8080/health`
- PostgreSQL: `localhost:5432`
- Object storage console: `http://localhost:9001`

### Default local accounts

- Email: `admin@localhost`
- Password: `admin123`
- Viewer email: `viewer@localhost`
- Viewer password: `viewer123`

## Local Development

### Backend

```bash
cd backend
cp .env.example .env
cargo run --bin rustshare-server
```

### Frontend

```bash
cd frontend
npm install
npm run dev
```

In production-style Docker builds, the frontend is compiled into static assets and served by Axum. There is no separate Node.js frontend server in the shipped runtime.

Current note:

- the frontend dependency contract is now aligned on SvelteKit 2 + Svelte 5; refresh `frontend/node_modules` before the next frontend install/build cycle if you were working from an older checkout

## Validation

Common validation commands:

```bash
cd backend
cargo check --workspace
cargo test --workspace

cd ../frontend
npm run check
npm test
```

## Key Environment Variables

Backend and Compose expect environment variables like these:

```bash
# Database
DATABASE_URL=postgres://rustshare:changeme@localhost:5432/rustshare

# RustFS-compatible storage
RUSTFS_ENDPOINT=http://localhost:9000
RUSTFS_PUBLIC_ENDPOINT=http://localhost:9000
RUSTFS_BUCKET=rustshare-files
RUSTFS_REGION=us-east-1
AWS_ACCESS_KEY_ID=rustfsadmin
AWS_SECRET_ACCESS_KEY=rustfsadmin

# Session / auth
JWT_SECRET=change-me-in-production
PASSWORD_LOGIN_ENABLED=true

# Optional OIDC
OIDC_ISSUER_URL=
OIDC_CLIENT_ID=
OIDC_CLIENT_SECRET=
OIDC_REDIRECT_URL=
OIDC_LOGIN_LABEL=
OIDC_SCOPES=openid profile email

# Optional mobile OIDC / PKCE
OIDC_MOBILE_CLIENT_ID=
OIDC_MOBILE_CLIENT_SECRET=
OIDC_MOBILE_REDIRECT_URIS=rustshare://auth/callback

# Rate limits
RUSTSHARE_RATE_LIMIT_AUTH_LOGIN_PER_MINUTE=10
RUSTSHARE_RATE_LIMIT_OIDC_LOGIN_PER_MINUTE=30
RUSTSHARE_RATE_LIMIT_SHARE_SESSION_PER_MINUTE=5
RUSTSHARE_RATE_LIMIT_SHARE_INFO_PER_MINUTE=30
RUSTSHARE_RATE_LIMIT_SHARE_DOWNLOAD_PER_MINUTE=30
RUSTSHARE_RATE_LIMIT_SHARE_UPLOAD_PER_MINUTE=20
RUSTSHARE_RATE_LIMIT_AUTHENTICATED_SHARE_ADMIN_PER_MINUTE=120
```

Notes:

- The web app uses secure HTTP-only session cookies for its primary auth flow.
- `JWT_SECRET` still exists in configuration because some compatibility and non-browser paths still depend on token primitives internally.

## API Shape

Current route conventions:

- API routes: `/api/...`
- Primary versioned routes: `/api/v1/...`
- WebSocket: `/api/ws`
- Legacy unversioned aliases still exist for compatibility, but new integrations should use `/api/v1/...`

Examples:

- `GET /api/v1/me`
- `POST /api/v1/auth/login`
- `POST /api/v1/auth/oidc/mobile/authorize`
- `POST /api/v1/auth/oidc/mobile/exchange`
- `GET /api/v1/shares`
- `GET /api/v1/admin/replication/summary`
- `GET /api/ws`

Contract freeze:

- New client work should follow [API Contract Freeze](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-api-contract-freeze.md)
- `/api/v1/...` is the stable client surface
- `/api/ws` is the stable realtime endpoint
- `/api/v1/ws` and `/api/sync` have been removed
- legacy `/api/auth/...` aliases have been removed
- unversioned file, folder, share, notification, and public-share aliases have been removed

Mobile OIDC notes:

- Web login keeps using backend-issued HTTP-only cookies.
- Mobile login uses OIDC Authorization Code + PKCE against backend endpoints under `/api/v1/auth/oidc/mobile/*`.
- The mobile app sends its `redirect_uri`, `code_challenge`, `state`, and `nonce` to `POST /api/v1/auth/oidc/mobile/authorize`.
- The backend returns the provider authorization URL after validating the redirect URI against `OIDC_MOBILE_REDIRECT_URIS`.
- After the provider redirects back to the mobile app, the app exchanges `code + code_verifier + nonce` through `POST /api/v1/auth/oidc/mobile/exchange`.
- The backend validates the ID token and returns a Rustshare bearer token for mobile API access.

## Operations

Current operator docs live here:

- [Status](/Users/scolak/Projects/x/rustshare/STATUS.md)
- [Mobile OIDC Contract](/Users/scolak/Projects/x/rustshare/docs/2026-03-20-mobile-oidc-contract.md)
- [Frontend Status](/Users/scolak/Projects/x/rustshare/FRONTEND_STATUS.md)
- [Production Readiness](/Users/scolak/Projects/x/rustshare/PRODUCTION_READINESS.md)
- [Backup and Restore Runbook](/Users/scolak/Projects/x/rustshare/docs/2026-03-20-backup-restore-runbook.md)
- [Restore Drill Checklist](/Users/scolak/Projects/x/rustshare/docs/2026-03-20-restore-drill-checklist.md)
- [Replication Observability](/Users/scolak/Projects/x/rustshare/docs/2026-03-20-replication-observability.md)
- [Rate Limit Hardening](/Users/scolak/Projects/x/rustshare/docs/2026-03-20-rate-limit-hardening.md)
- [API Contract Freeze](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-api-contract-freeze.md)
- [Client Integration Checklist](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-client-integration-checklist.md)
- [Compatibility Surface Inventory](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-compatibility-surface.md)
- [Mobile Postponement Decision](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-mobile-postponement-decision.md)
- [Phase 5 Launch Hardening Spec](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-phase-5-launch-hardening-spec.md)
- [Phase 6 Environment Sign-Off Spec](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-phase-6-environment-signoff-spec.md)
- [Launch Gate: Web-First Pilot](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-launch-gate-web-first-pilot.md)
- [Phase 7 Post-Launch And Client Roadmap](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-phase-7-post-launch-and-client-roadmap.md)
- [Phase 6 Execution Report](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-phase-6-execution-report.md)
- [Web-First Pilot Gate Decision](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-web-first-pilot-gate-decision.md)

Restore-drill helper:

```bash
scripts/run-restore-drill.sh backups/<timestamp>
```

## Named Remaining Roadmap Steps

- Phase 6: environment sign-off
- Launch Gate: Web-First Pilot
- Phase 7: post-launch cleanup and deferred client roadmap

Current gate note:

- the current Docker-based password-login pilot profile has a **Conditional Pass**
- broader production claims still require real IdP validation and real monitoring/alert wiring

## Scope Discipline

RustShare is intentionally not trying to be full Nextcloud. Current scope is:

- files and folders
- sharing and permissions
- public links and file drops
- realtime updates
- operational durability through async replication and recovery tooling

It is not currently targeting full desktop sync, document collaboration, or a broad plugin ecosystem.
