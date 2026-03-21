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

The web file-sharing product is close to launchable with careful operational discipline. The broader product vision is not complete yet, especially around mobile sync/photos and deeper production observability.

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

Examples:

- `GET /api/v1/me`
- `POST /api/auth/login`
- `POST /api/v1/auth/oidc/mobile/authorize`
- `POST /api/v1/auth/oidc/mobile/exchange`
- `GET /api/v1/shares`
- `GET /api/v1/admin/replication/summary`
- `GET /api/ws`

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

Restore-drill helper:

```bash
scripts/run-restore-drill.sh backups/<timestamp>
```

## Known Remaining Gaps

- Light mobile client with photo backup and offline flows
- Stronger anonymous/public upload attribution in audits
- Real restore-drill execution against a production-like backup set
- Deeper alerting and metrics beyond current summaries and helper scripts

## Scope Discipline

RustShare is intentionally not trying to be full Nextcloud. Current scope is:

- files and folders
- sharing and permissions
- public links and file drops
- realtime updates
- operational durability through async replication and recovery tooling

It is not currently targeting full desktop sync, document collaboration, or a broad plugin ecosystem.
