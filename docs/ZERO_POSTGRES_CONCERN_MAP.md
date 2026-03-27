# Zero-Postgres Concern Classification Map

This document provides exact mapping of every concern in RustShare to its target storage location.

---

## Concern: User Accounts

### Current State
- **Storage:** PostgreSQL `users` table
- **Pattern:** CRUD with SQL queries
- **Queries:** Find by email, find by username, find by ID, list all

### Target State
**Classification:** RustFS Canonical

**Schema:**
```rust
// UserDocument (new - to add to metadata_v2/schemas.rs)
pub struct UserDocument {
    pub schema_version: u32,
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub password_hash: String,
    pub is_admin: bool,
    pub is_disabled: bool,
    pub disabled_at: Option<DateTime<Utc>>,
    pub disabled_reason: Option<String>,
    pub storage_quota_bytes: i64,
    pub theme: String,  // "light", "dark", "system"
    pub email_verified_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: u64,
}
```

**Storage Key:** `users/{namespace}/{user_id}.json`

**Indexes (RustFS Derived):**
- `indexes/users/by-email/{email_hash} → user_id`
- `indexes/users/by-username/{username_hash} → user_id`
- `indexes/users/all → Vec<user_id>`

**Operations:**
- Create: PUT `users/{id}.json`
- Read: GET `users/{id}.json`
- Update: PUT with If-Match
- Delete: PUT tombstone or hard DELETE
- Find by email: Lookup index, then GET
- Find by username: Lookup index, then GET

---

## Concern: User Sessions

### Current State
- **Storage:** PostgreSQL `user_sessions` table
- **Pattern:** Create on login, lookup by token hash, delete on logout

### Target State
**Classification:** Ephemeral (Redis or Memory)

**Rationale:** Sessions are inherently transient. If Redis is lost, users simply need to log in again.

**Schema (Redis Hash):**
```
Key: session:{token_hash}
Fields:
  - user_id: Uuid
  - created_at: timestamp
  - expires_at: timestamp
  - user_agent: String
  - ip_address: String
TTL: session expiry (e.g., 24 hours)
```

**Schema (Memory - Standalone):**
```rust
struct SessionEntry {
    user_id: Uuid,
    token_hash: String,
    created_at: Instant,
    expires_at: Instant,
    user_agent: Option<String>,
    ip_address: Option<String>,
}
// Stored in: HashMap<String, SessionEntry>
```

**Operations:**
- Create: HSET session:{hash} with EXPIRE
- Read: HGETALL session:{hash}
- Delete: DEL session:{hash}
- List by user: Scan session:*, filter by user_id (or separate index)

**Revocation Cache:**
```
Key: revoked:{token_signature}
Value: "1"
TTL: remaining JWT lifetime
```

---

## Concern: Folders

### Current State
- **Storage:** PostgreSQL `folders` table
- **Pattern:** Hierarchical queries, parent lookups

### Target State
**Classification:** RustFS Canonical (ALREADY IMPLEMENTED in metadata_v2)

**Schema:** `FolderDocument` (exists in metadata_v2/schemas.rs)

**Storage Key:** `folders/{namespace}/{folder_id}.json`

**Indexes (RustFS Derived):**
- `FolderChildrenIndex` (exists)
- `UserRootsIndex` (exists)

---

## Concern: Files

### Current State
- **Storage:** PostgreSQL `files` table

### Target State
**Classification:** RustFS Canonical (ALREADY IMPLEMENTED in metadata_v2)

**Schema:** `FileDocument` (exists in metadata_v2/schemas.rs)

**Storage Key:** `files/{namespace}/{file_id}.json`

---

## Concern: File Versions

### Current State
- **Storage:** PostgreSQL `file_versions` table

### Target State
**Classification:** RustFS Canonical (ALREADY IMPLEMENTED in metadata_v2)

**Schema:** `FileVersionDocument` (exists in metadata_v2/schemas.rs)

**Storage Key:** `file-versions/{namespace}/{version_id}.json`

---

## Concern: Shares

### Current State
- **Storage:** PostgreSQL `shares` table

### Target State
**Classification:** RustFS Canonical (ALREADY IMPLEMENTED in metadata_v2)

**Schema:** `ShareDocument` (exists in metadata_v2/schemas.rs)

**Storage Key:** `shares/{namespace}/{share_id}.json`

**Indexes (RustFS Derived):**
- `indexes/shares/by-token/{token_hash} → share_id`
- `indexes/shares/by-resource/{resource_type}/{resource_id} → Vec<share_id>`
- `indexes/shares/by-creator/{user_id} → Vec<share_id>`

---

## Concern: Notifications

### Current State
- **Storage:** PostgreSQL `notifications` table
- **Pattern:** Per-user list, mark read, delete

