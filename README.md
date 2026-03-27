# RustShare

RustShare is a lightweight file-sharing and sync platform built with Rust and SvelteKit. The current product focus is a Nextcloud-lite / Seafile-lite experience: secure file and folder sharing, public links, internal collaboration, asynchronous replication, and a small operational surface.

## Table of Contents

- [Architecture Overview](#architecture-overview)
- [Current Maturity](#current-maturity)
- [Quick Start](#quick-start)
- [Local Development](#local-development)
- [Project Structure](#project-structure)
- [Validation](#validation)
- [Configuration](#configuration)
- [API Reference](#api-reference)
- [Operations](#operations)
- [Roadmap](#roadmap)
- [Documentation](#documentation)

---

## Architecture Overview

### High-Level Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        Clients                               │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐                  │
│  │ Web App  │  │ Mobile   │  │ Desktop  │                  │
│  │ (Browser)│  │ (Future) │  │ (Future) │                  │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘                  │
└───────┼─────────────┼─────────────┼────────────────────────┘
        │             │             │
        └─────────────┴─────────────┘
                      │
        ┌─────────────┴─────────────┐
        │                           │
┌───────▼────────┐        ┌─────────▼──────┐
│   Cloudflare   │        │   Axum Server  │
│   (CDN/WAF)    │        │   (Port 8080)  │
└───────┬────────┘        └───────┬────────┘
        │                         │
        └──────────┬──────────────┘
                   │
    ┌──────────────┼──────────────┐
    │              │              │
    │         ┌────▼────┐   ┌────▼─────┐
    │         │  RustFS  │   │  WebSocket │
    │         │ (S3 API) │   │  (/api/ws) │
    │         │ Files +  │   │            │
    │         │ Metadata │   │            │
    │         │(Canonical│   └──────────┘
    │         │  Store)  │
    │         └─────────┘
    │              ▲
    │              │ (Optional)
┌───▼───┐     ┌────┴────┐
│ Redis │     │  Local  │
│(Coord)│     │   FS    │
│       │     │(Stand-  │
│       │     │ alone)  │
└───────┘     └─────────┘
(Distributed) (Standalone)
```

### Technology Stack

| Layer | Technology | Version | Notes |
|-------|------------|---------|-------|
| **Backend** | Rust + Axum | 0.8.8 | Zero-PostgreSQL architecture |
| **Frontend** | SvelteKit 2 + Svelte 5 | Latest | SPA with Svelte 5 runes |
| **Metadata Store** | RustFS (S3-compatible) | Latest | **Canonical store** |
| **Coordination** | Redis (optional) | 7+ | Required for distributed mode |
| **Object Storage** | RustFS (S3-compatible) | Latest | File content storage |
| **Session Cache** | Memory or Redis | - | Ephemeral, reconstructible |
| **Real-time** | WebSocket | Native | Event-driven updates |
| **Runtime** | Docker + Docker Compose | 24+ | Standalone or distributed |

### Backend Architecture

The backend follows a **layered, multi-crate workspace** architecture:

```
backend/
├── server/              # Axum HTTP server (main binary)
│   ├── handlers/        # Request handlers (files, folders, shares, etc.)
│   ├── middleware/      # CSRF, rate limiting, IP extraction
│   ├── oidc.rs          # OIDC integration
│   ├── replication.rs   # Background replication worker
│   └── web_session.rs   # Session management
│
├── crates/
│   ├── core/            # Domain models, services (business logic)
│   ├── storage/         # Object storage abstraction
│   ├── auth/            # JWT, password hashing
│   ├── crypto/          # Encryption utilities
│   └── infrastructure/  # Repository implementations
│
└── migrations/          # SQLx database migrations
```

**Key Design Principles**:
- **Separation of Concerns**: Core has no I/O dependencies
- **Repository Pattern**: Trait-based data access for testability
- **Event-Driven**: WebSocket updates via event broadcaster
- **Type Safety**: SQLx compile-time query validation

### Frontend Architecture

```
frontend/src/lib/
├── components/          # Reusable UI components
│   ├── files/          # FileGrid, FileList, DropZone
│   ├── modals/         # CreateFolderModal, ShareModal
│   └── layout/         # Header, Sidebar, Breadcrumbs
├── stores/             # Svelte stores (auth, theme, selection)
├── api/                # API client functions
├── websocket/          # WebSocket client
└── files/              # File browser components
```

**State Management**:
- **Server State**: TanStack Query for caching and synchronization
- **Client State**: Svelte stores for UI state
- **Real-time**: WebSocket events invalidate queries automatically

### Authentication Flow

**Web (Cookie-Based)**:
```
┌─────────┐    Login      ┌─────────┐    Session    ┌──────────┐
│  User   │ ─────────────>│  Axum   │ ────────────> │   DB     │
└────┬────┘               └────┬────┘               └──────────┘
     │                         │
     │  HTTP-Only Cookie       │
     │<────────────────────────┘
     │  (rustshare.sid)
```

**Mobile (OIDC + JWT)**:
```
┌─────────┐   Auth Code + PKCE   ┌─────────┐
│  Mobile │ ────────────────────>│  Axum   │
└────┬────┘                      └────┬────┘
     │                                │
     │  JWT Token                     │
     │<───────────────────────────────┘
     │  (Authorization: Bearer)
```

### Real-Time Updates

Events flow through the system as follows:

```
┌──────────┐    Event     ┌──────────────┐    Broadcast    ┌──────────┐
│  Handler │ ────────────>│ EventBroadcaster│ ───────────> │ WebSocket│
└──────────┘              └──────────────┘                └────┬─────┘
                                                               │
                                    ┌──────────────────────────┼───────┐
                                    │                          │       │
                              ┌─────▼─────┐           ┌───────▼──┐ ┌──▼────┐
                              │  User A   │           │  User B  │ │User C │
                              └───────────┘           └──────────┘ └───────┘
```

**Event Types**: `FileUploaded`, `FileModified`, `FolderCreated`, `ShareRevoked`, `ReplicationStateChanged`

---

## Current Maturity

RustShare should currently be described as:

**Late MVP / pre-release**

The web file-sharing product is close to launchable with careful operational discipline. The broader product vision is not complete yet, especially around mobile sync/photos, desktop client maturity, and deeper production observability.

Current roadmap note:

- The aligned standalone mobile foundation exists, but active mobile product work is postponed while launch hardening remains the top priority
- The current Docker-based web pilot profile has now completed environment sign-off with a conditional Web-First Pilot Gate result

---

## Quick Start

### Prerequisites

- Docker 24+ and Docker Compose
- Rust toolchain (for local backend development)
- Node.js 20+ (for local frontend development)

### Start the Stack

```bash
# Clone the repository
git clone <repository-url>
cd rustshare

# Start all services
docker compose up -d

# Or start specific services
docker compose up -d backend postgres rustfs
```

### Check Health

```bash
# Backend health
curl http://localhost/health

# Full stack health
docker compose ps
```

### Access the App

| Service | URL |
|---------|-----|
| Web App | http://localhost |
| Backend API | http://localhost:8080 |
| Health Check | http://localhost:8080/health |
| PostgreSQL | localhost:5432 |
| RustFS Console | http://localhost:9001 |

### Default Local Accounts

| Role | Email | Password |
|------|-------|----------|
| Admin | `admin@localhost` | `admin123` |
| Viewer | `viewer@localhost` | `viewer123` |

---

## Local Development

### Backend Development

```bash
cd backend
cp .env.example .env
# Edit .env with your configuration

cargo run --bin rustshare-server

# Or with auto-reload (requires cargo-watch)
cargo watch -x 'run --bin rustshare-server'
```

### Frontend Development

```bash
cd frontend
npm install

# Development server with hot reload
npm run dev

# Type checking
npm run check

# Run tests
npm test
```

**Note**: In production-style Docker builds, the frontend is compiled into static assets and served by Axum. There is no separate Node.js frontend server in the shipped runtime.

Current note:

- The frontend dependency contract is now aligned on SvelteKit 2 + Svelte 5; refresh `frontend/node_modules` before the next frontend install/build cycle if you were working from an older checkout

---

## Project Structure

```
rustshare/
├── backend/                    # Rust backend
│   ├── server/                 # Axum HTTP server
│   ├── crates/                 # Workspace crates
│   │   ├── core/              # Domain + services
│   │   ├── storage/           # Object storage
│   │   ├── auth/              # JWT, sessions
│   │   ├── crypto/            # Encryption
│   │   └── infrastructure/    # Repositories
│   └── migrations/            # Database migrations
│
├── frontend/                   # SvelteKit frontend
│   ├── src/
│   │   ├── lib/              # Components, stores, API
│   │   └── routes/           # SvelteKit routes
│   └── static/               # Static assets
│
├── docs/                      # Documentation
│   ├── ADR.md                # Architecture decisions
│   ├── DEPENDENCY_MANAGEMENT.md
│   └── *.md                  # Various specs and runbooks
│
├── scripts/                   # Operational scripts
│   ├── check-dependencies.sh
│   └── run-restore-drill.sh
│
├── docker-compose.yml         # Main compose file
├── docker-compose.dev.yml     # Development overrides
└── README.md                  # This file
```

---

## Validation

### Backend

```bash
cd backend

# Type checking
cargo check --workspace

# Run all tests
cargo test --workspace

# Check formatting
cargo fmt -- --check

# Run clippy lints
cargo clippy --workspace -- -D warnings

# Check dependencies
cargo outdated -R

# Security audit
cargo audit
```

### Frontend

```bash
cd frontend

# Type checking
npm run check

# Run tests
npm test

# Linting
npm run lint

# Build check
npm run build
```

### Dependency Management

```bash
# Check all dependencies
./scripts/check-dependencies.sh

# Update all dependencies
./scripts/check-dependencies.sh --update
```

---

## Configuration

### Required Environment Variables

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

# Session / auth (change in production!)
JWT_SECRET=change-me-in-production

# Server
SERVER_HOST=0.0.0.0
SERVER_PORT=8080
```

### Optional Environment Variables

```bash
# Password login (disable for OIDC-only)
PASSWORD_LOGIN_ENABLED=true

# OIDC Configuration
OIDC_ISSUER_URL=
OIDC_CLIENT_ID=
OIDC_CLIENT_SECRET=
OIDC_REDIRECT_URL=
OIDC_LOGIN_LABEL=
OIDC_SCOPES=openid profile email

# Mobile OIDC (PKCE)
OIDC_MOBILE_CLIENT_ID=
OIDC_MOBILE_CLIENT_SECRET=
OIDC_MOBILE_REDIRECT_URIS=rustshare://auth/callback

# Rate Limits
RUSTSHARE_RATE_LIMIT_AUTH_LOGIN_PER_MINUTE=10
RUSTSHARE_RATE_LIMIT_OIDC_LOGIN_PER_MINUTE=30
RUSTSHARE_RATE_LIMIT_SHARE_SESSION_PER_MINUTE=5
RUSTSHARE_RATE_LIMIT_SHARE_INFO_PER_MINUTE=30
RUSTSHARE_RATE_LIMIT_SHARE_DOWNLOAD_PER_MINUTE=30
RUSTSHARE_RATE_LIMIT_SHARE_UPLOAD_PER_MINUTE=20

# Replication
REPLICATION_ENABLED=true
REPLICATION_BATCH_SIZE=8
REPLICATION_LEASE_TIMEOUT_SECONDS=120

# WebSocket
BROADCAST_CAPACITY=1000

# Metadata Backend (migration in progress)
RUSTSHARE_METADATA_BACKEND=postgres       # Options: postgres, rustfs, dual_write, rustfs_reads, localfs
RUSTSHARE_METADATA_CACHE=true             # Enable in-memory caching
RUSTSHARE_METADATA_PREFIX=apps/rustshare  # Object prefix in storage
RUSTSHARE_METADATA_NAMESPACE=default      # Namespace for multi-tenancy
```

### Metadata Backend Migration

The metadata system supports multiple backends with a phased migration approach:

| Backend | Description | Use Case |
|---------|-------------|----------|
| `postgres` | PostgreSQL only (legacy) | Current default, stable |
| `dual_write` | Write to both, read from PostgreSQL | Migration stage 1 |
| `rustfs_reads` | Write to both, read from RustFS | Migration stage 2 |
| `rustfs` | RustFS/S3 only (target) | Future state, horizontal scaling |
| `localfs` | Local filesystem | Development only |

**Migration stages:**
1. `postgres` → `dual_write` → Verify parity → `rustfs_reads` → Validate → `rustfs`

**Admin endpoints** (for verification/repair):
- `GET /api/admin/metadata/health` - Health check
- `GET /api/admin/metadata/stats` - Storage statistics
- `GET /api/admin/metadata/verify/*` - Verification tools
- `POST /api/admin/metadata/repair` - Repair inconsistencies

See [Metadata Refactor ADR](docs/2026-03-27-metadata-refactor-adr.md) for details.

Notes:

- The web app uses secure HTTP-only session cookies for its primary auth flow.
- `JWT_SECRET` still exists in configuration because some compatibility and non-browser paths still depend on token primitives internally.

---

## API Reference

### API Conventions

| Aspect | Convention |
|--------|------------|
| Base URL | `/api/v1/` |
| WebSocket | `/api/ws` |
| Content-Type | `application/json` |
| Auth (Web) | HTTP-only cookie (`rustshare.sid`) |
| Auth (Mobile) | `Authorization: Bearer <jwt>` |

### Key Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/api/v1/auth/login` | Login with email/password |
| `GET` | `/api/v1/me` | Get current user |
| `GET` | `/api/v1/files` | List files |
| `POST` | `/api/v1/files/upload` | Upload file |
| `GET` | `/api/v1/folders` | List folders |
| `POST` | `/api/v1/folders` | Create folder |
| `GET` | `/api/v1/folders/{id}/contents` | List folder contents |
| `GET` | `/api/v1/shares` | List shares |
| `POST` | `/api/v1/files/{id}/shares` | Create share |
| `GET` | `/api/ws` | WebSocket connection |

### Error Response Format

```json
{
  "error": "Human readable message",
  "details": "Optional additional context"
}
```

### Contract Freeze

- New client work should follow [API Contract Freeze](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-api-contract-freeze.md)
- `/api/v1/...` is the stable client surface
- `/api/ws` is the stable realtime endpoint

Mobile OIDC notes:

- Web login keeps using backend-issued HTTP-only cookies.
- Mobile login uses OIDC Authorization Code + PKCE against backend endpoints under `/api/v1/auth/oidc/mobile/*`.

---

## Operations

### Backup and Restore

```bash
# Create backup
./scripts/backup.sh

# Run restore drill
./scripts/run-restore-drill.sh backups/<timestamp>
```

### Health Monitoring

```bash
# Check replication status
curl http://localhost:8080/api/v1/admin/replication/summary

# Check system health
curl http://localhost/health
```

### Operational Documentation

| Document | Purpose |
|----------|---------|
| [Status](/Users/scolak/Projects/x/rustshare/STATUS.md) | Current project status |
| [Production Readiness](/Users/scolak/Projects/x/rustshare/PRODUCTION_READINESS.md) | Launch checklist |
| [Backup and Restore Runbook](/Users/scolak/Projects/x/rustshare/docs/2026-03-20-backup-restore-runbook.md) | DR procedures |
| [Replication Observability](/Users/scolak/Projects/x/rustshare/docs/2026-03-20-replication-observability.md) | Monitoring replication |
| [Rate Limit Hardening](/Users/scolak/Projects/x/rustshare/docs/2026-03-20-rate-limit-hardening.md) | Rate limit configuration |
| [Architecture Decisions](/Users/scolak/Projects/x/rustshare/docs/ADR.md) | ADR documentation |
| [Dependency Management](/Users/scolak/Projects/x/rustshare/docs/DEPENDENCY_MANAGEMENT.md) | Keeping deps current |

---

## Roadmap

### Completed Phases

- ✅ Phase 1: Foundation (core storage, auth)
- ✅ Phase 2: File Operations (CRUD, versioning)
- ✅ Phase 3a: User Sharing (internal shares, notifications)
- ✅ Phase 3b: Public Sharing (public links, file drops)
- ✅ Phase 4: Frontend Web App
- ✅ Phase 5: Launch Hardening
- ✅ Phase 6: Environment Sign-Off (Conditional Pass)

### Current Phase

**Phase 7: Post-Launch and Client Roadmap**

- Cleanup deferred items
- Evaluate mobile timeline
- Desktop client planning

### Named Remaining Roadmap Steps

- Launch Gate: Web-First Pilot (Conditional Pass achieved)
- Phase 7: post-launch cleanup and deferred client roadmap

Current gate note:

- The current Docker-based password-login pilot profile has a **Conditional Pass**
- Broader production claims still require real IdP validation and real monitoring/alert wiring

---

## Documentation

### Architecture

- **[Architecture Decision Records](docs/ADR.md)** - Complete architecture documentation
- **[API Contract Freeze](docs/2026-03-21-api-contract-freeze.md)** - API stability guarantees
- **[Compatibility Surface](docs/2026-03-21-compatibility-surface.md)** - Public interface inventory

### Operations

- **[Dependency Management](docs/DEPENDENCY_MANAGEMENT.md)** - Keeping dependencies current
- **[Backup/Restore Runbook](docs/2026-03-20-backup-restore-runbook.md)** - Disaster recovery
- **[Replication Observability](docs/2026-03-20-replication-observability.md)** - Monitoring guide

### Specifications

- **[Mobile OIDC Contract](docs/2026-03-20-mobile-oidc-contract.md)** - Mobile auth spec
- **[Rate Limit Hardening](docs/2026-03-20-rate-limit-hardening.md)** - Rate limiting design
- **[Phase 5 Launch Hardening](docs/2026-03-21-phase-5-launch-hardening-spec.md)** - Launch prep

### Decisions

- **[Mobile Postponement Decision](docs/2026-03-21-mobile-postponement-decision.md)**
- **[Web-First Pilot Gate Decision](docs/2026-03-21-web-first-pilot-gate-decision.md)**

---

## Scope Discipline

RustShare is intentionally not trying to be full Nextcloud. Current scope is:

- ✅ Files and folders
- ✅ Sharing and permissions
- ✅ Public links and file drops
- ✅ Realtime updates
- ✅ Operational durability through async replication and recovery tooling

It is not currently targeting:

- ❌ Full desktop sync (like Dropbox)
- ❌ Real-time document collaboration
- ❌ Calendar/Contacts
- ❌ Plugin ecosystem

---

*Last Updated: 2026-03-26*
