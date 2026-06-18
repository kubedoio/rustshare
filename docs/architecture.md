# RustShare System Architecture

> **Status:** Production-readiness gap closure complete — Workstreams A–F  
> **Last updated:** 2026-06-18

---

## 1. System Overview

RustShare is a self-hosted file-sharing and sync platform. It is designed to give individuals and small teams full control over their data while providing a modern, real-time web experience comparable to commercial alternatives.

### Design Philosophy

- **Self-hosted first:** You own the infrastructure, the data, and the keys.
- **Storage-first metadata:** RustShare is moving toward a world where the object store (S3/RustFS) is the canonical system of record, making the system naturally portable and backup-friendly.
- **Single-backend runtime:** The Axum backend serves both the API and the compiled SvelteKit SPA, keeping production deployments simple.
- **Async by default:** Expensive operations such as cross-node replication are decoupled from the synchronous request path.
- **Tenant-aware by default:** Metadata and share links are scoped to a tenant at the repository layer; unauthenticated public routes derive or validate tenant context before resolving data.

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
| `rustshare-auth` | `backend/crates/auth` | Password hashing (Argon2id), JWT issuance/validation, cookie-session helpers, share-session claims, OIDC state machine. |
| `rustshare-crypto` | `backend/crates/crypto` | Encryption utilities for data at rest (`RUSTSHARE_SECRET_ENCRYPTION_KEY`) and HMAC-SHA256 webhook signature verification. |
| `rustshare-infrastructure` | `backend/crates/infrastructure` | External service integrations (email providers, push gateways, etc.). |
| `rustshare-server` | `backend/server` | Axum HTTP server, request routing, middleware stack, WebSocket handler, SPA static-file serving, and admin endpoints. |

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
4. Backend handler streams the multipart field to a temporary file and computes the SHA-256 content hash.
5. Backend streams verified bytes to RustFS/S3 under the content-addressed key `blobs/{sha256}`.
6. Backend cleans up the temporary file on success or error.
7. Backend writes metadata to the database (or RustFS, depending on `RUSTSHARE_METADATA_BACKEND`).
8. Backend enqueues an async **replication job** (if replication targets are configured).
9. Backend returns `201 Created` with file metadata.
10. WebSocket event `file_created` is broadcast to the user's active sessions.

### 5.2 File Download

1. User clicks download.
2. Frontend requests `GET /api/v1/files/{id}/download`.
3. Backend validates ownership or share permissions.
4. Backend calls `ObjectStore::get_stream` and returns a streaming body without buffering the entire object in memory.
5. File is delivered to the client with `Content-Type` preserved. `Content-Length` is preserved where available except for content-addressed blob streams, where the backend omits it because SHA-256 integrity can only be confirmed at end-of-stream.

### 5.3 Share Link Flow

1. Owner creates a public share: `POST /api/v1/files/{id}/share`.
2. Backend generates a cryptographically random `share_token`, optionally with password and expiry.
3. Recipient visits `/share/{token}`.
4. For unauthenticated public share routes, `X-Tenant-ID` is optional. If present, backend verifies it matches the share's tenant; if omitted, backend derives the tenant from the globally unique share token.
5. The share session JWT includes `tenant_id` so subsequent share-session routes remain scoped to the issuing tenant.
6. Recipient can view, download, or upload (for upload-only folder links) without a user account.
7. Access is logged to `share_access_log`.

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
| `users` | Accounts, quotas, admin flag | `tenant_id` |
| `folders` | Hierarchical directory structure | `owner_id → users`, `parent_folder_id → folders`, `tenant_id` |
| `files` | File metadata (name, path, size, hash, storage key) | `owner_id → users`, `parent_folder_id → folders`, `tenant_id` |
| `file_versions` | Version history and replication state | `file_id → files`, `tenant_id` |
| `shares` | Public links and user-to-user shares | `file_id → files`, `folder_id → folders`, `recipient_user_id → users`, `tenant_id` |
| `share_access_log` | Audit trail of share usage | `share_id → shares` |
| `notifications` | In-app notifications | `user_id → users`, `tenant_id` |
| `user_sessions` | Server-side session records | `user_id → users`, `tenant_id` |
| `replication_jobs` | Async replication work queue | `file_id → files`, `file_version_id → file_versions` |
| `replication_targets` | Destination configuration for replication | — |
| `events` | Append-only event store (CQRS foundation) | `aggregate_id`, `user_id → users` |
| `user_security_events` | Security-relevant audit events | `user_id → users` |
| `vaults` / `vault_files` / `vault_devices` | Obsidian vault sync metadata | `tenant_id` |

### Notes on Schema Evolution

- The `shares` table supports both **public links** (via `share_token`) and **internal shares** (via `recipient_user_id`).
- `file_versions.replication_state` tracks whether a version has been replicated to all targets.
- Event-sourced tables (`events`) enable future CQRS read-model rebuilds.
- `tenant_id` is present on all tenant-scoped entities and is used as the primary isolation filter.

---

## 7. Object Storage

Files and metadata are stored in an S3-compatible object store (RustFS in Docker or AWS S3 in production).

### Object Layout