### Target State
**Classification:** RustFS Derived Projection

**Rationale:** Notifications are derived from events. The event log is canonical.

**Schema:**
```rust
// NotificationDocument
pub struct NotificationDocument {
    pub schema_version: u32,
    pub id: Uuid,
    pub user_id: Uuid,           // Recipient
    pub event_id: Uuid,          // Source event
    pub resource_type: String,   // "file", "folder", "share"
    pub resource_id: Uuid,
    pub notification_type: NotificationType,
    pub title: String,
    pub message: String,
    pub read: bool,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

// UserNotificationIndex (rebuildable)
pub struct UserNotificationIndex {
    pub schema_version: u32,
    pub user_id: Uuid,
    pub version: u64,
    pub updated_at: DateTime<Utc>,
    pub notifications: Vec<NotificationRef>,
    pub unread_count: u32,
}

pub struct NotificationRef {
    pub notification_id: Uuid,
    pub notification_type: NotificationType,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub read: bool,
    pub created_at: DateTime<Utc>,
}

pub enum NotificationType {
    FileShared,
    FolderShared,
    FileModified,
    AccessRequested,
    // etc.
}
```

**Storage Keys:**
- `notifications/{namespace}/{user_id}/{notification_id}.json`
- `indexes/notifications/by-user/{user_id}.json`

**Projection Logic:**
- Event appended → NotificationProjector creates notification
- User marks read → Update notification doc, update index
- User deletes → Delete notification doc, update index

---

## Concern: Replication Jobs

### Current State
- **Storage:** PostgreSQL `replication_jobs`, `replication_attempts`
- **Pattern:** Job queue with leasing, retry tracking

### Target State
**Classification:** RustFS Canonical (job doc) + Ephemeral Coordination (leases)

**Schema:**
```rust
// JobDocument
pub struct JobDocument {
    pub schema_version: u32,
    pub id: Uuid,
    pub job_type: JobType,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub status: JobStatus,
    pub priority: i32,
    pub payload: serde_json::Value,  // Job-specific data
    pub result: Option<JobResult>,
    pub retry_count: u32,
    pub max_retries: u32,
    pub created_at: DateTime<Utc>,
    pub scheduled_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub worker_id: Option<String>,
    pub version: u64,
}

pub enum JobType {
    ReplicateToTarget { target_id: Uuid },
    GenerateThumbnail,
    VirusScan,
}

pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

// JobQueueIndex (rebuildable)
pub struct JobQueueIndex {
    pub namespace: String,
    pub version: u64,
    pub updated_at: DateTime<Utc>,
    pub pending: Vec<JobRef>,
    pub running: Vec<JobRef>,
    pub completed_recent: Vec<JobRef>, // Last 100
}
```

**Storage Keys:**
- `jobs/{namespace}/{job_id}.json`
- `indexes/jobs/queue/{status}.json`

**Lease (Ephemeral):**
```
Redis Key: job:lease:{job_id}
Value: {worker_id}:{timestamp}
TTL: lease_duration (e.g., 5 minutes)
```

**Coordination Flow:**
1. Worker queries JobQueueIndex for Pending jobs
2. Worker attempts to acquire lease: SET job:lease:{id} {worker_id} NX EX 300
3. If success, update JobDocument status to Running, write to RustFS
4. Worker extends lease periodically during execution
5. On complete, update JobDocument status, release lease

---

## Concern: Replication Targets

### Current State
- **Storage:** PostgreSQL `replication_targets` table

### Target State
**Classification:** RustFS Canonical

**Schema:**
```rust
pub struct ReplicationTargetDocument {
    pub schema_version: u32,
    pub id: Uuid,
    pub name: String,
    pub target_type: TargetType,  // S3, S3Compatible, etc.
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub access_key_id: String,      // Encrypted
    pub secret_access_key: String,  // Encrypted
    pub path_prefix: String,
    pub enabled: bool,
    pub priority: i32,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: u64,
}
```

**Storage Key:** `replication-targets/{namespace}/{target_id}.json`

---

## Concern: OIDC Login State

### Current State
- **Storage:** PostgreSQL `oidc_login_states` table
- **Pattern:** Create at login start, validate at callback, delete

### Target State
**Classification:** Ephemeral (Redis or Memory)

**Rationale:** OIDC state is transient - only lives for the duration of the login flow (minutes).

**Schema (Redis):**
```
Key: oidc:state:{state_token}
Value: {csrf_token}:{nonce}:{redirect_uri}:{created_at}
TTL: 10 minutes
```

---

## Concern: Device Pairing

### Current State
- **Storage:** PostgreSQL `device_pair_requests`

### Target State
**Classification:** Ephemeral (Redis or Memory)

