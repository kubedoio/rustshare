# RustShare System Architecture

> **Status:** Late MVP / pre-release — web product nearing a careful launch  
> **Last updated:** 2026-04-29

---

## 1. System Overview

RustShare is a self-hosted file-sharing and sync platform. It is designed to give individuals and small teams full control over their data while providing a modern, real-time web experience comparable to commercial alternatives.

### Design Philosophy

- **Self-hosted first:** You own the infrastructure, the data, and the keys.
- **Storage-first metadata:** RustShare is moving toward a world where the object store (S3/RustFS) is the canonical system of record, making the system naturally portable and backup-friendly.
- **Single-backend runtime:** The Axum backend serves both the API and the compiled SvelteKit SPA, keeping production deployments simple.
- **Async by default:** Expensive operations such as cross-node replication are decoupled from the synchronous request path.

---

## 2. Component Diagram

```text
┌─────────────────────────────────────────────────────────────┐
│                         Client                              │
│  (Browser / Mobile / Desktop)                               │
└──────────────────────┬──────────────────────────────────────┘
                       │ HTTPS (operator-provided TLS)
                       ▼
┌─────────────────────────────────────────────────────────────┐
│                      Nginx (reverse proxy)                  │
│           • TLS termination (production)                    │
│           • Static asset caching                            │
│           • WebSocket upgrade handling                      │
│           • Rate-limiting & security headers                │
└──────────┬────────────────────────────────┬─────────────────┘
           │                                │
           ▼                                ▼
┌─────────────────────┐         ┌─────────────────────────────┐
│   Backend (Axum)    │         │   RustFS / S3-compatible    │
│   Port 8080         │◄───────►│   Object Storage            │
│                     │         │   Port 9000 / 9001          │
│  • API (/api/v1/…)  │         │                             │
│  • WebSocket        │         │  • File contents            │
│  • SPA fallback     │         │  • Metadata sidecars        │
│  • Auth & sessions  │         │  • Replication targets      │
└──────────┬──────────┘         └─────────────────────────────┘
           │
           ▼
┌─────────────────────┐
│   PostgreSQL 16     │
│   Port 5432         │
│                     │
│  • Users, files,    │
│    folders, shares  │
│  • Replication jobs │
│  • Audit logs       │
│  • Notifications    │
└─────────────────────┘
```

