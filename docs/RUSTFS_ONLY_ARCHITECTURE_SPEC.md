# RustShare RustFS-Only Per-User Bucket Architecture Specification

**Version:** 1.0  
**Status:** Draft  
**Date:** 2026-03-28  

---

## 1. Architecture Overview

### 1.1 Core Principles

1. **Every user has an isolated canonical bucket**  
   All durable data for a user lives in their dedicated bucket: `{USER_BUCKET_PREFIX}{user_uuid}`

2. **All durable state in RustFS**  
   No PostgreSQL, no SQLx, no local filesystem durable metadata. S3-compatible object storage is the only durable store.

3. **Redis is OPTIONAL**  
   - Standalone mode works without Redis  
   - Distributed mode uses Redis only for ephemeral coordination (locks, leases, presence)
   - Redis loss never destroys durable truth

4. **Owner/Recipient State Split**  
   - Owner side: canonical file/folder metadata, blobs, outbound share grants  
   - Recipient side: received share references, shared_with_me index, recipient favourites

5. **No Shared Global Bucket Dependency**  
   The shared blob bucket is removed. All blobs live in user buckets with content-addressed storage.

6. **Explicit Per-Request User Scoping**  
   No "system user" fallback in real request paths. Every authenticated request resolves the effective user identity and storage scope.

7. **Portable Storage Locators**  
   Cross-user references use versioned locators with explicit storage provider, bucket, and key information.

---

## 2. Bucket Layout

### 2.1 Bucket Naming

```
{USER_BUCKET_PREFIX}{user_uuid}

Example: rustshare-user-550e8400-e29b-41d4-a716-446655440000
```

Bucket naming rules are centralized in `UserBucketConfig::bucket_for_user()`.

### 2.2 Object Key Prefixes

Each user bucket contains the following structure:

```
identity/
  profile.json              # User profile document
  settings.json             # User settings/preferences

owned/
  folders/
    {folder-id}.json        # Canonical folder metadata
  files/
    {file-id}.json          # Canonical file metadata
  file_versions/
    {file-id}/
      {version-id}.json     # Version metadata
  blobs/
    sha256/
      {hash-prefix}/        # First 4 chars of hash
        {hash-suffix}       # Remaining hash bytes
  shares/
    outbound/
      {share-id}.json       # Outbound share grants
  tombstones/
    files/
      {file-id}.json        # Soft-deleted file records
    folders/
      {folder-id}.json      # Soft-deleted folder records

received/
  shares/
    {share-id}.json         # Received share references with portable locators

indexes/
  folders/
    {folder-id}/
      children.json         # Folder children index (files + subfolders)
  owned/
    roots.json              # Root-level files and folders
    recent.json             # Recently modified items
    favourites.json         # User's favourited items
  received/
    shared_with_me.json     # All received shares
    favourites.json         # Favourited received items
  notifications.json        # User notification list
  devices.json              # Registered devices
  groups.json               # Group memberships

lookups/
  public_share_tokens/
    {token-hash}.json       # Public share token -> share locator
  pairing_codes/
    {code}.json             # Pairing code -> pairing request
  pairing_tokens/
    {token-hash}.json       # Token hash -> device

jobs/
  {job-id}.json             # Async job records

events/
  {YYYY}/
    {MM}/
      {DD}/
        {event-id}.json     # Event log entries

audit/
  {YYYY}/
    {MM}/
      {DD}/
        {audit-id}.json     # Audit log entries

config/
  app.json                  # App-level config (admin only)
  webhooks/
    {webhook-id}.json       # Webhook configurations
```

---

## 3. JSON Document Schemas

### 3.1 Core Documents

#### FileDocV2 (owned/files/{file-id}.json)

```json
{
  "schema_version": 2,
  "id": "uuid",
  "owner_id": "uuid",
  "parent_folder_id": "uuid | null",
  "name": "string",
  "path": "/full/path/to/file",
  "current_version_id": "uuid",
  "version_number": 1,
  "size": 12345,
  "mime_type": "application/pdf",
  "content_hash": "sha256-hex",
  "deleted": false,
  "created_at": "2026-03-28T00:00:00Z",
  "updated_at": "2026-03-28T00:00:00Z"
}
```

