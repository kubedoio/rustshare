# Phase 0 Gap Report: RustShare Vault Sync Readiness Audit

> **Date:** 2026-06-02  
> **Scope:** Read-only analysis of RustShare codebase for Vault Sync implementation  
> **Status:** Complete — No code changes made  
> **Next Step:** Proceed to Phase 1 (Backend Foundation: Storage & Metadata)

> **Disclaimer:** Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.

---

## 1. Executive Summary

The RustShare codebase is **well-structured for adding Vault Sync**. It uses Axum, layered services, content-addressed S3 storage, event sourcing, and sqlx migrations. However, several **greenfield components** are required, and **two critical security gaps** were identified that should be addressed before or during Vault Sync implementation.

**Overall Readiness:** 7/10 — Architecture supports the feature, but auth scopes, device tracking, and server revisions need new infrastructure.

---

## 2. Existing Architecture Overview

### 2.1 Backend Stack

| Layer | Technology | Notes |
|-------|-----------|-------|
| Web Framework | Axum 0.8 + tower-http | Clean route merging pattern |
| Database | PostgreSQL + sqlx 0.8 | Compile-time checked queries, 59 migrations |
| Object Storage | S3-compatible (aws-sdk-s3) | Content-addressed: `blobs/{sha256}` |
| Auth | JWT (HMAC-SHA256) + Device Tokens | 24h JWT expiry; opaque device tokens (no expiry) |
| Event Sourcing | Append-only `events` table + broadcaster | WebSocket real-time sync |
| Frontend | SvelteKit 2.6 + Svelte 5 + Tailwind v4 | File-based routing, explorer store |

### 2.2 Crate Structure

```
backend/
  server/          # Axum handlers, routes, middleware, bootstrap
  crates/
    core/          # Domain: File, Folder, services, traits, events
    storage/       # Impl: MetadataStore (sqlx), EventStore, ObjectStore (S3), metadata_v2 docs
    auth/          # JWT manager, Argon2 hasher, session handling
    crypto/        # AES-GCM encryption, webhook HMAC
    infrastructure/# Repository pattern over raw sqlx

crates/            # Client-side (desktop app)
  sync-domain/     # Client sync types
  sync-engine/     # Client reconciliation engine
  sync-protocol/   # Wire formats
  client-state/    # State management
  file-ops/        # File operations
  platform/        # Platform abstractions
  vfs-macos/       # macOS VFS
  vfs-win/         # Windows VFS
```

### 2.3 Service Layer Pattern

```
HTTP Handler (Axum extractors: State<AppState>, AuthenticatedUser)
    ↓
Server-local service OR Core service
    ↓
Storage trait (generic, defined in core)
    ↓
Storage impl (MetadataStore, EventStore, ObjectStore)
    ↓
Postgres / S3
```

---

## 3. What EXISTS and Can Be Leveraged

### 3.1 File Storage ✅

| Capability | Status | Location |
|-----------|--------|----------|
| Content-addressed blob storage | ✅ | `ObjectStore` → `blobs/{sha256}` |
| SHA-256 hashing | ✅ | `calculate_sha256(&Bytes)` in `core/src/validation.rs` |
| Deduplication | ✅ | Skips `put` if `exists("blobs/{hash}")` |
| Presigned URLs | Superseded for user-facing file downloads | File downloads now use verified backend streaming; low-level object-store presigning remains available only for explicitly internal/object-store use cases. |
| Path traversal prevention | ✅ | `validate_file_name`, `normalize_path` reject `..` and `/` |
| Tenant isolation (app-layer) | ✅ | `tenant_id` in all queries |
| Soft delete | ✅ | `deleted_at` on `files`, `folders` |
| File versioning | ✅ | `file_versions` table with `version_number` |

### 3.2 Sync Infrastructure ✅ (Partial)

| Capability | Status | Location |
|-----------|--------|----------|
| Event store (append-only) | ✅ | `events` table: `FileUploaded`, `FileModified`, etc. |
| WebSocket broadcast | ✅ | `EventBroadcaster` + `/api/ws` handler |
| Sync cursors / deltas | ✅ | `SyncService` + `RustFsSyncRepository` |
| Device tokens | ✅ | `device_tokens` table + pairing flow |
| Device revocation | ✅ | `DELETE /api/v1/user/devices/{id}` |
| Conflict resolution strategies | ✅ | `ServerWins`, `ClientWins`, `LastWriteWins`, `Rename` |

