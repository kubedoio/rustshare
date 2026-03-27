# RustShare Metadata Refactor Design Document

## Executive Summary

This document describes the architecture and implementation plan for migrating RustShare from PostgreSQL as the canonical metadata store to RustFS-backed metadata objects, indexes, and events.

**Target State:**
- PostgreSQL is no longer the source of truth for file/folder/share/version metadata
- Metadata documents are stored as durable objects in RustFS
- Read indexes/projections are rebuildable from canonical metadata
- Events are append-only objects for audit/rebuild/sync
- Runtime cache provides hot-path acceleration
- Optional local filesystem backend for development

---

## 1. Current State Analysis

### 1.1 PostgreSQL Dependencies

| Component | Current Implementation |
|-----------|----------------------|
| MetadataStore | `storage/src/metadata.rs` - SQLx queries |
| EventStore | `storage/src/event_store.rs` - SQLx queries |
| Domain Models | `sqlx::FromRow` derive macros |
| Repositories | SQLx-based in `infrastructure/src/repositories/` |

### 1.2 Current Schema (Key Tables)

```sql
-- Files (canonical metadata)
files: id, name, path, content_hash, size, mime_type, parent_folder_id, owner_id, current_version, created_at, modified_at

-- Folders (canonical metadata)
folders: id, name, path, parent_folder_id, owner_id, created_at, updated_at

-- File Versions (canonical metadata)
file_versions: id, file_id, version_number, content_hash, storage_key, size, replication_state, created_by, created_at

-- Shares (canonical metadata)
shares: id, file_id, folder_id, share_token, permissions, password_hash, expires_at, created_by, created_at, revoked_at, access_count

-- Events (append-only log)
events: event_id, event_type, aggregate_id, aggregate_type, payload, user_id, timestamp, version

-- Replication Jobs (ephemeral state)
replication_jobs: id, file_id, file_version_id, storage_key, status, attempt_count, ...
```

### 1.3 Current Object Storage Usage

```
rustshare-files/
  blobs/
    {content_hash}          # Immutable file content
  thumbnails/
    {file_id}/{size}.webp   # Generated thumbnails
```

---

## 2. Target Architecture

### 2.1 Object Layout

```
rustshare-files/
  # Shared immutable blobs (content-addressed)
  shared/
    blobs/
      sha256/
        ab/
          cd/
            {hash}          # Content-addressed blobs
  
  # App-specific metadata (RustShare)
  apps/
    rustshare/
      meta/
        # Canonical metadata documents
        folders/            {folder_id}.json
        files/              {file_id}.json
        file_versions/      {file_id}/{version_id}.json
        shares/             {share_id}.json
        users/              {user_id}.json
        tombstones/         files/{file_id}.json, folders/{folder_id}.json
        
        # Append-only events
        events/
          2026/
            03/
              27/
                {event_id}.json
      
      indexes/              # Rebuildable projections
        folders/
          {folder_id}/
            children.json   # Folder children index
        users/
          {user_id}/
            roots.json      # Root folders for user
            recent.json     # Recently modified files
            shared_with_me.json  # Shares received by user
        shares/
          by_token/
            {token_hash_prefix}/
              {token}.json  # Share lookup by token
```

### 2.2 Document Schemas (Versioned)

#### Folder Document (v1)
```json
{
  "schema_version": 1,
  "id": "uuid",
  "namespace_id": "uuid",
  "parent_id": "uuid|null",
  "name": "string",
  "owner_id": "uuid",
  "created_at": "ISO8601",
  "updated_at": "ISO8601",
  "version": 1,
  "deleted": false
}
```

#### File Head Document (v1)
```json
{
  "schema_version": 1,
  "id": "uuid",
  "namespace_id": "uuid",
  "parent_id": "uuid|null",
  "name": "string",
  "owner_id": "uuid",
  "current_version_id": "uuid",
  "size": 12345,
  "mime": "application/pdf",
  "content_ref": "sha256:abc...",
  "checksum": "sha256:abc...",
  "created_at": "ISO8601",
  "updated_at": "ISO8601",
  "version": 3,
  "deleted": false
}
```

#### File Version Document (v1)
```json
{
  "schema_version": 1,
  "id": "uuid",
  "file_id": "uuid",
  "content_ref": "sha256:abc...",
  "size": 12345,
  "checksum": "sha256:abc...",
  "created_by": "uuid",
  "created_at": "ISO8601",
  "change_description": "string|null"
}
```

#### Share Document (v1)
```json
{
  "schema_version": 1,
  "id": "uuid",
  "resource_type": "file|folder",
  "resource_id": "uuid",
  "scope": "public|user",
  "permissions": "view|edit|admin",
  "token_hash": "string|null",
  "recipient_user_id": "uuid|null",
  "expires_at": "ISO8601|null",
  "created_by": "uuid",
  "created_at": "ISO8601",
  "revoked_at": "ISO8601|null",
  "version": 1
}
```