#### FolderDocV2 (owned/folders/{folder-id}.json)

```json
{
  "schema_version": 2,
  "id": "uuid",
  "owner_id": "uuid",
  "parent_folder_id": "uuid | null",
  "name": "string",
  "path": "/full/path/to/folder",
  "deleted": false,
  "version": 1,
  "created_at": "2026-03-28T00:00:00Z",
  "updated_at": "2026-03-28T00:00:00Z"
}
```

#### FileVersionDocV2 (owned/file_versions/{file-id}/{version-id}.json)

```json
{
  "schema_version": 2,
  "id": "uuid",
  "file_id": "uuid",
  "version_number": 1,
  "size": 12345,
  "content_hash": "sha256-hex",
  "storage_key": "blobs/sha256/ab/cd/abcdef...",
  "created_by": "uuid",
  "created_at": "2026-03-28T00:00:00Z"
}
```

### 3.2 Share Documents

#### OutboundShareDocV2 (owned/shares/outbound/{share-id}.json)

```json
{
  "schema_version": 2,
  "id": "uuid",
  "resource_type": "file | folder",
  "resource_id": "uuid",
  "resource_locator": {
    "locator_version": 1,
    "storage_alias": "rustfs",
    "bucket": "rustshare-user-{owner-uuid}",
    "key": "owned/files/{file-id}.json",
    "resource_type": "file",
    "resource_id": "uuid"
  },
  "shared_with_user_id": "uuid",
  "permissions": "read | write | admin",
  "created_at": "2026-03-28T00:00:00Z",
  "expires_at": "2026-04-28T00:00:00Z | null"
}
```

#### ReceivedShareDocV2 (received/shares/{share-id}.json)

```json
{
  "schema_version": 2,
  "id": "uuid",
  "share_id": "uuid",
  "resource_type": "file | folder",
  "resource_locator": {
    "locator_version": 1,
    "storage_alias": "rustfs",
    "bucket": "rustshare-user-{owner-uuid}",
    "key": "owned/files/{file-id}.json",
    "resource_type": "file",
    "resource_id": "uuid"
  },
  "permissions": "read | write | admin",
  "shared_by": "uuid",
  "owner_user_id": "uuid",
  "created_at": "2026-03-28T00:00:00Z",
  "expires_at": "2026-04-28T00:00:00Z | null"
}
```

### 3.3 Index Documents

#### FolderChildrenIndex (indexes/folders/{folder-id}/children.json)

```json
{
  "schema_version": 2,
  "folder_id": "uuid",
  "files": [
    {
      "id": "uuid",
      "name": "string",
      "updated_at": "2026-03-28T00:00:00Z",
      "deleted": false
    }
  ],
  "folders": [
    {
      "id": "uuid",
      "name": "string",
      "updated_at": "2026-03-28T00:00:00Z",
      "deleted": false
    }
  ],
  "updated_at": "2026-03-28T00:00:00Z"
}
```

#### UserRootsIndex (indexes/owned/roots.json)

```json
{
  "schema_version": 2,
  "files": [{"id": "uuid", "name": "string", "updated_at": "timestamp"}],
  "folders": [{"id": "uuid", "name": "string", "updated_at": "timestamp"}],
  "updated_at": "2026-03-28T00:00:00Z"
}
```

#### FavouritesIndex (indexes/owned/favourites.json)

```json
{
  "schema_version": 2,
  "entries": [
    {
      "resource_id": "uuid",
      "resource_type": "owned_file | owned_folder | received_file | received_folder",
      "added_at": "2026-03-28T00:00:00Z"
    }
  ],
  "updated_at": "2026-03-28T00:00:00Z"
}
```

#### SharedWithMeIndex (indexes/received/shared_with_me.json)

```json
{
  "schema_version": 2,
  "shares": [
    {
      "share_id": "uuid",
      "resource_type": "file | folder",
      "resource_locator": { /* PortableStorageLocator */ },
      "shared_by": "uuid",
      "permissions": "read | write | admin",
      "received_at": "2026-03-28T00:00:00Z"
    }
  ],
  "updated_at": "2026-03-28T00:00:00Z"
}
```

