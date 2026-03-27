# Architecture Decision Records (ADR)

This document contains the key architectural decisions made in RustShare, organized by layer and concern.

## Overview

RustShare is a file-sharing and synchronization platform built with:
- **Backend**: Rust (Axum web framework)
- **Frontend**: SvelteKit 2 with Svelte 5
- **Database**: PostgreSQL
- **Storage**: RustFS-compatible S3 object storage
- **Real-time**: WebSocket for live updates

---

## 1. System Architecture

### 1.1 High-Level Architecture

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
┌─────────────────────┼─────────────────────────────────────┐
│                     ▼                                      │
│  ┌─────────────────────────────────────────────────────┐  │
│  │              RustShare Server (Axum)                 │  │
│  │  ┌───────────────────────────────────────────────┐  │  │
│  │  │  Static SPA Assets (SvelteKit)                │  │  │
│  │  └───────────────────────────────────────────────┘  │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌────────────┐  │  │
│  │  │ API Routes  │  │ WebSocket   │  │ Public     │  │  │
│  │  │ /api/v1/... │  │ /api/ws     │  │ Shares     │  │  │
│  │  └─────────────┘  └─────────────┘  └────────────┘  │  │
│  └─────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────┘
                      │
        ┌─────────────┼─────────────┐
        ▼             ▼             ▼