#### Folder Children Index (v1)
```json
{
  "schema_version": 1,
  "folder_id": "uuid",
  "version": 5,
  "updated_at": "ISO8601",
  "children": [
    {
      "id": "uuid",
      "kind": "file|folder",
      "name": "string",
      "deleted": false,
      "size": 12345,
      "mime": "string|null",
      "updated_at": "ISO8601"
    }
  ]
}
```

#### Event Document (v1)
```json
{
  "schema_version": 1,
  "id": "uuid",
  "event_type": "file_uploaded|file_modified|...",
  "actor_id": "uuid",
  "resource_type": "file|folder|share",
  "resource_id": "uuid",
  "occurred_at": "ISO8601",
  "correlation_id": "uuid|null",
  "payload": { ... }
}
```

#### Tombstone Document (v1)
```json
{
  "schema_version": 1,
  "id": "uuid",
  "resource_type": "file|folder",
  "resource_id": "uuid",
  "deleted_at": "ISO8601",
  "deleted_by": "uuid",
  "previous_parent_id": "uuid|null",
  "restore_metadata": { ... }
}
```

---

## 3. Storage Abstractions

### 3.1 Core Traits

```rust
/// Blob storage for immutable content
#[async_trait]
pub trait BlobStore: Send + Sync {
    async fn put(&self, key: &str, data: Bytes) -> Result<()>;
    async fn get(&self, key: &str) -> Result<Bytes>;
    async fn exists(&self, key: &str) -> Result<bool>;
    async fn delete(&self, key: &str) -> Result<()>;
    fn content_key(&self, hash: &str) -> String;
}

/// Metadata document storage
#[async_trait]
pub trait MetadataStore: Send + Sync {
    async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>>;
    async fn put<T: Serialize>(&self, key: &str, value: &T, opts: PutOptions) -> Result<()>;
    async fn delete(&self, key: &str) -> Result<()>;
}

/// Put options for conditional writes
pub struct PutOptions {
    pub if_match: Option<String>,  // ETag for optimistic concurrency
    pub if_none_match: Option<String>,
}

/// Index/projection storage
#[async_trait]
pub trait IndexStore: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>>;
    async fn put(&self, key: &str, data: &[u8], opts: PutOptions) -> Result<()>;
    async fn rebuild(&self, source: &str) -> Result<()>;
}

/// Append-only event storage
#[async_trait]
pub trait EventStore: Send + Sync {
    async fn append(&self, event: &EventDoc) -> Result<()>;
    async fn read_range(&self, start: &str, end: &str) -> Result<Vec<EventDoc>>;
}

/// Coordination for multi-object mutations
#[async_trait]
pub trait MetadataCoordination: Send + Sync {
    /// Attempt to acquire a lease for mutation
    async fn acquire_lease(&self, resource_id: &str, ttl_secs: u64) -> Result<Lease>;
    /// Release a lease
    async fn release_lease(&self, lease: &Lease) -> Result<()>;
    /// Check if resource has active lease
    async fn check_lease(&self, resource_id: &str) -> Result<Option<Lease>>;
}

pub struct Lease {
    pub resource_id: String,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

/// Runtime cache for hot paths
pub trait RuntimeIndex: Send + Sync {
    fn get_folder_children(&self, folder_id: &str) -> Option<Vec<ChildEntry>>;
    fn put_folder_children(&self, folder_id: &str, children: Vec<ChildEntry>);
    fn invalidate_folder(&self, folder_id: &str);
    fn rebuild_from_durable(&self, store: &dyn IndexStore) -> Result<()>;
}
```

### 3.2 Backend Implementations

| Backend | Description | Use Case |
|---------|-------------|----------|
| `RustFsMetadataBackend` | Object-store native | Production |
| `LocalFsMetadataBackend` | Filesystem with sidecars | Development, testing |
| `PostgresMetadataBackend` | Legacy SQL | Migration, fallback |
| `DualWriteBackend` | Writes to both backends | Migration phase |

---

## 4. Consistency Model

### 4.1 Chosen Model: Option A (Synchronous)

**Decision:** Use **synchronous metadata + synchronous index updates + event append**

**Rationale:**
- RustShare is a user-facing file system with immediate consistency expectations
- Folder listings must reflect recent mutations without delay
- The complexity of async projection updates is not justified for the workload
- Events are for audit/rebuild, not for driving consistency

### 4.2 Write Protocol

For each mutation:

1. **Acquire coordination** (lease or conditional check)
2. **Validate** current state
3. **Write canonical documents** (with conditional PUT)
4. **Write indexes** (folder children, user views)
5. **Append event** (for audit/rebuild)
6. **Update runtime cache**
7. **Release coordination**