### 3.3 Frontend Infrastructure ✅ (Partial)

| Capability | Status | Location |
|-----------|--------|----------|
| File explorer / tree | ✅ | `FileBrowserContent`, `FolderTree`, `explorerStore` |
| Markdown editor/viewer | ✅ | Tiptap-based `RichMarkdownEditor`, `RichMarkdownViewer` |
| Module system | ✅ | `ModuleDefinition` registry, dynamic sidebar |
| API client pattern | ✅ | `ApiClient` class + domain endpoint files |
| Search (basic) | ✅ | `GlobalSearch` → `/search?q=...` |
| Sanitization | ✅ | DOMPurify-based `sanitizeHtml` |

---

## 4. What is MISSING (Gaps)

### 4.1 🔴 Critical Gaps (Must Build New)

| # | Gap | Impact | Required For |
|---|-----|--------|--------------|
| 1 | **No "vault" entity** | High | Vault creation, listing, scoping |
| 2 | **No server revision clock** | High | Conflict detection, 409 responses, stale write prevention |
| 3 | **No tombstone persistence for sync** | High | Delete propagation, preventing re-upload of deleted files |
| 4 | **No scoped tokens / PAT system** | High | Vault Sync plugin auth (needs `vault_sync:*` scopes) |
| 5 | **No device_id in file metadata** | Medium | Attribution, conflict file naming (`Senol-MacBook`) |
| 6 | **No per-vault ACL / membership** | Medium | Multi-user vault access control |
| 7 | **No bulk tree listing API** | Medium | Initial sync / manifest endpoint |
| 8 | **No content verification protocol** | Low | Partial sync integrity checks |
| 9 | **No external adapter registry** | Low | Future-proofing for other vault types |

### 4.2 🟡 Security Gaps (Should Fix Before/During)

| # | Gap | Severity | Details |
|---|-----|----------|---------|
| 1 | **RLS is non-functional** | 🔴 Critical | `app.current_user_id` is set to nil UUID in connection pool and **never updated per-request**. Tenant isolation relies entirely on application-layer query binding. |
| 2 | **Device tokens never expire** | 🟡 Medium | Leaked device token is valid forever unless manually revoked. No TTL. |
| 3 | **No global auth middleware** | 🟡 Medium | Auth is extractor-based. Missing extractor on a route = unprotected endpoint. |
| 4 | **Schema/model drift** | 🟡 Medium | `DeviceToken` domain model has `tenant_id`, but migration does not. |
| 5 | **No audit for sync ops** | 🟡 Medium | No security events for file sync operations. |

### 4.3 🟢 Frontend Gaps

| # | Gap | Impact | Required For |
|---|-----|--------|--------------|
| 1 | **No "Vaults" navigation section** | Medium | Sidebar, LeftRail need new entries |
| 2 | **No wikilink rendering `[[...]]`** | Medium | Obsidian vault preview |
| 3 | **No file badges for sync metadata** | Low | Source, adapter, last synced, device |
| 4 | **No "Open in Obsidian" deeplink** | Low | Local convenience URI |
| 5 | **Filename/H1 conflation in Notes** | Medium | Notes UI derives title from metadata/filename; vault files need independence |
| 6 | **Search lacks vault types** | Low | Search API only returns `file` / `folder` |

---

## 5. Affected Modules & Files to Touch

### 5.1 Backend — New Files

```
backend/migrations/
  20260602xxxxxx_create_vaults.sql
  20260602xxxxxx_create_vault_files.sql
  20260602xxxxxx_create_vault_devices.sql

backend/server/src/
  handlers/vault_sync.rs          # NEW: HTTP handlers for /api/vault-sync/v1
  services/vault_sync_service.rs  # NEW: Business logic
  routes.rs                       # MODIFY: Add vault_sync_routes()
  main.rs                         # MODIFY: .merge(routes::vault_sync_routes())
  state.rs                        # MODIFY: Add VaultSyncService to AppState
  bootstrap.rs                    # MODIFY: Initialize VaultSyncService
  handlers/mod.rs                 # MODIFY: Add mod vault_sync

backend/crates/core/src/
  domain/vault.rs                 # NEW: Vault, VaultFile, VaultDevice types
  services/vault_sync_service.rs  # NEW: Core vault sync service trait/impl
  services/errors.rs              # MODIFY: Add VaultSyncError variants

backend/crates/storage/src/
  metadata.rs or metadata_v2/     # MODIFY/NEW: Vault store methods
```

