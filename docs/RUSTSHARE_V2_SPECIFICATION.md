# RustShare V2 Specification
## Object-Store-Native Architecture with Per-User Bucket Isolation

**Version:** 2.0.0  
**Date:** 2026-03-27  
**Status:** DRAFT

---

## 1. Executive Summary

This specification defines a complete redesign of RustShare using an object-store-native architecture where:

- All durable application data lives in S3-compatible object storage
- Each user has an isolated User Storage Domain (USD) - implemented as a dedicated bucket
- No PostgreSQL or SQLx dependencies exist
- Redis is optional and used only for ephemeral runtime coordination
- User data can be restored independently without a central metadata repository

---

## 2. Architectural Principles

### 2.1 User Storage Domain (USD)

A User Storage Domain is the durable, portable storage unit containing a user's RustShare state.

**Properties:**
- Each USD is an isolated S3-compatible bucket
- Bucket naming: `rustshare-user-{user-id}`
- All canonical user state lives within their USD
- USDs are portable to other S3-compatible backends

### 2.2 Canonical State Definition

**Canonical user state** (must survive restart, must be in user's bucket):
- User profile and settings
- Owned files metadata and content references
- Owned folders metadata
- File versions metadata
- Outbound share grants
- Tombstones for deleted items

**Durable derived state** (rebuildable but persisted for performance):
- Folder children indexes
- User roots index
- Shared-with-me index (recipient-side)
- Favourites/stars index
- Notification projections

**Ephemeral runtime state** (may use Redis, not required for restore):
- Session tokens
- Device pairing coordination
- Job leasing
- Rate limiting counters
- Presence indicators

### 2.3 Owner vs Recipient Responsibilities

**Owner Side (in owner's USD):**
- Canonical resource metadata (files, folders)
- Canonical share grant metadata (outbound shares)
- Content blob references
- Version history

**Recipient Side (in recipient's USD):**
- Durable share reference (points to owner's resource)
- Shared-with-me index
- Favourites on shared items
- Recipient-side notification state

### 2.4 Portable Storage Locator (PSL)

Cross-user references MUST use Portable Storage Locators, not hard-coded bucket assumptions.

**PSL Schema (v1):**
```json
{
  "locator_version": 1,
  "storage_provider_kind": "s3",
  "endpoint_ref": "primary",           // alias defined in config
  "bucket": "rustshare-user-{user-id}",
  "key": "owned/files/{file-id}.json",
  "resource_type": "file",
  "resource_id": "uuid",
  "version_id": null,                  // optional
  "content_hash": "sha256:..."         // for verification
}
```

---

## 3. Bucket Layout Specification

### 3.1 Per-User Bucket Structure

```
rustshare-user-{user-id}
│
├── identity/
│   ├── profile.json
│   └── settings.json
│
├── owned/
│   ├── folders/
│   │   └── {folder-id}.json
│   ├── files/
│   │   └── {file-id}.json
│   ├── file_versions/
│   │   └── {file-id}/
│   │       └── {version-id}.json
│   ├── shares/
│   │   └── outbound/
│   │       └── {share-id}.json
│   └── tombstones/
│       ├── files/
│       │   └── {file-id}.json
│       └── folders/
│           └── {folder-id}.json
│
├── received/
│   └── shares/
│       └── {share-id}.json          # Local reference to shared resource
│
├── indexes/
│   ├── folders/
│   │   └── {folder-id}/
│   │       └── children.json
│   ├── owned/
│   │   ├── roots.json
│   │   ├── recent.json
│   │   └── favourites.json
│   └── received/
│       ├── shared_with_me.json
│       └── favourites.json
│
├── lookups/
│   └── public_share_tokens/
│       └── {token-hash}.json
│
├── events/
│   └── {yyyy}/{mm}/{dd}/
│       └── {event-id}.json
│
├── audit/
│   └── {yyyy}/{mm}/{dd}/
│       └── {audit-id}.json
│
└── config/
    └── webhooks/
        └── {webhook-id}.json
```

### 3.2 Shared Content Blob Bucket (Optional)

Content blobs may be stored in a shared bucket for deduplication:

```
rustshare-blobs
└── sha256/
    └── {ab}/
        └── {cd}/
            └── {full-hash}
```

**Note:** Blobs are immutable and content-addressed. References from user buckets use the content hash, enabling blob portability.

---

## 4. Document Schemas

### 4.1 User Identity Document

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserIdentityDocument {
    pub schema_version: u32,           // = 1
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub password_hash: String,         // Argon2
    pub is_admin: bool,
    pub disabled: bool,
    pub storage_quota_bytes: i64,
    pub theme: String,                 // "light", "dark", "system"
    pub email_verified_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: u64,                  // Optimistic concurrency
}
```

**Storage:** `identity/profile.json`

### 4.2 User Settings Document

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSettingsDocument {
    pub schema_version: u32,           // = 1
    pub user_id: Uuid,
    pub notification_preferences: NotificationPreferences,
    pub default_share_permission: SharePermission,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

**Storage:** `identity/settings.json`

### 4.3 Folder Document

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderDocument {
    pub schema_version: u32,           // = 1
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub path: String,                  // Computed, for convenience
    pub owner_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: u64,
    pub deleted: bool,
}
```

**Storage:** `owned/folders/{folder-id}.json`

### 4.4 File Document

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDocument {
    pub schema_version: u32,           // = 1
    pub id: Uuid,
    pub parent_id: Option<Uuid>,      // Folder ID or None for root
    pub name: String,
    pub path: String,                  // Computed
    pub owner_id: Uuid,
    pub current_version_id: Uuid,
    pub version_number: i32,
    pub size: i64,
    pub mime_type: String,
    pub content_ref: String,           // "sha256:{hash}"
    pub checksum: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: u64,
    pub deleted: bool,
}
```

**Storage:** `owned/files/{file-id}.json`

### 4.5 File Version Document

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileVersionDocument {
    pub schema_version: u32,           // = 1
    pub id: Uuid,
    pub file_id: Uuid,
    pub version_number: i32,
    pub content_ref: String,           // "sha256:{hash}"
    pub size: i64,
    pub checksum: String,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub change_description: Option<String>,
}
```

**Storage:** `owned/file_versions/{file-id}/{version-id}.json`

### 4.6 Outbound Share Document (in owner's bucket)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundShareDocument {
    pub schema_version: u32,           // = 1
    pub id: Uuid,
    pub resource_type: String,         // "file" or "folder"
    pub resource_id: Uuid,
    pub scope: ShareScope,             // Public or User
    pub permissions: SharePermission,
    pub token_hash: Option<String>,    // For public shares
    pub recipient_user_id: Option<Uuid>, // For user shares
    pub password_hash: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub upload_only: bool,
    pub access_count: i32,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub version: u64,
}
```

**Storage:** `owned/shares/outbound/{share-id}.json`

### 4.7 Received Share Reference (in recipient's bucket)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceivedShareReference {
    pub schema_version: u32,           // = 1
    pub id: Uuid,                      // Local reference ID
    pub share_id: Uuid,                // Original share ID
    pub owner_user_id: Uuid,
    pub resource_locator: PortableStorageLocator,
    pub resource_name: String,
    pub resource_type: String,
    pub permissions: SharePermission,
    pub shared_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub hidden: bool,                  // User can hide from UI
    pub version: u64,
}
```

**Storage:** `received/shares/{share-id}.json`

### 4.8 Portable Storage Locator

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortableStorageLocator {
    pub locator_version: u32,          // = 1
    pub storage_provider_kind: String, // "s3", "gcs", "azure"
    pub endpoint_ref: String,          // Alias like "primary", "eu-west"
    pub bucket: String,
    pub key: String,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub version_id: Option<String>,
    pub content_hash: Option<String>,
}
```

### 4.9 Favourite Entry

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FavouriteEntry {
    pub id: Uuid,                      // Favourite entry ID
    pub resource_type: String,         // "file" or "folder"
    pub resource_id: Uuid,
    pub resource_locator: Option<PortableStorageLocator>, // For shared items
    pub starred_at: DateTime<Utc>,
    pub notes: Option<String>,
}
```

### 4.10 Favourites Index

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FavouritesIndex {
    pub schema_version: u32,           // = 1
    pub user_id: Uuid,
    pub version: u64,
    pub updated_at: DateTime<Utc>,
    pub owned_entries: Vec<FavouriteEntry>,
    pub shared_entries: Vec<FavouriteEntry>,
}
```

**Storage:**
- Owned favourites: `indexes/owned/favourites.json`
- Shared favourites: `indexes/received/favourites.json`

### 4.11 Shared With Me Index

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedWithMeIndex {
    pub schema_version: u32,           // = 1
    pub user_id: Uuid,
    pub version: u64,
    pub updated_at: DateTime<Utc>,
    pub entries: Vec<ReceivedShareReference>,
}
```

**Storage:** `indexes/received/shared_with_me.json`

### 4.12 Folder Children Index

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderChildEntry {
    pub id: Uuid,
    pub kind: String,                  // "file" or "folder"
    pub name: String,
    pub deleted: bool,
    pub size: Option<i64>,             // For files
    pub mime: Option<String>,          // For files
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderChildrenIndex {
    pub schema_version: u32,           // = 1
    pub folder_id: Uuid,
    pub version: u64,
    pub updated_at: DateTime<Utc>,
    pub children: Vec<FolderChildEntry>,
}
```

**Storage:** `indexes/folders/{folder-id}/children.json`

### 4.13 Tombstone Document

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TombstoneDocument {
    pub schema_version: u32,           // = 1
    pub id: Uuid,                      // Same as resource ID
    pub resource_type: String,         // "file" or "folder"
    pub resource_id: Uuid,
    pub deleted_at: DateTime<Utc>,
    pub deleted_by: Uuid,
    pub previous_parent_id: Option<Uuid>,
    pub original_path: Option<String>,
    pub restore_data: serde_json::Value, // Full serialized original
}
```

**Storage:**
- Files: `owned/tombstones/files/{file-id}.json`
- Folders: `owned/tombstones/folders/{folder-id}.json`

### 4.14 Event Document

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDocument {
    pub schema_version: u32,           // = 1
    pub id: Uuid,
    pub event_type: EventType,
    pub actor_id: Uuid,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: Option<Uuid>,
    pub payload: serde_json::Value,
}
```

**Event Types:**
- `file_uploaded`
- `file_modified`
- `file_moved`
- `file_renamed`
- `file_deleted`
- `file_restored`
- `folder_created`
- `folder_moved`
- `folder_renamed`
- `folder_deleted`
- `folder_restored`
- `share_created`
- `share_revoked`
- `share_accepted`
- `resource_starred`
- `resource_unstarred`

**Storage:** `events/{yyyy}/{mm}/{dd}/{event-id}.json`

---

## 5. Mutation Protocols

### 5.1 Upload File

**Input:** owner_id, name, parent_folder_id, content, mime_type

**Protocol:**
1. Validate file name (no /, no \0, not empty)
2. Calculate SHA256 hash of content
3. Store blob in shared blob bucket (if not exists): `sha256/{ab}/{cd}/{hash}`
4. Create FileDocument with version=1
5. Create FileVersionDocument (version 1)
6. Store FileDocument: `owned/files/{file-id}.json`
7. Store FileVersionDocument: `owned/file_versions/{file-id}/{version-id}.json`
8. Update FolderChildrenIndex for parent folder
9. Write EventDocument: `file_uploaded`
10. Write AuditLog entry

**Failure Recovery:**
- If step 3-7 fail: Delete any written documents
- Blob is immutable and deduplicated - safe to leave

### 5.2 Create Folder

**Input:** owner_id, name, parent_folder_id

**Protocol:**
1. Validate folder name
2. Verify parent exists (if provided)
3. Compute path from parent
4. Create FolderDocument
5. Store: `owned/folders/{folder-id}.json`
6. Update FolderChildrenIndex for parent
7. Write EventDocument: `folder_created`
8. Update UserRootsIndex if no parent

### 5.3 Rename File

**Input:** file_id, new_name, user_id

**Protocol:**
1. Load FileDocument
2. Verify ownership
3. Update name and path
4. Bump version
5. Store updated FileDocument
6. Update FolderChildrenIndex for parent
7. Write EventDocument: `file_renamed`

### 5.4 Move File

**Input:** file_id, target_folder_id, user_id

**Protocol:**
1. Load FileDocument
2. Verify ownership
3. Load target folder (if not root)
4. Update parent_id and path
5. Bump version
6. Store updated FileDocument
7. Update FolderChildrenIndex for old parent (remove)
8. Update FolderChildrenIndex for new parent (add)
9. Write EventDocument: `file_moved`

### 5.5 Delete File

**Input:** file_id, user_id

**Protocol:**
1. Load FileDocument
2. Verify ownership
3. Create TombstoneDocument from FileDocument
4. Store tombstone: `owned/tombstones/files/{file-id}.json`
5. Mark FileDocument as deleted
6. Store updated FileDocument
7. Update FolderChildrenIndex (mark deleted)
8. Write EventDocument: `file_deleted`
9. **Do NOT delete blob** (content-addressed, may be shared)

### 5.6 Restore File

**Input:** file_id, user_id

**Protocol:**
1. Load TombstoneDocument
2. Verify deleted_by == user_id
3. Restore FileDocument from restore_data
4. Clear deleted flag
5. Store restored FileDocument
6. Delete tombstone (or mark restored)
7. Update FolderChildrenIndex (unmark deleted)
8. Write EventDocument: `file_restored`

### 5.7 Create Share (User-to-User)

**Input:** resource_id, resource_type, recipient_user_id, permissions, owner_id

**Protocol (Owner Side):**
1. Verify resource exists and is owned
2. Create OutboundShareDocument
3. Store: `owned/shares/outbound/{share-id}.json`
4. Write EventDocument: `share_created`

**Protocol (Recipient Side):**
1. Create PortableStorageLocator pointing to owner's resource
2. Create ReceivedShareReference
3. Store: `received/shares/{share-id}.json`
4. Update SharedWithMeIndex
5. Write EventDocument: `share_accepted` (when accepted)

### 5.8 Revoke Share

**Input:** share_id, owner_id

**Protocol (Owner Side):**
1. Load OutboundShareDocument
2. Verify ownership
3. Set revoked_at timestamp
4. Store updated document
5. Write EventDocument: `share_revoked`

**Protocol (Recipient Side - Eventual):**
1. On next sync or access, detect revoked share
2. Mark ReceivedShareReference as revoked
3. Update SharedWithMeIndex

### 5.9 Star Resource (Owned)

**Input:** resource_id, resource_type, user_id

**Protocol:**
1. Load FavouritesIndex for user
2. Check if already starred
3. Add FavouriteEntry with resource details
4. Store updated FavouritesIndex
5. Write EventDocument: `resource_starred`

### 5.10 Star Resource (Shared)

**Input:** resource_id, resource_type, share_id, user_id

**Protocol:**
1. Load ReceivedShareReference
2. Verify share is active
3. Load FavouritesIndex (shared entries)
4. Add FavouriteEntry with PortableStorageLocator
5. Store updated FavouritesIndex
6. Write EventDocument: `resource_starred`

**Important:** Owner's canonical file document is NOT modified.

### 5.11 Unstar Resource

**Input:** favourite_entry_id, user_id

**Protocol:**
1. Load FavouritesIndex
2. Remove matching entry
3. Store updated FavouritesIndex
4. Write EventDocument: `resource_unstarred`

---

## 6. Read Path Rules

### 6.1 Folder Contents Listing

**Must NOT require bucket-wide scan.**

**Implementation:**
1. Load FolderChildrenIndex: `indexes/folders/{folder-id}/children.json`
2. Return entries where deleted == false
3. Sort by name

**Index Rebuild (if missing):**
1. Scan `owned/files/` and `owned/folders/` for children of folder
2. Build index
3. Store for future use

### 6.2 Shared With Me Listing

**Must use recipient-side index.**

**Implementation:**
1. Load SharedWithMeIndex: `indexes/received/shared_with_me.json`
2. Filter out hidden entries
3. Enrich with current resource names (from locators if needed)
4. Return sorted by shared_at desc

### 6.3 Favourites Listing

**Implementation:**
1. Load FavouritesIndex: `indexes/owned/favourites.json` and `indexes/received/favourites.json`
2. Merge and sort by starred_at desc
3. For shared entries, resolve current metadata via locator if stale

### 6.4 File Metadata Fetch

**Implementation:**
1. Load FileDocument: `owned/files/{file-id}.json`
2. Verify ownership (owner_id == user_id) or share access
3. Return document

### 6.5 Public Share Lookup

**Implementation:**
1. Load lookup: `lookups/public_share_tokens/{token-hash}.json`
2. Get share_id
3. Load OutboundShareDocument from owner's bucket (requires cross-bucket read or cached copy)
4. Validate not revoked/expired
5. Return share info

---

## 7. Portability and Restore Model

### 7.1 User Bucket Export

To export a user's data:

1. List all objects in `rustshare-user-{user-id}` bucket
2. Download all JSON documents
3. Download all referenced blobs (or note content hashes for re-fetch)
4. Create export manifest with:
   - User ID
   - Export timestamp
   - List of all resources
   - List of all shares (outbound and received)
   - Schema versions

### 7.2 User Bucket Restore

To restore a user from their bucket:

1. Verify bucket structure
2. Load UserIdentityDocument
3. Restore all canonical documents (files, folders, versions, shares)
4. Rebuild derived indexes from canonical documents:
   - Folder children indexes
   - User roots index
   - Shared-with-me index (from received shares)
   - Favourites indexes
5. Verify blob references (blobs may need re-fetching if in separate bucket)
6. Resume operation

**Critical:** Restore must NOT require access to a central RustShare metadata repository.

### 7.3 Cross-User Reference Survival

When a user's bucket is moved to a new backend:

1. PortableStorageLocators in received shares remain valid
2. The `endpoint_ref` field can be remapped to new endpoint
3. Content hashes enable verification after move
4. Shares continue to work if:
   - Owner's bucket is also accessible, OR
   - Resource was copied to recipient's bucket (for offline access)

---

## 8. Storage Layer Abstractions

### 8.1 UserBucketStore

```rust
#[async_trait]
pub trait UserBucketStore: Send + Sync {
    /// Get the bucket name for a user
    fn bucket_for_user(&self, user_id: UserId) -> String;
    
    /// Check if user's bucket exists
    async fn bucket_exists(&self, user_id: UserId) -> Result<bool>;
    
    /// Create user's bucket
    async fn create_bucket(&self, user_id: UserId) -> Result<()>;
    
    /// Get object from user's bucket
    async fn get_object(&self, user_id: UserId, key: &str) -> Result<Option<Bytes>>;
    
    /// Put object to user's bucket
    async fn put_object(&self, user_id: UserId, key: &str, data: Bytes) -> Result<()>;
    
    /// Delete object from user's bucket
    async fn delete_object(&self, user_id: UserId, key: &str) -> Result<()>;
    
    /// List objects with prefix in user's bucket
    async fn list_objects(&self, user_id: UserId, prefix: &str) -> Result<Vec<String>>;
}
```

### 8.2 CrossBucketReader

```rust
#[async_trait]
pub trait CrossBucketReader: Send + Sync {
    /// Read object from another user's bucket using locator
    async fn read_with_locator(&self, locator: &PortableStorageLocator) -> Result<Option<Bytes>>;
    
    /// Check if locator is accessible
    async fn check_locator(&self, locator: &PortableStorageLocator) -> Result<bool>;
}
```

### 8.3 CoordinationStore (Redis Optional)

```rust
#[async_trait]
pub trait CoordinationStore: Send + Sync {
    /// Acquire lease for operation
    async fn acquire_lease(&self, key: &str, ttl_secs: u64) -> Result<Lease>;
    
    /// Release lease
    async fn release_lease(&self, lease: Lease) -> Result<()>;
    
    /// Publish event for fanout
    async fn publish(&self, channel: &str, message: &str) -> Result<()>;
    
    /// Rate limit check
    async fn check_rate_limit(&self, key: &str, max_requests: u32, window_secs: u64) -> Result<bool>;
}
```

---

## 9. Service Layer Design

### 9.1 FileService

```rust
pub struct FileService<UB, BR, CS>
where
    UB: UserBucketStore,
    BR: CrossBucketReader,
    CS: CoordinationStore,
{
    user_buckets: Arc<UB>,
    cross_bucket: Arc<BR>,
    coordination: Arc<CS>,
    blob_store: Arc<dyn BlobStore>,
}
```

**Methods:**
- `upload_file(user_id, name, parent_id, content, mime_type) -> Result<File>`
- `get_file(user_id, file_id) -> Result<File>`
- `list_files(user_id, folder_id) -> Result<Vec<File>>`
- `rename_file(user_id, file_id, new_name) -> Result<File>`
- `move_file(user_id, file_id, target_folder_id) -> Result<File>`
- `delete_file(user_id, file_id) -> Result<()>`
- `restore_file(user_id, file_id) -> Result<File>`

### 9.2 FolderService

**Methods:**
- `create_folder(user_id, name, parent_id) -> Result<Folder>`
- `get_folder(user_id, folder_id) -> Result<Folder>`
- `list_folders(user_id, parent_id) -> Result<Vec<Folder>>`
- `rename_folder(user_id, folder_id, new_name) -> Result<Folder>`
- `move_folder(user_id, folder_id, target_parent_id) -> Result<Folder>`
- `delete_folder(user_id, folder_id) -> Result<()>`
- `restore_folder(user_id, folder_id) -> Result<Folder>`

### 9.3 ShareService

**Methods:**
- `create_user_share(owner_id, resource_id, resource_type, recipient_id, permissions) -> Result<Share>`
- `create_public_share(owner_id, resource_id, resource_type, permissions, options) -> Result<Share>`
- `revoke_share(owner_id, share_id) -> Result<()>`
- `list_outbound_shares(owner_id) -> Result<Vec<Share>>`
- `list_received_shares(recipient_id) -> Result<Vec<ReceivedShare>>`
- `accept_share(recipient_id, share_id) -> Result<()>`
- `hide_received_share(recipient_id, share_id) -> Result<()>`

### 9.4 FavouriteService

**Methods:**
- `star_resource(user_id, resource_id, resource_type) -> Result<FavouriteEntry>`
- `star_shared_resource(user_id, share_id) -> Result<FavouriteEntry>`
- `unstar_resource(user_id, favourite_id) -> Result<()>`
- `list_favourites(user_id) -> Result<Vec<FavouriteEntry>>`
- `is_favourited(user_id, resource_id) -> Result<bool>`

---

## 10. Contract Test Matrix

### 10.1 User Bucket Isolation Tests

| Test ID | Description | Expected Result |
|---------|-------------|-----------------|
| UB-01 | Create file writes to correct user bucket | Object exists only in owner's bucket |
| UB-02 | Recipient share reference in recipient bucket | ReceivedShareReference in recipient's bucket |
| UB-03 | Favourites in user's bucket only | FavouritesIndex in user's bucket, not owner's |
| UB-04 | No central metadata repository required | Test passes without PostgreSQL |

### 10.2 File Lifecycle Tests

| Test ID | Description | Expected Result |
|---------|-------------|-----------------|
| FL-01 | Upload creates all required documents | FileDocument, VersionDocument, FolderChildrenIndex |
| FL-02 | File identity stable across operations | ID unchanged after rename/move |
| FL-03 | Delete creates tombstone | TombstoneDocument exists, FileDocument marked deleted |
| FL-04 | Restore from tombstone works | File restored with original metadata |
| FL-05 | Version history preserved | All versions accessible after multiple updates |

### 10.3 Sharing Tests

| Test ID | Description | Expected Result |
|---------|-------------|-----------------|
| SH-01 | Create share writes outbound doc | OutboundShareDocument in owner's bucket |
| SH-02 | Recipient receives reference | ReceivedShareReference in recipient's bucket |
| SH-03 | Shared-with-me index updated | Recipient's index contains share entry |
| SH-04 | Revoke share removes visibility | Recipient can no longer access (eventually) |
| SH-05 | Revoke does not delete resource | Owner's file still exists |

### 10.4 Favourites Tests

| Test ID | Description | Expected Result |
|---------|-------------|-----------------|
| FV-01 | Star owned file updates user favourites | FavouritesIndex updated, owner file unchanged |
| FV-02 | Star shared file updates recipient favourites only | Recipient's index updated, owner's file unchanged |
| FV-03 | Unstar removes from favourites | Entry removed from user's index |
| FV-04 | Favourites survive user bucket restore | Favourites present after restore |

### 10.5 Restore Independence Tests

| Test ID | Description | Expected Result |
|---------|-------------|-----------------|
| RI-01 | Export user bucket produces complete state | All documents exported |
| RI-02 | Restore from bucket without central DB | User state fully restored |
| RI-03 | Shared-with-me restored from received shares | Recipient shares present after restore |
| RI-04 | Favourites restored from indexes | Favourites present after restore |

### 10.6 Portable Locator Tests

| Test ID | Description | Expected Result |
|---------|-------------|-----------------|
| PL-01 | Locator serializes correctly | Valid JSON, all fields present |
| PL-02 | Locator deserializes correctly | All fields parsed correctly |
| PL-03 | Locator survives bucket relocation | Endpoint ref can be remapped |
| PL-04 | Cross-bucket read via locator works | Resource fetched from owner bucket |

### 10.7 No-Scan Hot Path Tests

| Test ID | Description | Expected Result |
|---------|-------------|-----------------|
| NS-01 | Folder listing uses index | FolderChildrenIndex read, not bucket scan |
| NS-02 | Favourites listing uses index | FavouritesIndex read, not scan |
| NS-03 | Shared-with-me uses index | SharedWithMeIndex read, not scan |
| NS-04 | User roots uses index | UserRootsIndex read, not scan |

### 10.8 Redis Optionality Tests

| Test ID | Description | Expected Result |
|---------|-------------|-----------------|
| RO-01 | Core flows work without Redis | File CRUD, sharing work |
| RO-02 | Distributed coordination requires Redis | Leases, fanout require Redis |
| RO-03 | Redis loss does not destroy truth | Durable state remains in buckets |

---

## 11. Implementation Phases

### Phase 1: Foundation
1. Define PortableStorageLocator schema
2. Create UserBucketStore trait and S3 implementation
3. Create CrossBucketReader trait
4. Update schemas for new architecture

### Phase 2: Core Services
1. Implement FileService with per-user buckets
2. Implement FolderService
3. Implement ShareService with dual-sided writes
4. Write contract tests for file/folder lifecycle

### Phase 3: Sharing and Favourites
1. Implement ReceivedShareReference handling
2. Implement SharedWithMeIndex
3. Implement FavouriteService
4. Write contract tests for sharing and favourites

### Phase 4: Indexes and Performance
1. Implement FolderChildrenIndex
2. Implement index rebuild capability
3. Implement no-scan listing methods
4. Write performance contract tests

### Phase 5: Restore and Portability
1. Implement bucket export
2. Implement bucket restore
3. Implement portable locator resolution
4. Write restore independence tests

---

## 12. Migration Notes

This is a **clean-slate design**. No migration from PostgreSQL is required or provided.

Existing installations must:
1. Export data from old system
2. Import into new per-user bucket structure
3. Rebuild all indexes

---

## Appendix A: JSON Schema Examples

### Example: FileDocument
```json
{
  "schema_version": 1,
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "parent_id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
  "name": "document.pdf",
  "path": "/Documents/document.pdf",
  "owner_id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
  "current_version_id": "6ba7b811-9dad-11d1-80b4-00c04fd430c8",
  "version_number": 1,
  "size": 1048576,
  "mime_type": "application/pdf",
  "content_ref": "sha256:a3f5c8d9e2b1...",
  "checksum": "a3f5c8d9e2b1...",
  "created_at": "2026-03-27T12:00:00Z",
  "updated_at": "2026-03-27T12:00:00Z",
  "version": 1,
  "deleted": false
}
```

### Example: PortableStorageLocator
```json
{
  "locator_version": 1,
  "storage_provider_kind": "s3",
  "endpoint_ref": "primary",
  "bucket": "rustshare-user-6ba7b810-9dad-11d1-80b4-00c04fd430c8",
  "key": "owned/files/550e8400-e29b-41d4-a716-446655440000.json",
  "resource_type": "file",
  "resource_id": "550e8400-e29b-41d4-a716-446655440000",
  "version_id": null,
  "content_hash": "sha256:a3f5c8d9e2b1..."
}
```

---

## Appendix B: Glossary

- **USD**: User Storage Domain - the isolated storage bucket for a user
- **PSL**: Portable Storage Locator - a cross-reference that survives relocation
- **Canonical state**: The source of truth stored in owner's bucket
- **Derived state**: Rebuildable indexes/projections
- **Ephemeral state**: Runtime-only state that doesn't need persistence
- **Owner**: The user who owns a resource
- **Recipient**: The user who receives access via share