### 4.3 Failure Handling

| Failure Point | Recovery Strategy |
|--------------|-------------------|
| After canonical write, before index | Index rebuild tool detects and repairs |
| After index, before event | Event stream has gap (acceptable for audit) |
| Runtime cache out of sync | Cache invalidation/refresh on next read |
| Coordination lease lost | Mutation fails, client retries |

---

## 5. Mutation Protocols

### 5.1 Create Folder

```
1. Validate name (no /, \0, empty)
2. Generate folder_id = UUIDv4
3. IF parent_id provided:
   - GET parent folder (validate exists, ownership)
   - Compute path = parent.path + "/" + name
4. ELSE: path = "/" + name
5. CREATE folder doc at apps/rustshare/meta/folders/{folder_id}.json
6. UPDATE folder children index for parent (or user roots)
7. APPEND FolderCreated event
8. UPDATE runtime cache
```

### 5.2 Move File

```
1. GET file head (validate exists, ownership)
2. GET source parent folder
3. GET target folder (validate exists, ownership, not descendant)
4. COMPUTE new_path = target.path + "/" + file.name
5. ACQUIRE lease on file_id
6. ACQUIRE lease on source_parent_id
7. ACQUIRE lease on target_parent_id
8. PUT updated file head (with If-Match for version check)
9. UPDATE source folder children index (remove file)
10. UPDATE target folder children index (add file)
11. APPEND FileMoved event
12. INVALIDATE runtime cache for both folders
13. RELEASE leases
```

### 5.3 Delete File (Soft Delete with Tombstone)

```
1. GET file head (validate exists, ownership)
2. ACQUIRE lease on file_id
3. ACQUIRE lease on parent_folder_id
4. WRITE tombstone doc at apps/rustshare/meta/tombstones/files/{file_id}.json
5. MARK file head as deleted (or delete canonical doc)
6. UPDATE parent folder children index (mark deleted or remove)
7. APPEND FileDeleted event
8. INVALIDATE runtime cache
9. RELEASE leases
```

### 5.4 Create Share

```
1. GET resource (file or folder) - validate exists, ownership
2. GENERATE token (32-char alphanumeric)
3. HASH token for storage
4. CREATE share doc at apps/rustshare/meta/shares/{share_id}.json
5. UPDATE share by_token index
6. IF user share: UPDATE recipient's shared_with_me index
7. APPEND ShareCreated event
```

---

## 6. Migration Strategy

### 6.1 Configuration

```rust
pub enum MetadataBackend {
    Postgres,      // Legacy - read/write Postgres only
    RustFs,        // New - read/write RustFS only
    DualWrite,     // Migration - write both, read Postgres
    DualRead,      // Migration - write both, compare reads
    LocalFs,       // Dev - local filesystem backend
}
```

Environment variable: `RUSTSHARE_METADATA_BACKEND=postgres|rustfs|dual_write|dual_read|localfs`

### 6.2 Migration Stages

#### Stage 1: Add Object-Backed Layer (2-3 weeks)
- [ ] Implement versioned schemas (serde with schema_version)
- [ ] Implement RustFsMetadataBackend
- [ ] Implement LocalFsMetadataBackend
- [ ] Add feature flag support
- [ ] Keep Postgres as default read path
- **Verification:** Dual backend can be instantiated, no regression

#### Stage 2: Dual-Write (2-3 weeks)
- [ ] Implement DualWriteBackend
- [ ] Write all mutations to both Postgres and RustFS
- [ ] Add parity verification tool (compare canonical objects with SQL)
- [ ] Log discrepancies for analysis
- **Verification:** Both stores contain identical data

#### Stage 3: Read Path Migration - Low Risk (1-2 weeks)
- [ ] Switch file fetch to RustFS (with fallback)
- [ ] Switch folder listing to RustFS (with fallback)
- [ ] Switch share lookup to RustFS (with fallback)
- [ ] Monitor error rates, fallback frequency
- **Verification:** API tests pass, fallback rate < 0.1%

#### Stage 4: Full Read Migration (1-2 weeks)
- [ ] Switch all reads to RustFS
- [ ] Keep Postgres as compatibility shadow (writes only)
- [ ] Run verification tooling continuously
- **Verification:** Zero discrepancies over 7 days

#### Stage 5: PostgreSQL Deprecation (1 week)
- [ ] Remove dual-write
- [ ] Remove Postgres repository implementations
- [ ] Keep migration/export tooling
- **Verification:** System operates with Postgres disabled

---

## 7. Rebuild/Repair Tooling

### 7.1 CLI Commands