> **Note:** In the **zero-PostgreSQL** runtime profile, PostgreSQL can be removed entirely and all metadata lives in RustFS. See [Future Evolution](#10-future-evolution).

---

## 3. Backend Architecture

The backend is a Cargo workspace split into focused crates plus the server binary.

| Crate | Path | Responsibility |
|-------|------|----------------|
| `rustshare-core` | `backend/crates/core` | Domain models, business logic, and entity definitions (users, files, folders, shares, notifications). |
| `rustshare-storage` | `backend/crates/storage` | PostgreSQL and object-storage integration. Abstracts away whether metadata lives in Postgres or RustFS. |
| `rustshare-auth` | `backend/crates/auth` | Password hashing (Argon2id), JWT issuance/validation, cookie-session helpers, OIDC state machine. |
| `rustshare-crypto` | `backend/crates/crypto` | Encryption utilities for data at rest (`RUSTSHARE_SECRET_ENCRYPTION_KEY`). |
| `rustshare-infrastructure` | `backend/crates/infrastructure` | External service integrations (email providers, push gateways, etc.). |
| `rustshare-server` | `backend/server` | Axum HTTP server, request routing, middleware stack, WebSocket handler, and SPA static-file serving. |

### Crate Dependency Flow

```text
server → auth, core, storage, crypto, infrastructure
storage → core
auth → core, crypto
infrastructure → core
```

This structure keeps the domain layer (`core`) free of framework and I/O concerns, making it easy to test and evolve.

---

## 4. Frontend Architecture

The frontend is a **SvelteKit SPA** built with Svelte 5 and compiled to static assets.

- **No separate Node.js runtime in production.** The compiled bundle is baked into the backend Docker image and served by Axum.
- **Real-time sync** is handled through a WebSocket manager that connects to `/api/ws` and dispatches events (file changes, share notifications, replication state) to the UI.
- **State management** is Svelte-native (runes + stores). The app is organized around domain modules: files, shares, notifications, and settings.
- **Authentication** relies on HTTP-only cookies; the frontend does not store JWTs in `localStorage`.

---

## 5. Data Flow

### 5.1 File Upload

1. User selects file(s) in the browser.
2. Frontend `POST /api/v1/files/upload` with multipart body.
3. Nginx forwards to backend (no body-size limit in default config).
4. Backend handler streams the file to RustFS/S3 under `files/{owner_id}/{uuid}`.
5. Backend writes metadata to the database (or RustFS, depending on `RUSTSHARE_METADATA_BACKEND`).
6. Backend enqueues an async **replication job** (if replication targets are configured).
7. Backend returns `201 Created` with file metadata.
8. WebSocket event `file_created` is broadcast to the user's active sessions.

### 5.2 File Download

1. User clicks download.
2. Frontend requests `GET /api/v1/files/{id}/download`.
3. Backend validates ownership or share permissions.
4. Backend generates a presigned URL (or proxies the stream) from RustFS/S3.
5. File is delivered to the client.

### 5.3 Share Link Flow

1. Owner creates a public share: `POST /api/v1/files/{id}/share`.
2. Backend generates a cryptographically random `share_token`, optionally with password and expiry.
3. Recipient visits `/share/{token}`.
4. Backend validates token, password, and expiry; creates a lightweight share session.
5. Recipient can view, download, or upload (for upload-only folder links) without a user account.
6. Access is logged to `share_access_log`.

### 5.4 Real-Time Sync Flow

1. Frontend opens WebSocket to `/api/ws`.
2. Connection authenticates via session cookie, `Authorization: Bearer` header, or `?token=` query parameter.
3. Server maintains a per-connection event subscriber.
4. On relevant mutations (file upload, share received, notification), the server publishes an event.
5. Subscriber pushes JSON event to the WebSocket; UI updates in real time.

---

## 6. Database Schema (High-Level)

The PostgreSQL schema is intentionally normalized and event-sourced-friendly.

### Major Entities

| Entity | Purpose | Key Relationships |
|--------|---------|-------------------|
| `users` | Accounts, quotas, admin flag | — |
| `folders` | Hierarchical directory structure | `owner_id → users`, `parent_folder_id → folders` |
| `files` | File metadata (name, path, size, hash, storage key) | `owner_id → users`, `parent_folder_id → folders` |
| `file_versions` | Version history and replication state | `file_id → files` |
| `shares` | Public links and user-to-user shares | `file_id → files`, `folder_id → folders`, `recipient_user_id → users` |
| `share_access_log` | Audit trail of share usage | `share_id → shares` |
| `notifications` | In-app notifications | `user_id → users` |
| `user_sessions` | Server-side session records | `user_id → users` |
| `replication_jobs` | Async replication work queue | `file_id → files`, `file_version_id → file_versions` |
| `replication_targets` | Destination configuration for replication | — |
| `events` | Append-only event store (CQRS foundation) | `aggregate_id`, `user_id → users` |
| `user_security_events` | Security-relevant audit events | `user_id → users` |

### Notes on Schema Evolution

- The `shares` table supports both **public links** (via `share_token`) and **internal shares** (via `recipient_user_id`).
- `file_versions.replication_state` tracks whether a version has been replicated to all targets.
- Event-sourced tables (`events`) enable future CQRS read-model rebuilds.

---

## 7. Object Storage

Files and metadata are stored in an S3-compatible object store (RustFS in Docker or AWS S3 in production).

### Object Layout

```text
{bucket}/
  files/
    {owner_id}/
      {file_uuid}              ← raw file content
  meta/
    notes/
      {file_id}.json           ← note sidecar metadata (kind, visibility, excerpt)
    notes/public/
      {share_id}.json          ← reverse index for public note lookup
  apps/rustshare/              ← metadata v2 objects (when enabled)
    ...
```

### Design Rationale

- **Storage keys are content-addressable where possible** (content hash in metadata), enabling deduplication opportunities.
- **Metadata sidecars** keep note data portable with the file itself.
- **Zero-PostgreSQL mode** migrates all relational metadata into hierarchical JSON objects in the bucket.

---

## 8. Request Lifecycle

```text
┌─────────┐    ┌─────────┐    ┌─────────────┐    ┌─────────┐    ┌──────────┐    ┌─────────┐
│  Client │───►│  Nginx  │───►│ Axum Router │───►│Middleware│───►│ Handler  │───►│ Service │
└─────────┘    └─────────┘    └─────────────┘    └─────────┘    └──────────┘    └────┬────┘
                                                                                      │
                                                                                      ▼
                                                                              ┌──────────────┐
                                                                              │ Repository   │
                                                                              │ (sqlx / S3)  │
                                                                              └──────┬───────┘
                                                                                     │
                                                                    ┌────────────────┼────────────────┐
                                                                    ▼                ▼                ▼
                                                              ┌──────────┐    ┌──────────┐    ┌──────────┐
                                                              │PostgreSQL│    │  RustFS  │    │  Redis   │
                                                              └──────────┘    └──────────┘    └──────────┘
```

1. **Nginx** terminates TLS (production), adds security headers, and proxies to the backend.
2. **Axum Router** matches the request path (`/api/v1/...`, `/api/ws`, or SPA fallback).
3. **Middleware Stack:**
   - **CORS** (configured via `ORIGIN`)
   - **Rate limiting** (per-route, Redis-backed or in-memory)
   - **Authentication** (cookie session validation or Bearer token)
   - **CSRF protection** (for cookie-authenticated mutations)
   - **Request tracing/logging**
4. **Handler** extracts validated parameters and calls the domain service.
5. **Service** executes business logic and calls repositories.
6. **Repository** performs the actual I/O against PostgreSQL, RustFS, or Redis.
7. **Response** flows back through the stack; WebSocket events are pushed asynchronously.

---

## 9. Technology Choices

| Layer | Technology | Rationale |
|-------|------------|-----------|
| Backend language | Rust | Memory safety, performance, and excellent async ecosystem. |
| Web framework | Axum | Composable middleware, first-class Tower integration, and strong WebSocket support. |
| Database | PostgreSQL 16 | Proven reliability, rich indexing, and JSONB for hybrid relational/document workloads. |
| Object storage | S3-compatible (RustFS) | Portable, deduplication-friendly, and allows zero-PostgreSQL operation. |
| Frontend framework | SvelteKit + Svelte 5 | Minimal runtime overhead, fine-grained reactivity, and simple static export. |
| Auth hashing | Argon2id | OWASP-recommended password hashing. |
| Encryption at rest | AES-256-GCM | Standard authenticated encryption for secrets and sensitive metadata. |
| Real-time | WebSocket (tokio-tungstenite) | Native async streaming with low overhead. |
| Replication | Async job queue | Upload latency is not tied to cross-node replication. |

---

## 10. Future Evolution

### Metadata Backend Migration

RustShare is transitioning from PostgreSQL as the metadata authority to **RustFS as the canonical metadata store**. This is controlled by `RUSTSHARE_METADATA_BACKEND`:

| Stage | Value | Behavior |
|-------|-------|----------|
| 1 | `postgres` | PostgreSQL only (current default). |
| 2 | `dual_write` | Writes to both; reads from PostgreSQL. |
| 3 | `rustfs_reads` | Writes to both; reads from RustFS. |
| 4 | `rustfs` | RustFS only; PostgreSQL optional or absent. |

Additional tuning:

```bash
RUSTSHARE_METADATA_CACHE=true
RUSTSHARE_METADATA_PREFIX=apps/rustshare
RUSTSHARE_METADATA_NAMESPACE=default
```

Admin endpoints for verification and repair:

- `GET /admin/metadata/health`
- `GET /admin/metadata/stats`
- `GET /admin/metadata/verify/parity`
- `POST /admin/metadata/repair`

See [docs/2026-03-27-metadata-refactor-design.md](2026-03-27-metadata-refactor-design.md) for the full migration plan.

### Known Roadmap Items

- **Mobile clients:** Aligned foundation exists; active product work is postponed.
- **Desktop app:** Early prototype only; not production-ready.
- **Deep observability:** Metrics dashboards and automated alerting are partial.
- **Virus scanning:** Out of scope for the core platform; planned as an external hook.

---

## See Also

- [Deployment Guide](DEPLOYMENT.md)
- [Production Readiness](PRODUCTION_READINESS.md)
- [Security Model](security-model.md)
- [Troubleshooting](troubleshooting.md)
- [API Contract Freeze](2026-03-21-api-contract-freeze.md)