┌──────────────┐ ┌─────────┐ ┌─────────────┐
│  PostgreSQL  │ │ RustFS  │ │   Redis     │
│  (Metadata)  │ │(Objects)│ │  (Future)   │
└──────────────┘ └─────────┘ └─────────────┘
```

### 1.2 Backend Crate Structure

```
backend/
├── Cargo.toml              # Workspace configuration
├── server/                 # Axum HTTP server (main binary)
│   ├── src/
│   │   ├── main.rs         # Server initialization, routing
│   │   ├── handlers/       # HTTP request handlers
│   │   │   ├── mod.rs      # Handler exports, error responses
│   │   │   ├── files.rs    # File operations
│   │   │   ├── folders.rs  # Folder operations
│   │   │   ├── shares.rs   # Public share management
│   │   │   ├── user_shares.rs  # Internal user sharing
│   │   │   ├── public_shares.rs # Public share access
│   │   │   ├── sync.rs     # WebSocket sync handler
│   │   │   ├── users.rs    # User profile, sessions
│   │   │   ├── admin/      # Admin panel handlers
│   │   │   └── ...
│   │   ├── middleware/     # Axum middleware
│   │   │   ├── csrf.rs     # CSRF protection
│   │   │   ├── rate_limit.rs  # Rate limiting
│   │   │   └── client_ip.rs   # IP extraction
│   │   ├── oidc.rs         # OIDC integration
│   │   ├── replication.rs  # Replication worker
│   │   └── web_session.rs  # Session management
│   └── Cargo.toml
├── crates/
│   ├── core/               # Domain models, services
│   │   ├── src/
│   │   │   ├── domain/     # Entities: File, Folder, User, Share
│   │   │   ├── services/   # Business logic: FileService, FolderService
│   │   │   └── events/     # Event broadcaster for real-time updates
│   │   └── Cargo.toml
│   ├── storage/            # Object storage abstraction
│   │   ├── src/
│   │   │   ├── object_store.rs  # S3/RustFS client
│   │   │   ├── metadata.rs      # Metadata operations
│   │   │   └── event_store.rs   # Event persistence
│   │   └── Cargo.toml
│   ├── auth/               # JWT, password hashing, sessions
│   │   ├── src/
│   │   │   ├── jwt.rs
│   │   │   ├── session.rs
│   │   │   └── lib.rs
│   │   └── Cargo.toml
│   ├── crypto/             # Encryption utilities
│   │   ├── src/
│   │   │   ├── secret_encryption.rs
│   │   │   └── password.rs
│   │   └── Cargo.toml
│   └── infrastructure/     # Repository implementations
│       ├── src/
│       │   └── repositories/
│       │       ├── file_repository.rs
│       │       ├── folder_repository.rs
│       │       ├── share_repository.rs
│       │       └── user_repository.rs
│       └── Cargo.toml
└── migrations/             # SQLx database migrations
```

**Decision**: Multi-crate workspace with clear separation of concerns:
- `core`: Business logic, domain models (pure Rust, no I/O)
- `storage`: Persistence abstraction (S3, database)
- `auth`: Authentication primitives (reusable)
- `crypto`: Cryptographic utilities (reusable)
- `infrastructure`: Concrete repository implementations
- `server`: HTTP layer, composes everything together

---

## 2. Technology Stack Decisions

### 2.1 Backend Framework: Axum 0.8

**Status**: ✅ Active (Updated 2026-03-26)

**Decision**: Use Axum as the web framework.

**Rationale**:
- Native async/await support (no `async_trait` needed since 0.8)
- Excellent Tower ecosystem integration
- Type-safe request handlers with extractors
- Built-in WebSocket support
- Strong middleware composition

**Migration History**:
- Started with Axum 0.7
- Migrated to Axum 0.8 (2026-03-26)
- Changes required: `{id}` route syntax, `async_trait` removal, WebSocket `Utf8Bytes`

### 2.2 Frontend Framework: SvelteKit 2 + Svelte 5

**Status**: ✅ Active

**Decision**: Use SvelteKit 2 with Svelte 5 runes.

**Rationale**:
- Compiled output (no virtual DOM overhead)
- Excellent TypeScript integration
- Built-in static adapter for SPA deployment
- Runes API for fine-grained reactivity

**Build Output**: Static SPA served by Axum (no Node.js runtime in production)

### 2.3 Database: PostgreSQL + SQLx

**Status**: ✅ Active

**Decision**: Use PostgreSQL with SQLx for type-safe queries.

**Rationale**:
- Compile-time checked SQL queries
- No ORM abstraction leakage
- Full PostgreSQL feature access
- Migration support via `sqlx migrate`

**Schema**: See `backend/migrations/` for full history

### 2.4 Object Storage: RustFS (S3-compatible)

**Status**: ✅ Active

**Decision**: Use S3-compatible object storage (RustFS) for file content.

**Rationale**:
- Decouples file content from database
- Supports replication via async background jobs
- Compatible with AWS S3, MinIO, etc.

**Configuration**:
```bash
RUSTFS_ENDPOINT=http://localhost:9000
RUSTFS_BUCKET=rustshare-files
RUSTFS_REGION=us-east-1
```

### 2.5 Real-time Updates: WebSocket + Event Broadcaster

**Status**: ✅ Active

**Decision**: Use WebSocket with a custom event broadcaster for real-time updates.

**Architecture**:
- Events published to `EventBroadcaster` (in-memory channel)
- WebSocket subscribers receive filtered events
- Event persistence in PostgreSQL for catch-up

**Event Types**:
- `FileUploaded`, `FileModified`, `FileDeleted`
- `FolderCreated`, `FolderRenamed`, `FolderDeleted`
- `ShareCreated`, `ShareRevoked`
- `ReplicationStateChanged`

---

## 3. Authentication & Authorization

### 3.1 Session Management

**Decision**: HTTP-only secure cookies for web, JWT tokens for mobile.

**Web Flow**:
1. User logs in with password or OIDC
2. Server creates session in `user_sessions` table
3. Session ID in HTTP-only cookie (`rustshare.sid`)
4. CSRF token required for state-changing operations

**Mobile Flow**:
1. OIDC Authorization Code + PKCE
2. Backend validates, issues JWT
3. JWT in `Authorization: Bearer` header

### 3.2 OIDC Integration

**Status**: ✅ Implemented, needs production validation

**Decision**: Support OIDC for enterprise integration.

**Configuration**:
```bash
OIDC_ISSUER_URL=https://accounts.google.com
OIDC_CLIENT_ID=...
OIDC_CLIENT_SECRET=...
OIDC_REDIRECT_URL=https://app.rustshare.io/auth/oidc/callback
```

**Mobile OIDC**: Separate client registration with PKCE

### 3.3 Authorization Model

**Decision**: Role-based + Resource-level permissions.

**Roles**:
- `admin`: Full system access
- `user`: Standard user (own files + shared with them)

**Share Permissions**:
- `View`: Read-only
- `Download`: Can download files
- `Upload`: Can upload to folders
- `Admin`: Full control

---

## 4. Data Layer

### 4.1 Domain Models

**Core Entities**:
- `User`: Account, authentication, quotas
- `Folder`: Hierarchical organization
- `File`: Metadata, versioning
- `FileVersion`: Historical versions
- `Share`: Public and internal sharing
- `UserSession`: Active sessions
- `Notification`: User notifications

### 4.2 Repository Pattern

**Decision**: Use repository pattern with trait interfaces.

**Structure**:
```rust
// In core: trait definition
pub trait FileRepository {
    async fn get_file(&self, id: Uuid) -> Result<File, FileError>;
    async fn save_file(&self, file: &File) -> Result<(), FileError>;
}