```bash
# Rebuild folder children indexes from metadata docs
rustshare-admin rebuild folder-children [--folder-id <id>]

# Rebuild per-user indexes
rustshare-admin rebuild user-indexes [--user-id <id>]

# Verify file heads vs version refs
rustshare-admin verify file-versions

# Verify folder parent/child consistency
rustshare-admin verify folder-hierarchy

# Verify tombstones and restore metadata
rustshare-admin verify tombstones

# Full namespace scan for migration
rustshare-admin scan namespace --output <path>

# Compare Postgres vs RustFS
rustshare-admin verify parity --sample-rate 0.01

# Rebuild runtime cache
rustshare-admin cache rebuild
```

### 7.2 Admin API Endpoints (Internal)

```
POST /api/admin/metadata/rebuild-indexes
POST /api/admin/metadata/verify-consistency
GET  /api/admin/metadata/stats
```

---

## 8. Testing Strategy

### 8.1 Backend Parity Tests

All tests run against:
- Postgres backend (during migration)
- RustFS backend
- LocalFS backend

```rust
#[async_trait]
trait MetadataBackendTest {
    async fn run_all_tests(&self);
}

// Tests: create/rename/move/delete/restore for files and folders
// Tests: share create/revoke
// Tests: version creation
// Tests: conflict handling
```

### 8.2 Crash/Failure Simulation

```rust
#[tokio::test]
async fn test_recover_from_failed_index_update() {
    // 1. Setup: Create file
    // 2. Simulate: Fail after metadata write, before index update
    // 3. Verify: Rebuild tool can recover
}
```

### 8.3 Concurrency Tests

```rust
#[tokio::test]
async fn test_concurrent_rename_conflict() {
    // Two concurrent rename attempts
    // One should succeed, one should fail with conflict
}
```

### 8.4 Migration Tests

```rust
#[tokio::test]
async fn test_dual_write_correctness() {
    // Write via DualWriteBackend
    // Read from both backends
    // Verify equality
}
```

---

## 9. Runtime Cache/Index Behavior

### 9.1 Cache Invalidation Rules

| Mutation | Cache Action |
|----------|-------------|
| File uploaded | Invalidate parent folder children |
| File renamed | Invalidate parent folder children, update entry |
| File moved | Invalidate source and target folder children |
| File deleted | Invalidate parent folder children |
| Folder created | Invalidate parent folder children |
| Folder renamed | Invalidate parent, update descendant paths |
| Folder moved | Invalidate source, target, and all descendants |
| Share created | Invalidate user's shared_with_me |
| Share revoked | Invalidate user's shared_with_me |

### 9.2 Cache Rebuild

- On startup: Lazy load (populate on first read)
- On demand: `rustshare-admin cache rebuild`
- Background: Optional periodic refresh

---

## 10. Local Filesystem Backend

### 10.1 Design

- Store metadata as sidecar `.json` files alongside content
- Use OS advisory locks (flock) for coordination
- Same logical schema and stable IDs as RustFS backend
- Same domain invariants enforced

### 10.2 Path Layout

```
local-storage/
  blobs/
    {hash}                    # File content
  meta/
    folders/
      {folder_id}.json
    files/
      {file_id}.json
    shares/
      {share_id}.json
  indexes/
    ...
```

---

## 11. Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| RustFS conditional write semantics insufficient | Low | High | Verify in code, implement lease fallback |
| Migration data inconsistency | Low | Critical | Dual-write verification, parity tooling |
| Performance regression | Medium | Medium | Runtime cache, load testing |
| Concurrent mutation conflicts | Medium | Medium | Optimistic concurrency, lease coordination |
| Rollback needed | Low | High | Keep Postgres shadow during migration |

---

## 12. Implementation Phases (Revised Timeline)

| Phase | Duration | Deliverables |
|-------|----------|--------------|
| 0. Design & Scaffolding | 1 week | This doc, trait definitions, module structure |
| 1. Schemas & Stores | 2 weeks | Versioned schemas, RustFS backend, LocalFS backend |
| 2. Coordination & Cache | 1 week | Lease coordination, runtime cache |
| 3. Dual-Write | 2 weeks | DualWriteBackend, parity tooling |
| 4. Read Migration | 2 weeks | Gradual read path switchover |
| 5. Cleanup | 1 week | Remove Postgres, finalize |
| **Total** | **9 weeks** | |

---

## 13. Appendix: Schema Versioning Strategy

### Forward Compatibility
- New code can read old schema versions
- Migration function: `old_version -> current_version`

### Backward Compatibility (during rollback)
- Old code should ignore unknown fields (serde default)
- New required fields must have sensible defaults

### Version Bumping Rules
- **Minor:** Add optional field (no bump needed)
- **Major:** Change semantics, add required field, remove field

---

*Document Version: 1.0*
*Date: 2026-03-27*
*Author: RustShare Architecture Team*