**Schema (Redis):**
```
Key: device:pair:{pairing_code}
Hash:
  - request_id: Uuid
  - requesting_device_id: String
  - status: pending|approved|rejected
  - user_id: Uuid (set on approval)
  - created_at: timestamp
TTL: 10 minutes
```

---

## Concern: Device Tokens

### Current State
- **Storage:** PostgreSQL `device_tokens` table
- **Pattern:** Long-lived tokens for mobile apps

### Target State
**Classification:** RustFS Canonical

**Schema:**
```rust
pub struct DeviceTokenDocument {
    pub schema_version: u32,
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub device_name: String,
    pub device_type: String,  // "ios", "android", "desktop"
    pub last_used_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}
```

**Storage Key:** `device-tokens/{namespace}/{token_hash}.json`

---

## Concern: Security Events / Audit Log

### Current State
- **Storage:** PostgreSQL `user_security_events`, `admin_actions`, `share_access_log`

### Target State
**Classification:** RustFS Derived (Event Log)

**Rationale:** All these are audit trails derived from events.

**Schema:**
```rust
// Uses existing EventDocument with specific event types:
// - SecurityEvent { event_type: "password_login", "logout", "password_change" }
// - AdminAction { action: "user_created", "config_changed" }
// - ShareAccess { share_id, action: "view", "download" }
```

**Storage:** Event log is append-only in RustFS.

**Query Pattern:** Scan events by type and time range.

---

## Concern: System Configuration

### Current State
- **Storage:** PostgreSQL `oidc_config`, `smtp_config`, `webhook_configs`

### Target State
**Classification:** RustFS Canonical

**Schema:**
```rust
pub struct SystemConfigDocument {
    pub schema_version: u32,
    pub config_type: ConfigType,
    pub config: serde_json::Value,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
}

pub enum ConfigType {
    Oidc,
    Smtp,
    Webhooks,
    Server,
}
```

**Storage Key:** `config/{namespace}/{config_type}.json`

---

## Concern: User Groups

### Current State
- **Storage:** PostgreSQL `user_groups`, `user_group_members`

### Target State
**Classification:** RustFS Canonical

**Schema:**
```rust
pub struct UserGroupDocument {
    pub schema_version: u32,
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub member_ids: Vec<Uuid>,
    pub version: u64,
}
```

**Storage Key:** `user-groups/{namespace}/{group_id}.json`

**Index:**
- `indexes/user-groups/by-name/{name_hash} → group_id`

---

## Concern: Thumbnails

### Current State
- **Storage:** PostgreSQL `file_thumbnails` table

### Target State
**Classification:** RustFS Derived

**Schema:**
```rust
pub struct ThumbnailMetadataDocument {
    pub schema_version: u32,
    pub file_id: Uuid,
    pub thumbnail_key: String,  // Blob store key
    pub width: u32,
    pub height: u32,
    pub format: String,  // "webp", "jpeg"
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
}
```

**Storage Key:** `thumbnails/{namespace}/{file_id}.json`

**Blob Storage:** Thumbnail data stored in RustFS blob store.

---

## Summary Table

| Concern | Canonical (RustFS) | Derived (RustFS) | Ephemeral (Redis) | Ephemeral (Memory) |
|---------|-------------------|------------------|-------------------|-------------------|
| Users | ✅ | - | - | - |
| Sessions | - | - | ✅ | ✅ (standalone) |
| Folders | ✅ | - | - | - |
| Files | ✅ | - | - | - |
| File Versions | ✅ | - | - | - |
| Shares | ✅ | - | - | - |
| Notifications | - | ✅ | - | - |
| Replication Jobs | ✅ | ✅ | ✅ | ✅ (standalone) |
| Replication Targets | ✅ | - | - | - |
| OIDC State | - | - | ✅ | ✅ (standalone) |
| Device Pairing | - | - | ✅ | ✅ (standalone) |
| Device Tokens | ✅ | - | - | - |
| Security Events | ✅ | - | - | - |
| System Config | ✅ | - | - | - |
| User Groups | ✅ | - | - | - |
| Thumbnails | - | ✅ | - | - |

---

## File Locations Summary

### New Schemas (add to metadata_v2/schemas.rs)
- UserDocument
- NotificationDocument
- UserNotificationIndex
- JobDocument
- JobQueueIndex
- ReplicationTargetDocument
- DeviceTokenDocument
- UserGroupDocument
- SystemConfigDocument
- ThumbnailMetadataDocument

### New Repository Traits (new modules)
- UserRepository
- NotificationRepository
- JobRepository
- DeviceTokenRepository
- UserGroupRepository
- ConfigRepository

### New Coordination (new coordination module)
- CoordinationStore trait
- InMemoryCoordinationStore
- RedisCoordinationStore

### New Session (new session module)
- SessionManager trait
- StatelessSessionManager