### 3.4 Portable Storage Locator

```json
{
  "locator_version": 1,
  "storage_alias": "rustfs",
  "bucket": "rustshare-user-{uuid}",
  "key": "owned/files/{file-id}.json",
  "resource_type": "file | folder | share",
  "resource_id": "uuid",
  "version_id": "uuid | null",
  "checksum": "sha256-hex | null"
}
```

---

## 4. Mutation Protocols

### 4.1 Upload File

**Documents Written:**
1. Blob: `owned/blobs/sha256/{hash-prefix}/{hash}` (if not exists)
2. File: `owned/files/{file-id}.json`
3. Version: `owned/file_versions/{file-id}/{version-id}.json`
4. Folder Children Index: `indexes/folders/{parent-id}/children.json` (if parent exists)
5. User Roots Index: `indexes/owned/roots.json` (if no parent)

**Events:**
- `FileUploaded` event appended to event log

**Audit:**
- Audit record for file creation

### 4.2 Create Folder

**Documents Written:**
1. Folder: `owned/folders/{folder-id}.json`
2. Parent Folder Children Index: `indexes/folders/{parent-id}/children.json` (if parent exists)
3. User Roots Index: `indexes/owned/roots.json` (if no parent)

**Events:**
- `FolderCreated` event appended

### 4.3 Rename File

**Documents Updated:**
1. File: `owned/files/{file-id}.json` (name, path, updated_at)
2. Source Folder Children Index: `indexes/folders/{old-parent}/children.json`
3. Source Folder Children Index: `indexes/folders/{new-parent}/children.json` (if parent changed)

**Events:**
- `FileRenamed` event appended

### 4.4 Move File

**Documents Updated:**
1. File: `owned/files/{file-id}.json` (parent_folder_id, path, updated_at)
2. Source Folder Children Index: `indexes/folders/{old-parent}/children.json`
3. Target Folder Children Index: `indexes/folders/{new-parent}/children.json`
4. User Roots Index: `indexes/owned/roots.json` (if moving to/from root)

**Events:**
- `FileMoved` event appended

### 4.5 Delete File

**Documents Updated:**
1. File: `owned/files/{file-id}.json` (deleted: true)
2. Tombstone: `tombstones/files/{file-id}.json` (for recovery)
3. Folder Children Index: `indexes/folders/{parent-id}/children.json`

**Events:**
- `FileDeleted` event appended

### 4.6 Restore File

**Documents Updated:**
1. File: `owned/files/{file-id}.json` (deleted: false)
2. Tombstone: Deleted
3. Folder Children Index: Restored

**Events:**
- `FileRestored` event appended

### 4.7 Create Share (Internal)

**Owner Side (Sharer):**
1. Outbound Share: `owned/shares/outbound/{share-id}.json`

**Recipient Side:**
1. Received Share: `received/shares/{share-id}.json`
2. Shared With Me Index: `indexes/received/shared_with_me.json`

**Events:**
- `ShareCreated` event on owner side
- `ShareReceived` notification on recipient side

### 4.8 Revoke Share

**Owner Side:**
1. Outbound Share: Deleted or marked revoked

**Recipient Side:**
1. Received Share: Deleted or marked revoked
2. Shared With Me Index: Updated

**Events:**
- `ShareRevoked` event

### 4.9 Star Item (Owned)

**Documents Updated:**
1. Favourites Index: `indexes/owned/favourites.json`

**Note:** Canonical file document is NOT modified.

### 4.10 Star Item (Received Share)

**Documents Updated:**
1. Received Favourites Index: `indexes/received/favourites.json`

**Note:** Owner's file document is NOT modified.

### 4.11 Provision User Bucket

**Executed On:** User creation/signup

**Actions:**
1. Create bucket if not exists
2. Initialize empty indexes:
   - `indexes/owned/roots.json`
   - `indexes/owned/favourites.json`
   - `indexes/received/shared_with_me.json`
   - `indexes/received/favourites.json`
   - `indexes/notifications.json`
   - `indexes/devices.json`