// In infrastructure: concrete implementation
pub struct PgFileRepository { ... }
impl FileRepository for PgFileRepository { ... }
```

**Benefits**:
- Swappable implementations (test doubles)
- Core crate has no database dependencies
- Easier testing

### 4.3 Event Sourcing (Lightweight)

**Decision**: Store events for audit and real-time sync, not full event sourcing.

**Event Table**:
```sql
CREATE TABLE events (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL,
    event_type TEXT NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

**Usage**:
- Real-time sync over WebSocket
- Audit logging
- Replication coordination

---

## 5. Replication Architecture

### 5.1 Asynchronous Replication

**Status**: ✅ Implemented

**Decision**: Strictly asynchronous background replication.

**Flow**:
1. File uploaded to primary storage
2. Metadata saved to PostgreSQL
3. Replication job queued (`replication_jobs` table)
4. Background worker processes jobs
5. Target health tracked, failures retried

**Benefits**:
- No upload latency increase
- Resilient to target failures
- Observable queue depth

### 5.2 Replication Worker

**Implementation**: Background Tokio task

**Configuration**:
```bash
REPLICATION_BATCH_SIZE=8
REPLICATION_LEASE_TIMEOUT_SECONDS=120
REPLICATION_POLL_INTERVAL_MS=5000
```

---

## 6. Deployment & Operations

### 6.1 Container Strategy

**Decision**: Docker Compose for local/single-node, Kubernetes-ready.

**Services**:
- `backend`: Axum server
- `frontend`: SvelteKit (built into backend image)
- `postgres`: PostgreSQL
- `rustfs`: S3-compatible object storage

### 6.2 Configuration Management

**Decision**: Environment variables only, no config files.

**Priority**:
1. Environment variables
2. `.env` file (local development)
3. Defaults (development only)

### 6.3 Health Checks

**Endpoints**:
- `GET /health`: Basic liveness
- `GET /api/v1/admin/replication/summary`: Replication status

### 6.4 Rate Limiting

**Decision**: Token bucket per IP + per-user for authenticated routes.

**Implementation**: Custom middleware with `governor` crate

**Configurable Limits**:
```bash
RUSTSHARE_RATE_LIMIT_AUTH_LOGIN_PER_MINUTE=10
RUSTSHARE_RATE_LIMIT_SHARE_DOWNLOAD_PER_MINUTE=30
```

---

## 7. Security Decisions

### 7.1 Password Storage

**Decision**: Argon2id with OWASP recommended parameters.

**Implementation**: `rustshare_auth::PasswordHasher`

### 7.2 CSRF Protection

**Decision**: Double-submit cookie pattern.

- Session cookie: HTTP-only, Secure, SameSite=Strict
- CSRF header: `X-Rustshare-Csrf: 1`

### 7.3 File Upload Security

**Decisions**:
- File type validation by magic bytes (not just extension)
- Size limits (500MB default)
- Virus scanning (future: ClamAV integration)

### 7.4 Share Token Security

**Decision**: JWT for share tokens with expiration.

**Structure**:
```json
{
  "sub": "share-id",
  "exp": 1234567890,
  "type": "public_file"
}
```

---

## 8. API Design

### 8.1 Versioning Strategy

**Decision**: URI path versioning (`/api/v1/...`).

**Current Surface**: `/api/v1/...` is stable
**Legacy**: `/api/...` unversioned routes removed 2026-03-21

### 8.2 Error Handling

**Format**:
```json
{
  "error": "Human readable message",
  "details": "Optional additional context"
}
```

**Status Codes**:
- `400`: Bad Request (validation errors)
- `401`: Unauthorized (not authenticated)
- `403`: Forbidden (no permission)
- `404`: Not Found
- `409`: Conflict (duplicate name, version conflict)
- `422`: Unprocessable Entity
- `500`: Internal Server Error

### 8.3 Pagination

**Decision**: Cursor-based for events, offset-based for lists.

**Offset Example**:
```
GET /api/v1/notifications?limit=20&offset=40
```

**Cursor Example**:
```
GET /api/v1/events?limit=100&after_id=uuid
```

---

## 9. Frontend Architecture

### 9.1 State Management

**Decision**: Svelte stores + TanStack Query.

**Pattern**:
- Server state: TanStack Query (`createQuery`, `createMutation`)
- Client state: Svelte writable/readable stores

### 9.2 Component Structure

```
frontend/src/lib/
├── components/       # Reusable UI components
│   ├── files/       # FileGrid, FileList, etc.
│   ├── modals/      # CreateFolderModal, ShareModal, etc.
│   ├── layout/      # Header, Sidebar, etc.
│   └── common/      # Toast, ThemeToggle, etc.
├── files/           # File browser specific components
├── stores/          # Svelte stores (auth, theme, selection)
├── api/             # API client functions
├── websocket/       # WebSocket client and event handling
└── utils/           # Helper functions
```

### 9.3 Build Strategy

**Decision**: Static adapter, served by Axum.

**Flow**:
1. `npm run build` → `build/` directory
2. Axum `ServeDir` serves static assets
3. SPA routing handled by SvelteKit router

---

## 10. Future Considerations

### 10.1 Planned (Post-Launch)
- Desktop sync client (Rust + Tauri)
- Mobile apps (postponed, foundation exists)
- Full-text search (Elasticsearch/Meilisearch)
- Virus scanning (ClamAV)

### 10.2 Under Evaluation
- Redis for session caching
- CDN integration for public shares
- WebRTC for P2P large file transfer

### 10.3 Explicitly Out of Scope
- Full desktop sync (like Dropbox)
- Real-time document collaboration
- Plugin ecosystem
- Calendar/Contacts (Nextcloud scope)

---

## Appendix A: Configuration Reference

### Required Environment Variables

```bash
# Database
DATABASE_URL=postgres://user:pass@host:5432/db

# Storage
RUSTFS_ENDPOINT=http://localhost:9000
RUSTFS_BUCKET=rustshare
AWS_ACCESS_KEY_ID=...
AWS_SECRET_ACCESS_KEY=...

# Security
JWT_SECRET=change-me-in-production

# Server
SERVER_HOST=0.0.0.0
SERVER_PORT=8080
```

### Optional Environment Variables

```bash
# OIDC
OIDC_ISSUER_URL=
OIDC_CLIENT_ID=
OIDC_CLIENT_SECRET=

# Replication
REPLICATION_BATCH_SIZE=8
REPLICATION_ENABLED=true

# Rate Limiting
RUSTSHARE_RATE_LIMIT_AUTH_LOGIN_PER_MINUTE=10

# Frontend
FRONTEND_DIST_DIR=/app/frontend-build
```

---

## Appendix B: Migration History

| Date | Change | Impact |
|------|--------|--------|
| 2026-03-26 | Axum 0.7 → 0.8 | Route syntax, removed `async_trait` |
| 2026-03-26 | Tokio 1.37 → 1.50 | Performance improvements |
| 2026-03-26 | SQLx 0.7 → 0.8 | Performance improvements |
| 2026-03-21 | API Contract Freeze | `/api/v1/` stable surface |
| 2026-03-20 | Rate Limit Hardening | Per-endpoint limits |

---

*Last Updated: 2026-03-26*