```text
{bucket}/
  blobs/
    {sha256}                   ← content-addressed file bytes
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
   - **Request ID / correlation ID** — `X-Request-ID` is preserved from the client when valid or generated; propagated into tracing spans as `request_id` and returned in response headers.
   - **Authentication** (cookie session validation or Bearer token)
   - **CSRF protection** (for cookie-authenticated mutations)
   - **Request tracing/logging**
4. **Handler** extracts validated parameters and calls the domain service.
5. **Service** executes business logic and calls repositories.
6. **Repository** performs the actual I/O against PostgreSQL, RustFS, or Redis. Tenant-scoped queries include `WHERE tenant_id = $N`.
7. **Response** flows back through the stack; WebSocket events are pushed asynchronously.

---

## 9. Tenant Isolation Model

Tenant isolation is enforced as a defense-in-depth boundary at multiple layers:

### Authenticated Requests

- Every authenticated session carries the user's `tenant_id` (from `user_sessions`).
- Handlers pass `tenant_id` into service and repository calls.
- Repository methods filter by `tenant_id` in SQL `WHERE` clauses.

### Anonymous Public Routes

- Public-share endpoints accept optional `X-Tenant-ID`: when supplied it must match the share tenant, and when omitted the tenant is derived from the globally unique share token.
- Public chat-unfurl requests can also use `X-Tenant-ID` to scope tenant resolution.
- The share token is resolved only within the effective tenant. A token from tenant A is treated as non-existent if requested with tenant B's `X-Tenant-ID`.

### Share-Session JWT

- When `validate_and_create_session` succeeds, the resulting JWT contains the share's `tenant_id`.
- All share-session routes use this claim to scope subsequent lookups.

### What Was Removed

- The previous PostgreSQL RLS context middleware was a no-op: it set session variables on a connection that was returned to the pool before the handler ran. It has been removed. RLS may be reintroduced only with connection pinning or `before_acquire` `SET` semantics.

### Password Login Compatibility Behavior

- Password login accepts an optional `tenant_id` and uses a tenant-scoped, case-insensitive email lookup when provided. If `tenant_id` is omitted for backward compatibility, the login path rejects ambiguous emails that exist in more than one tenant.

---

## 10. Secret Management and Rotation

Secrets are loaded from environment variables and held in memory only. They are never written to the database or object store.

### Required Production Secrets

| Secret | Purpose |
|--------|---------|
| `JWT_SECRET` | Signs session and share-session JWTs. |
| `RUSTSHARE_SECRET_ENCRYPTION_KEY` | Encrypts sensitive data at rest (AES-256-GCM). |
| `POSTGRES_PASSWORD` | PostgreSQL access. |
| `RUSTFS_ROOT_USER` / `RUSTFS_ROOT_PASSWORD` | Object storage admin credentials. |
| `STORAGE_ACCESS_KEY` / `STORAGE_SECRET_KEY` | S3 API credentials (must match RustFS root credentials). |
| `OIDC_CLIENT_SECRET` | OIDC RP authentication. |
| `RUSTSHARE_CHAT_WEBHOOK_SECRET` | HMAC-SHA256 webhook signing. |
| `METRICS_API_TOKEN` | Optional bearer token for `/metrics`. |

### Rotation Guidance

- Use `./scripts/pre-flight.sh` to generate strong values for a new deployment.
- Rotate `JWT_SECRET` on suspected compromise or quarterly. All sessions are invalidated; users must log in again.
- Rotate `RUSTSHARE_SECRET_ENCRYPTION_KEY` on suspected compromise. **Back up the old key** until all data encrypted with it has been re-encrypted.
- Rotate PostgreSQL and RustFS/S3 credentials together; update `.env` and restart the stack.
- Detailed rotation procedures (including session revocation and tenant containment) are in the [Security Incident Runbook](runbooks/security-incident.md).

---

## 11. Webhook Security Model

### Incoming Chat Webhooks

- Chat integration webhooks are verified with HMAC-SHA256 over the raw request body before deserialization.
- Expected signature formats: `v1=<hex>` or `t=<timestamp>,v1=<hex>`.
- Verification uses constant-time comparison.
- Missing or invalid signatures return `401 Unauthorized`.

### Outgoing Webhooks

- Dispatched events include `X-RustShare-Signature` and `X-RustShare-Event` headers.
- URLs must use HTTPS in production. HTTP is allowed only in debug builds or when `RUSTSHARE_ALLOW_HTTP_WEBHOOKS` is explicitly enabled (dev-only).
- Webhook registration is restricted to admin users via `/api/v1/admin/integrations/chat/webhooks`.

---

## 12. Technology Choices

| Layer | Technology | Rationale |
|-------|------------|-----------|
| Backend language | Rust | Memory safety, performance, and excellent async ecosystem. |
| Web framework | Axum | Composable middleware, first-class Tower integration, and strong WebSocket support. |
| Database | PostgreSQL 16 | Proven reliability, rich indexing, and JSONB for hybrid relational/document workloads. |
| Object storage | S3-compatible (RustFS) | Portable, deduplication-friendly, and allows zero-PostgreSQL operation. |
| Frontend framework | SvelteKit + Svelte 5 | Minimal runtime overhead, fine-grained reactivity, and simple static export. |
| Auth hashing | Argon2id | OWASP-recommended password hashing. |
| Encryption at rest | AES-256-GCM | Standard authenticated encryption for secrets and sensitive metadata. |
| Webhook signatures | HMAC-SHA256 | Industry standard for webhook authenticity and integrity. |
| Real-time | WebSocket (tokio-tungstenite) | Native async streaming with low overhead. |
| Replication | Async job queue | Upload latency is not tied to cross-node replication. |

---

## 13. Future Evolution

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
- **PostgreSQL RLS defense-in-depth:** Repository-level tenant filtering is the active boundary. RLS can be reintroduced later only with connection pinning or per-acquire tenant context.

---

## See Also

- [Deployment Guide](DEPLOYMENT.md)
- [Production Readiness](PRODUCTION_READINESS.md)
- [Security Model](security-model.md)
- [Security Incident Runbook](runbooks/security-incident.md)
- [Backup/Restore Runbook](runbooks/backup-restore.md)
- [Troubleshooting](troubleshooting.md)
- [API Contract Freeze](2026-03-21-api-contract-freeze.md)