**Failure Behavior:**
- Provisioning failure fails the signup operation
- Idempotent: repeated calls succeed if bucket exists

---

## 5. Request Scoping Rules

### 5.1 Authenticated Request Resolution

1. Extract JWT token from Authorization header or session cookie
2. Validate token and extract `user_id` claim
3. Use `user_id` as the storage scope for all operations

### 5.2 Handler Responsibilities

**Owned Operations:**
```rust
// Handler uses authenticated user's bucket
let file = file_service.get_file(auth.user_id, file_id).await?;
```

**Cross-User Operations (Shares):**
```rust
// Share creation writes to both owner and recipient buckets
share_service.create_share(
    owner_id: auth.user_id,  // Authenticated user is owner
    recipient_id: request.recipient_id,
    ...
).await?;
```

**Recipient Operations:**
```rust
// List received shares from recipient's bucket
let shares = share_service.list_received_shares(auth.user_id).await?;
```

### 5.3 Forbidden Patterns

- ❌ No `SYSTEM_USER_ID` fallback
- ❌ No hard-coded bucket names in handlers
- ❌ No shared bucket assumptions for blobs

---

## 6. Service Architecture

### 6.1 Layer Structure

```
HTTP Handlers (axum)
    ↓
V2 Services (business logic)
    ↓
UserBucketStore (per-user operations)
    ↓
S3Client (aws-sdk-s3)
    ↓
RustFS (S3-compatible)
```

### 6.2 Key Services

#### FileServiceV2
- `upload_file(owner_id, name, parent_id, content, mime_type) -> File`
- `get_file(user_id, file_id) -> File`
- `list_files(user_id, folder_id) -> Vec<File>`
- `rename_file(user_id, file_id, new_name)`
- `move_file(user_id, file_id, new_parent_id)`
- `delete_file(user_id, file_id)`
- `restore_file(user_id, file_id)`
- `list_versions(user_id, file_id) -> Vec<FileVersion>`

#### FolderServiceV2
- `create_folder(owner_id, name, parent_id) -> Folder`
- `get_folder(user_id, folder_id) -> Folder`
- `list_children(user_id, folder_id) -> (Vec<Folder>, Vec<File>)`
- `rename_folder(user_id, folder_id, new_name)`
- `move_folder(user_id, folder_id, new_parent_id)`
- `delete_folder(user_id, folder_id)`
- `restore_folder(user_id, folder_id)`

#### ShareServiceV2
- `create_share(owner_id, resource_id, recipient_id, permissions) -> Share`
- `revoke_share(user_id, share_id)`
- `list_outbound_shares(user_id) -> Vec<Share>`
- `list_received_shares(user_id) -> Vec<ReceivedShare>`
- `get_shared_resource(user_id, share_id) -> Resource`

#### FavouriteServiceV2
- `star_owned_file(user_id, file_id)`
- `star_owned_folder(user_id, folder_id)`
- `star_received_share(user_id, share_id)`
- `unstar(user_id, resource_id)`
- `list_favourites(user_id) -> Vec<Favourite>`

#### UserBucketProvisioningService
- `provision_user_bucket(user_id) -> Result<(), ProvisionError>`
- `is_bucket_provisioned(user_id) -> bool`

---

## 7. Redis Optionality Model

### 7.1 Standalone Mode (No Redis)

- In-process coordination only
- No distributed locking
- WebSocket fanout through in-memory broadcaster
- Job coordination through in-memory queue

### 7.2 Distributed Mode (With Redis)

**Allowed Uses:**
- Short-term locks/leases (e.g., upload coordination)
- Worker claim coordination
- WebSocket presence tracking
- Rate limiting
- Session revocation cache
- Idempotency keys

**Forbidden Uses:**
- Canonical file/folder metadata
- Durable favourites
- Durable shared_with_me
- Durable notifications
- Durable jobs
- Durable events

---

## 8. No-Compatibility Rules

### 8.1 Removed Components