### 5.2 Backend — Files to Modify

| File | Why |
|------|-----|
| `backend/server/src/routes.rs` | Add `vault_sync_routes()` |
| `backend/server/src/main.rs` | Merge vault sync router |
| `backend/server/src/state.rs` | Wire `VaultSyncService` into `AppState` |
| `backend/server/src/bootstrap.rs` | Initialize service, migration runner |
| `backend/server/src/handlers/mod.rs` | Export vault sync handlers |
| `backend/crates/core/src/domain/mod.rs` | Add vault domain types |
| `backend/crates/core/src/services/mod.rs` | Add vault sync service |
| `backend/crates/storage/src/lib.rs` | Implement storage traits for vaults |

### 5.3 Frontend — New Files

```
frontend/src/lib/api/vaults.ts          # NEW: Vault Sync API client
frontend/src/lib/components/vaults/     # NEW: Vault navigation, badges, status
```

### 5.4 Frontend — Files to Modify

| File | Why |
|------|-----|
| `frontend/src/lib/api/types.ts` | Add vault/adapter/sync metadata |
| `frontend/src/lib/layout/LeftRail.svelte` | Add "Vaults" primary nav |
| `frontend/src/lib/layout/SidebarNav.svelte` | Add Vaults/Obsidian sections |
| `frontend/src/lib/explorer/types.ts` | Extend `ExplorerRoot` for vaults |
| `frontend/src/routes/(app)/files/+page.svelte` | Handle vault query params |
| `frontend/src/lib/components/files/FileListItem.svelte` | Add vault badge slots |
| `frontend/src/lib/editor/adapter/markdown.ts` | Add wikilink support |
| `frontend/src/lib/editor/components/RichMarkdownViewer.svelte` | Handle wikilink clicks |
| `frontend/src/lib/modules/registry.ts` | Optionally register vault as module |
| `frontend/src/lib/layout/topbar/GlobalSearch.svelte` | Add vault results |

---

## 6. Database Schema Recommendations

### 6.1 New Tables (Follow Existing Conventions)

```sql
-- vaults
CREATE TABLE vaults (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    owner_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    adapter VARCHAR(50) NOT NULL,  -- 'obsidian_vault'
    root_path TEXT,
    server_rev BIGINT NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_vaults_tenant ON vaults(tenant_id);
CREATE INDEX idx_vaults_owner ON vaults(owner_user_id);

-- vault_files (sync metadata + tombstones)
CREATE TABLE vault_files (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    vault_id UUID NOT NULL REFERENCES vaults(id) ON DELETE CASCADE,
    relative_path TEXT NOT NULL,
    content_type TEXT,
    sha256 VARCHAR(64),
    size BIGINT,
    server_rev BIGINT NOT NULL,
    mtime_client BIGINT,
    mtime_server TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted BOOLEAN NOT NULL DEFAULT FALSE,
    deleted_at TIMESTAMPTZ,
    last_writer_device_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (vault_id, relative_path)
);
CREATE INDEX idx_vault_files_vault ON vault_files(vault_id, server_rev);
CREATE INDEX idx_vault_files_vault_path ON vault_files(vault_id, relative_path);

-- vault_devices (sync-scoped device tracking)
CREATE TABLE vault_devices (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    vault_id UUID REFERENCES vaults(id) ON DELETE SET NULL,
    device_name VARCHAR(255) NOT NULL,
    client_type VARCHAR(50) NOT NULL,  -- 'obsidian_plugin'
    client_version VARCHAR(50),
    last_sync_rev BIGINT,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_vault_devices_user ON vault_devices(user_id);
CREATE INDEX idx_vault_devices_vault ON vault_devices(vault_id);
```

### 6.2 sqlx Prepare Workflow

The project uses **offline query verification** (240 `.json` files in `backend/.sqlx/`). After writing migrations and Rust code with `sqlx::query!` macros, **run**:

```bash
cd backend && cargo sqlx prepare
```

CI depends on these prepared artifacts.

---

## 7. Auth & Security Recommendations

### 7.1 Recommended: Extend Device Token System

Rather than building a full OAuth scope system from scratch, extend the existing device token flow:

```sql
CREATE TABLE vault_sync_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    device_name TEXT NOT NULL,
    device_id TEXT NOT NULL,          -- stable client-visible UUID
    scopes TEXT[] NOT NULL DEFAULT '{}',
    tenant_id UUID NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    last_used_at TIMESTAMPTZ DEFAULT NOW(),
    revoked_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ
);
```