| Component | Status | Replacement |
|-----------|--------|-------------|
| PostgreSQL | REMOVED | RustFS-only |
| SQLx | REMOVED | aws-sdk-s3 |
| LocalFsDocumentStore | REMOVED | UserScopedDocumentStore |
| Shared global blob bucket | REMOVED | Per-user blobs |
| System user fallback | REMOVED | Explicit user scoping |

### 8.2 Code Patterns to Eliminate

- ❌ `ObjectStore` (shared bucket)
- ❌ `LocalFsDocumentStore`
- ❌ `SYSTEM_USER_ID` constants in request paths
- ❌ Direct SQL/x queries
- ❌ Local filesystem metadata paths
- ❌ Shared bucket assumptions

---

## 9. Testing Requirements

### 9.1 Contract Tests

All contract tests must:
1. Use in-memory or test-container S3 storage
2. Fail against stub/transitional implementations
3. Pass only against real implementations

### 9.2 Required Test Coverage

1. **Bucket Isolation**: Verify data goes to correct user bucket
2. **File Lifecycle**: Upload, rename, move, delete, restore
3. **Folder Lifecycle**: Create, rename, move, delete, restore
4. **Share Dual-Write**: Owner and recipient both have documents
5. **Favourites Isolation**: User A starring doesn't affect User B
6. **Portable Locators**: Cross-bucket references work
7. **Request Scoping**: Authenticated requests use correct bucket
8. **Bucket Provisioning**: User creation provisions bucket
9. **Redis Optionality**: Tests pass with and without Redis
10. **Stub Elimination**: No fake empty results

---

## 10. Implementation Phases

### Phase 1: Repository Audit ✓
- Identify all remaining stubbed handlers
- Identify transitional stores
- Identify shared-bucket dependencies

### Phase 2: Finalize Schemas
- Lock document schemas
- Define portable storage locator format
- Finalize bucket layout constants

### Phase 3: Remove Shared Bucket Blob Dependency
- Convert FileService to per-user blob storage
- Convert ThumbnailService to per-user storage
- Remove ObjectStore dependency from handlers

### Phase 4: Per-Request User Scoping
- Implement explicit scoping through handlers
- Remove system-user fallback
- Update extractors

### Phase 5: Share Dual-Write Logic
- Implement owner/recipient document creation
- Implement shared_with_me projection
- Implement favourites for owned and received items

### Phase 6: Stateful Flows
- Device registration/pairing
- Notifications
- Jobs
- Admin handlers

### Phase 7: Cleanup
- Remove dead compatibility code
- Validate zero-PostgreSQL runtime
- Validate zero-local-durable-metadata

---

## Appendix A: Bucket Layout Constants

```rust
pub const PREFIX_OWNED: &str = "owned";
pub const PREFIX_RECEIVED: &str = "received";
pub const PREFIX_INDEXES: &str = "indexes";
pub const PREFIX_LOOKUPS: &str = "lookups";
pub const PREFIX_JOBS: &str = "jobs";
pub const PREFIX_EVENTS: &str = "events";
pub const PREFIX_AUDIT: &str = "audit";
pub const PREFIX_CONFIG: &str = "config";
pub const PREFIX_TOMBSTONES: &str = "tombstones";
pub const PREFIX_IDENTITY: &str = "identity";

pub const SUBPREFIX_FOLDERS: &str = "folders";
pub const SUBPREFIX_FILES: &str = "files";
pub const SUBPREFIX_FILE_VERSIONS: &str = "file_versions";
pub const SUBPREFIX_BLOBS: &str = "blobs/sha256";
pub const SUBPREFIX_SHARES: &str = "shares";
pub const SUBPREFIX_SHARES_OUTBOUND: &str = "shares/outbound";
pub const SUBPREFIX_SHARES_RECEIVED: &str = "received/shares";
```

---

## Appendix B: Error Handling

All services return explicit error types:

```rust
pub enum StorageError {
    NotFound(Uuid),
    AlreadyExists(String),
    PermissionDenied,
    InvalidName(String),
    ParentNotFound(Uuid),
    Storage(String),
    Serialization(String),
}
```

---

END OF SPECIFICATION