**Rationale:**
- Device pairing flow already handles the UX pattern Vault Sync needs.
- `AuthenticatedUser` extractor can be extended to recognize vault sync tokens.
- Keeps auth complexity centralized in `extractors.rs`.

### 7.2 Critical Security Fixes to Apply

1. **Fix RLS** — Add per-request middleware that sets `SET LOCAL app.current_user_id = ?` and `SET LOCAL app.current_tenant_id = ?` on the connection for the request duration, OR remove RLS if not intended.
2. **Add token TTL** — Vault sync tokens should have an expiration (e.g., 90 days) with refresh capability.
3. **Add audit logging** — Write to `user_security_events` or a new `vault_sync_audit_log` for every vault sync operation.
4. **Fix schema/model drift** — Add `tenant_id` to `device_tokens` migration or remove it from domain model.

---

## 8. Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Data loss during sync | Low | Critical | Server revisions, conflict files, tombstones, extensive testing |
| Trademark/naming violation | Medium | High | ADR-004, SPEC-005, terminology CI check, compliance review |
| Path traversal | Low | High | Reuse existing `validate_file_name`, `normalize_path` |
| Double-sync with Obsidian Sync | Medium | Medium | Plugin warning + clear documentation |
| Large vault performance | Medium | Medium | Manifest pagination, batching, debounce |
| Auth scope escalation | Low | High | Scope-aware extractor, token TTL, audit logs |
| RLS bypass | Medium | High | Fix per-request RLS context OR remove RLS |

---

## 9. Recommended Execution Sequence

Based on this audit, the implementation phases should proceed as follows:

1. **Phase 0.5 — Security Hardening (Optional but Recommended)**
   - Fix RLS context setting per-request
   - Fix `device_tokens` schema drift
   - Add token TTL support

2. **Phase 1 — Database & Storage Foundation**
   - Create `vaults`, `vault_files`, `vault_devices` migrations
   - Implement `VaultSyncService` in core + storage layers
   - Add server revision management
   - Add tombstone support

3. **Phase 2 — Vault Sync API**
   - Implement `/api/vault-sync/v1` endpoints
   - Add conflict detection (409 responses)
   - Add scoped token extractor
   - Integration tests

4. **Phase 3 — RustShare UI**
   - Add Vaults section to sidebar
   - File badges, preview, search integration
   - Wikilink support

5. **Phase 4 — Obsidian Plugin**
   - Build plugin skeleton
   - Manual sync MVP
   - Conflict file creation

6. **Phase 5 — Incremental Sync**
   - Event-based sync, debounce, offline queue
   - Rename detection

7. **Phase 6 — Testing & Hardening**
   - Integration tests, compliance scan, terminology check

---

## 10. Appendix: Key Source Files Referenced

### Backend
- `backend/server/src/main.rs` — Router assembly
- `backend/server/src/routes.rs` — Route modules
- `backend/server/src/state.rs` — AppState wiring
- `backend/server/src/bootstrap.rs` — Service initialization
- `backend/server/src/handlers/extractors.rs` — Auth extractors
- `backend/server/src/handlers/files.rs` — File handlers
- `backend/server/src/handlers/sync.rs` — WebSocket sync
- `backend/crates/core/src/services/file_service.rs` — FileService
- `backend/crates/core/src/services/sync_service.rs` — SyncService
- `backend/crates/storage/src/metadata.rs` — MetadataStore
- `backend/crates/storage/src/object_store.rs` — S3 client
- `backend/crates/storage/src/metadata_v2/schemas.rs` — Document schemas
- `backend/crates/auth/src/jwt.rs` — JWT manager
- `backend/migrations/` — All SQL migrations

### Frontend
- `frontend/src/routes/(app)/files/+page.svelte` — File explorer page
- `frontend/src/lib/layout/LeftRail.svelte` — Primary navigation
- `frontend/src/lib/layout/SidebarNav.svelte` — Secondary sidebar
- `frontend/src/lib/explorer/store.svelte.ts` — Navigation store
- `frontend/src/lib/editor/adapter/markdown.ts` — Markdown pipeline
- `frontend/src/lib/api/client.ts` — API client
- `frontend/src/lib/modules/registry.ts` — Module definitions

---

*End of Phase 0 Gap Report. Ready for Phase 1 implementation.*
