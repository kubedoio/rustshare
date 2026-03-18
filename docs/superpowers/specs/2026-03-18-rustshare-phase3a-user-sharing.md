# RustShare Phase 3A: User-to-User File Sharing

**Date:** 2026-03-18
**Status:** Spec Review Complete - Ready for Implementation
**Dependencies:** Phase 1 (Foundation), Phase 3B (Public Share Links)

---

## 1. Overview

### 1.1 Purpose

Enable authenticated RustShare users to share files and folders directly with other registered users via email-based identification. Users can grant three permission levels (View, Edit, Admin) and manage recipients. The system supports both real-time WebSocket notifications and persistent in-app notifications.

### 1.2 Goals

- **User-to-user sharing**: Registered users can share files/folders with other registered users
- **Email-based identification**: Share using recipient's email address (no username lookup)
- **Three permission levels**: View (read-only), Edit (upload versions), Admin (manage recipients)
- **Folder sharing**: Share entire folders with inherited permissions for all contents
- **Unified share model**: Extend existing shares table to handle both public and user shares
- **Real-time notifications**: WebSocket events for immediate updates
- **Persistent notifications**: In-app notification system for offline/missed events
- **Recipient management**: Admin users can add/remove recipients and change permissions

### 1.3 Non-Goals (Deferred)

- User invitations (sharing with non-registered emails)
- External sharing (sharing outside organization)
- Group/team-based sharing
- Share link generation for user shares (only direct user-to-user)
- Expiration dates for user shares
- Access analytics (view counts, last accessed)

---

## 2. Architecture

### 2.1 Approach

**Unified Share Model with Permission Layers**: Extend the existing `shares` table to handle both public shares (anonymous access) and user shares (authenticated user-to-user) in a single unified model. Add `recipient_user_id` field (null for public shares, UserId for user shares) and expand permission system to three levels.

**Key principles:**
- Single source of truth for all share types
- Folder permissions inherit to all contents recursively
- Permission resolution walks folder tree (direct → inherited)
- Persistent notifications as source of truth, WebSocket for real-time delivery

### 2.2 Data Model

#### Extended `shares` table

```sql
CREATE TABLE shares (
  id UUID PRIMARY KEY,
  file_id UUID REFERENCES files(id),           -- MODIFIED: now nullable for folder shares
  folder_id UUID REFERENCES folders(id),       -- NEW: for folder shares
  share_token VARCHAR(255),                    -- MODIFIED: nullable for user shares
  permissions VARCHAR(50) NOT NULL,            -- Extended: View/Edit/Admin
  password_hash VARCHAR(255),                  -- For public shares only
  expires_at TIMESTAMP,                        -- For public shares only
  access_count INTEGER DEFAULT 0,              -- For public shares only
  recipient_user_id UUID REFERENCES users(id), -- NEW: for user shares (null = public)
  created_by UUID NOT NULL REFERENCES users(id),
  created_at TIMESTAMP NOT NULL,
  revoked_at TIMESTAMP,
  CONSTRAINT check_share_target CHECK (
    (file_id IS NOT NULL AND folder_id IS NULL) OR
    (file_id IS NULL AND folder_id IS NOT NULL)
  ),
  CONSTRAINT check_share_token_for_public CHECK (
    (recipient_user_id IS NULL AND share_token IS NOT NULL) OR
    (recipient_user_id IS NOT NULL)
  )
);

CREATE INDEX idx_shares_recipient ON shares(recipient_user_id, revoked_at);
CREATE INDEX idx_shares_file ON shares(file_id, revoked_at);
CREATE INDEX idx_shares_folder ON shares(folder_id, revoked_at);
CREATE UNIQUE INDEX idx_shares_token_unique ON shares(share_token) WHERE share_token IS NOT NULL;
```

**Share types:**
- **Public share**: `recipient_user_id = NULL`, `share_token` required (unique for public shares only)
- **User share**: `recipient_user_id = UserId`, `share_token = NULL` (not needed for authenticated shares)

**Data model: One-to-One mapping**
- Each share record represents access for ONE user to ONE resource (file or folder)
- Sharing with multiple users creates multiple share records
- Example: Alice shares `file.pdf` with Bob and Carol → 2 DB rows with same `file_id`, different `recipient_user_id`

**Permission levels (extended enum):**
```rust
pub enum SharePermissions {
    View,   // Read-only: download files, view folder contents
    Edit,   // View + upload new versions, create files/folders
    Admin,  // Edit + manage recipients (add/remove, change permissions)
}

impl SharePermissions {
    /// Returns numeric level for comparison (View=1, Edit=2, Admin=3)
    pub fn level(&self) -> u8 {
        match self {
            Self::View => 1,
            Self::Edit => 2,
            Self::Admin => 3,
        }
    }

    /// Returns the highest permission from a list
    pub fn max(permissions: &[SharePermissions]) -> SharePermissions {
        permissions
            .iter()
            .max_by_key(|p| p.level())
            .cloned()
            .unwrap_or(SharePermissions::View)
    }
}

impl PartialOrd for SharePermissions {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.level().partial_cmp(&other.level())
    }
}
```

**Permission hierarchy:** `View < Edit < Admin`

**Migration from Phase 3B permissions:**
- Existing `Read` → `View` (database migration will update enum values)
- Existing `ReadWrite` → `Edit` (renamed for clarity and consistency)
- New `Admin` added for recipient management

#### New `notifications` table

```sql
CREATE TABLE notifications (
  id UUID PRIMARY KEY,
  user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  notification_type VARCHAR(50) NOT NULL,
  title VARCHAR(255) NOT NULL,
  message TEXT NOT NULL,
  resource_id UUID NOT NULL,                   -- File/Folder/Share ID (no FK constraint - polymorphic)
  resource_type VARCHAR(50) NOT NULL,          -- "file", "folder", "share"
  action_url VARCHAR(500),                     -- Optional: deep link to resource
  read BOOLEAN DEFAULT FALSE,
  created_at TIMESTAMP NOT NULL,
  INDEX idx_user_unread (user_id, read, created_at),
  INDEX idx_resource (resource_id, resource_type)
);
```

**Note on resource_id:** This field is polymorphic (can reference files, folders, or shares) so it does NOT have a foreign key constraint. When the referenced resource is deleted, the notification remains but the action_url may become invalid (handled gracefully in frontend).

### 2.3 Domain Models

**Extend `Share` model** (`backend/crates/core/src/domain/share.rs`):

**BREAKING CHANGE:** This modifies the existing Share struct from Phase 3B. Migration required.

```rust
pub struct Share {
    pub id: ShareId,
    pub file_id: Option<FileId>,           // MODIFIED: was FileId (non-optional), now Option<FileId>
    pub folder_id: Option<FolderId>,       // NEW
    pub share_token: Option<String>,       // MODIFIED: was String, now Option<String>
    pub permissions: SharePermissions,     // MODIFIED: enum variants changed (Read→View, ReadWrite→Edit, +Admin)
    pub password_hash: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub access_count: i32,
    pub recipient_user_id: Option<UserId>, // NEW
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl Share {
    pub fn is_public_share(&self) -> bool {
        self.recipient_user_id.is_none()
    }

    pub fn is_user_share(&self) -> bool {
        self.recipient_user_id.is_some()
    }

    pub fn is_folder_share(&self) -> bool {
        self.folder_id.is_some()
    }

    pub fn is_file_share(&self) -> bool {
        self.file_id.is_some()
    }

    /// Get the resource being shared (file or folder ID)
    ///
    /// # Safety
    /// This uses expect() which panics if both file_id and folder_id are None.
    /// The database CHECK constraint guarantees this never happens, but callers
    /// in test code should ensure one is set.
    pub fn resource_id(&self) -> Uuid {
        self.file_id.or(self.folder_id).expect("Share must have file_id or folder_id")
    }
}
```

**New `Notification` model** (`backend/crates/core/src/domain/notification.rs`):
```rust
pub struct Notification {
    pub id: Uuid,
    pub user_id: UserId,
    pub notification_type: NotificationType,
    pub title: String,
    pub message: String,
    pub resource_id: Uuid,
    pub resource_type: ResourceType,
    pub action_url: Option<String>,
    pub read: bool,
    pub created_at: DateTime<Utc>,
}

pub enum NotificationType {
    ShareReceived,
    PermissionChanged,
    ShareRevoked,
}

pub enum ResourceType {
    File,
    Folder,
    Share,
}
```

**New `ShareRecipient` DTO** (`backend/crates/core/src/domain/share.rs`):
```rust
/// Represents a recipient of a share (for API responses)
pub struct ShareRecipient {
    pub share_id: ShareId,
    pub user_id: UserId,
    pub email: String,
    pub permission: SharePermissions,
    pub added_at: DateTime<Utc>,
    pub added_by: UserId,
}
```

### 2.4 Service Layer

#### New `UserShareService` (`backend/crates/core/src/services/user_share_service.rs`)

**Responsibilities:**
- Create user shares (file and folder)
- List shares received by user
- List recipients of a share
- Update recipient permissions (Admin only)
- Remove recipients (Admin only)
- Validate permissions (owner/admin checks)

**Key methods:**
```rust
impl UserShareService {
    async fn create_file_share(
        &self,
        file_id: FileId,
        recipient_email: &str,
        permission: SharePermissions,
        created_by: UserId,
    ) -> Result<Share, ShareError>;

    async fn create_folder_share(
        &self,
        folder_id: FolderId,
        recipient_email: &str,
        permission: SharePermissions,
        created_by: UserId,
    ) -> Result<Share, ShareError>;

    async fn list_received_shares(
        &self,
        user_id: UserId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Share>, ShareError>;

    async fn list_recipients(
        &self,
        share_id: ShareId,
        requesting_user: UserId,
    ) -> Result<Vec<ShareRecipient>, ShareError>;

    async fn update_recipient_permission(
        &self,
        share_id: ShareId,
        recipient_user_id: UserId,
        new_permission: SharePermissions,
        requesting_user: UserId,
    ) -> Result<Share, ShareError>;

    async fn remove_recipient(
        &self,
        share_id: ShareId,
        recipient_user_id: UserId,
        requesting_user: UserId,
    ) -> Result<(), ShareError>;
}
```

#### New `PermissionResolver` (`backend/crates/core/src/services/permission_resolver.rs`)

**Responsibilities:**
- Resolve effective permissions for user on resource
- Check ownership first (owners have implicit Admin permission)
- Handle direct shares (file/folder)
- Handle inherited permissions (folder ancestry)
- Apply permission hierarchy (take highest)
- Cache results per request

**Owner Permission Handling:**
- File/folder owners have implicit Admin permission (not stored in database)
- Check ownership BEFORE checking shares
- No share record needed for owner access

**Algorithm:**
```rust
impl PermissionResolver {
    /// Resolve permission for user on a resource
    /// Returns: Some(permission) if user has access, None if no access
    async fn resolve_permission(
        &self,
        user_id: UserId,
        resource: Resource, // File or Folder
    ) -> Result<Option<SharePermissions>, RepositoryError> {
        // 0. Check ownership first (implicit Admin permission)
        if self.is_owner(user_id, resource).await? {
            return Ok(Some(SharePermissions::Admin));
        }

        // 1. Check direct share on resource
        if let Some(perm) = self.check_direct_share(user_id, resource).await? {
            return Ok(Some(perm));
        }

        // 2. Walk up folder ancestry for inherited permissions
        let mut current_folder = self.get_parent_folder(resource).await?;
        let mut max_depth = 50; // Safety guard (reduced from 100 to realistic depth)

        while let Some(folder) = current_folder {
            if max_depth == 0 {
                return Err(RepositoryError::MaxDepthExceeded);
            }

            if let Some(perm) = self.check_folder_share(user_id, folder.id).await? {
                return Ok(Some(perm));
            }

            current_folder = self.get_parent_folder(Resource::Folder(folder.id)).await?;
            max_depth -= 1;
        }

        // 3. No permission found
        Ok(None)
    }

    /// Check if user owns the resource
    async fn is_owner(&self, user_id: UserId, resource: Resource) -> Result<bool, RepositoryError> {
        match resource {
            Resource::File(file_id) => {
                let file = self.file_repo.get_by_id(file_id).await?;
                Ok(file.owner_id == user_id)
            }
            Resource::Folder(folder_id) => {
                let folder = self.folder_repo.get_by_id(folder_id).await?;
                Ok(folder.owner_id == user_id)
            }
        }
    }

    /// Check direct share on resource (non-inherited)
    async fn check_direct_share(
        &self,
        user_id: UserId,
        resource: Resource,
    ) -> Result<Option<SharePermissions>, RepositoryError> {
        let share = match resource {
            Resource::File(file_id) => {
                self.share_repo.find_user_share(file_id, None, user_id).await?
            }
            Resource::Folder(folder_id) => {
                self.share_repo.find_user_share(None, folder_id, user_id).await?
            }
        };

        Ok(share.filter(|s| s.revoked_at.is_none()).map(|s| s.permissions))
    }

    /// Check folder share (for inheritance)
    async fn check_folder_share(
        &self,
        user_id: UserId,
        folder_id: FolderId,
    ) -> Result<Option<SharePermissions>, RepositoryError> {
        let share = self.share_repo.find_user_share(None, Some(folder_id), user_id).await?;
        Ok(share.filter(|s| s.revoked_at.is_none()).map(|s| s.permissions))
    }
}
```

**Per-request caching:**
- Cache key: `(user_id, resource_type, resource_id)`
- Cache storage: Thread-local HashMap or request-scoped context
- Cache invalidation: Cleared at end of request (no cross-request caching)
- Avoids repeated DB queries when checking permissions multiple times in same request

#### New `NotificationService` (`backend/crates/core/src/services/notification_service.rs`)

**Responsibilities:**
- Create notifications for users
- Mark notifications as read
- Get unread count
- List notifications with pagination
- Delete notifications

**Key methods:**
```rust
impl NotificationService {
    async fn create_notification(
        &self,
        user_id: UserId,
        notification_type: NotificationType,
        title: String,
        message: String,
        resource_id: Uuid,
        resource_type: ResourceType,
        action_url: Option<String>,
    ) -> Result<Notification, RepositoryError>;

    async fn mark_as_read(
        &self,
        notification_id: Uuid,
        user_id: UserId,
    ) -> Result<(), NotificationError>;

    async fn get_unread_count(
        &self,
        user_id: UserId,
    ) -> Result<i64, RepositoryError>;

    async fn list_notifications(
        &self,
        user_id: UserId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Notification>, RepositoryError>;

    async fn delete_notification(
        &self,
        notification_id: Uuid,
        user_id: UserId,
    ) -> Result<(), NotificationError>;
}
```

---

## 3. API Endpoints

### 3.1 User Share Endpoints

#### `POST /api/files/{file_id}/share`

**Description:** Share a file with one or more users.

**Request:**
```json
{
  "recipients": [
    { "email": "user@example.com", "permission": "View" },
    { "email": "admin@example.com", "permission": "Admin" }
  ]
}
```

**Response (201):**
```json
{
  "shares": [
    {
      "id": "uuid",
      "file_id": "uuid",
      "recipient_user_id": "uuid",
      "recipient_email": "user@example.com",
      "permissions": "View",
      "created_at": "2026-03-18T10:00:00Z"
    }
  ]
}
```

**Errors:**
- `404` - File not found
- `403` - User does not own file
- `404` - Recipient user not found (for specific email)
- `400` - Invalid permission level

#### `POST /api/folders/{folder_id}/share`

**Description:** Share a folder (and all contents) with one or more users.

**Request/Response:** Same as file share, with `folder_id` instead of `file_id`.

#### `GET /api/shares/received`

**Description:** List all shares received by the current user.

**Query params:**
- `limit` (default: 50, max: 100)
- `offset` (default: 0)
- `resource_type` (optional: "file" or "folder")

**Response (200):**
```json
{
  "shares": [
    {
      "id": "uuid",
      "file_id": "uuid",
      "file_name": "document.pdf",
      "permissions": "Edit",
      "created_by_email": "owner@example.com",
      "created_at": "2026-03-18T10:00:00Z"
    }
  ],
  "total": 42,
  "limit": 50,
  "offset": 0
}
```

#### `GET /api/shares/{share_id}/recipients`

**Description:** List all recipients of a share (Admin permission required).

**Response (200):**
```json
{
  "recipients": [
    {
      "user_id": "uuid",
      "email": "user@example.com",
      "permission": "View",
      "added_at": "2026-03-18T10:00:00Z",
      "added_by": "uuid"
    }
  ]
}
```

**Errors:**
- `404` - Share not found
- `403` - User does not have Admin permission on share

#### `PUT /api/shares/{share_id}/recipients`

**Description:** Update recipient permissions (Admin permission required).

**Request:**
```json
{
  "recipient_user_id": "uuid",
  "permission": "Edit"
}
```

**Response (200):**
```json
{
  "share": {
    "id": "uuid",
    "recipient_user_id": "uuid",
    "permissions": "Edit",
    "updated_at": "2026-03-18T11:00:00Z"
  }
}
```

**Errors:**
- `403` - User does not have Admin permission
- `404` - Share or recipient not found
- `400` - Cannot downgrade owner permission

#### `DELETE /api/shares/{share_id}/recipients/{user_id}`

**Description:** Remove a recipient from a share (Admin permission required).

**Response (204):** No content.

**Errors:**
- `403` - User does not have Admin permission
- `404` - Share or recipient not found
- `400` - Cannot remove owner from share

#### `DELETE /api/shares/{share_id}`

**Description:** Revoke an entire share (owner only). This soft-deletes the share by setting `revoked_at`.

**Response (204):** No content.

**Errors:**
- `403` - User is not the owner
- `404` - Share not found

**Note:** This is different from removing individual recipients. This endpoint revokes the entire share for ALL recipients.

### 3.2 Notification Endpoints

#### `GET /api/notifications`

**Description:** Get current user's notifications.

**Query params:**
- `limit` (default: 50, max: 100)
- `offset` (default: 0)
- `unread_only` (default: false)

**Response (200):**
```json
{
  "notifications": [
    {
      "id": "uuid",
      "notification_type": "share_received",
      "title": "New file shared with you",
      "message": "Alice shared 'Q4 Report.pdf' with you",
      "resource_id": "uuid",
      "resource_type": "file",
      "action_url": "/files/uuid",
      "read": false,
      "created_at": "2026-03-18T10:00:00Z"
    }
  ],
  "total": 15,
  "unread_count": 3
}
```

#### `PUT /api/notifications/{notification_id}/read`

**Description:** Mark a notification as read.

**Response (204):** No content.

**Errors:**
- `404` - Notification not found or does not belong to user

#### `DELETE /api/notifications/{notification_id}`

**Description:** Delete/dismiss a notification.

**Response (204):** No content.

**Errors:**
- `404` - Notification not found or does not belong to user

---

## 4. WebSocket Events

Extend the existing event system (`backend/crates/core/src/events/types.rs`) with new share-related events.

### 4.1 New Event Types

```rust
pub enum EventType {
    // ... existing events ...

    // User share events
    ShareReceivedByUser,      // User receives a new share
    SharePermissionChanged,   // User's permission on share changes
    ShareRevokedFromUser,     // User loses access to share
    NotificationCreated,      // New notification for user
}
```

### 4.2 Event Payloads

#### `ShareReceivedByUser`

```rust
pub struct ShareReceivedByUserPayload {
    pub share_id: ShareId,
    pub resource_id: Uuid,        // File or Folder ID
    pub resource_type: String,    // "file" or "folder"
    pub resource_name: String,
    pub permissions: SharePermissions,
    pub shared_by_email: String,
    pub shared_by_id: UserId,
    pub timestamp: DateTime<Utc>,
}
```

#### `SharePermissionChanged`

```rust
pub struct SharePermissionChangedPayload {
    pub share_id: ShareId,
    pub resource_id: Uuid,
    pub resource_type: String,
    pub old_permission: SharePermissions,
    pub new_permission: SharePermissions,
    pub changed_by_id: UserId,
    pub timestamp: DateTime<Utc>,
}
```

#### `ShareRevokedFromUser`

```rust
pub struct ShareRevokedFromUserPayload {
    pub share_id: ShareId,
    pub resource_id: Uuid,
    pub resource_type: String,
    pub resource_name: String,
    pub revoked_by_id: UserId,
    pub timestamp: DateTime<Utc>,
}
```

#### `NotificationCreated`

```rust
pub struct NotificationCreatedPayload {
    pub notification_id: Uuid,
    pub notification_type: String,
    pub title: String,
    pub message: String,
    pub resource_id: Uuid,
    pub action_url: Option<String>,
    pub timestamp: DateTime<Utc>,
}
```

### 4.3 Event Routing

**How events are routed to users:**

1. **Event creation**: When a share operation occurs (create/update/revoke), the service emits an event with recipient user_id
2. **WebSocket handler**: Looks up active WebSocket connections for the target user_id
3. **Delivery**: Sends event to ALL active connections for that user (multi-device support)
4. **Offline handling**: If no active connections, event is not sent (persistent notification in DB is source of truth)

**Implementation details:**
- WebSocket handler maintains in-memory map: `user_id → Set<connection_id>`
- When share event occurs, query: `connections = websocket_map.get(recipient_user_id)`
- Send event to each connection in the set
- No database queries for event routing (only in-memory lookup)

**User ID resolution:**
- For `ShareReceivedByUser`: recipient_user_id is in the share record (no lookup needed)
- For `SharePermissionChanged`: recipient_user_id is in the share record
- For `ShareRevokedFromUser`: recipient_user_id was in the share record before revocation (included in event payload)

**Frontend behavior:**
- Receives real-time event → shows toast notification → refetches affected data
- No client-side state mutation from events (always refetch from server for consistency)

---

## 5. Error Handling

### 5.1 New Error Types

Extend `ShareError` enum (`backend/crates/core/src/services/share_errors.rs`):

```rust
pub enum ShareError {
    // ... existing errors ...

    /// Recipient user not found by email
    #[error("User with email {0} not found")]
    RecipientNotFound(String),

    /// User does not have required permission level
    #[error("Requires {required} permission, user has {actual}")]
    InsufficientPermission {
        required: SharePermissions,
        actual: SharePermissions,
    },

    /// Cannot share with self
    #[error("Cannot share resource with yourself")]
    CannotShareWithSelf,

    /// Share already exists for this user
    #[error("Share already exists for user {0}")]
    ShareAlreadyExists(UserId),

    /// Cannot remove owner from share
    #[error("Cannot remove owner from share")]
    CannotRemoveOwner,
}
```

New `NotificationError` enum (`backend/crates/core/src/services/notification_errors.rs`):

```rust
pub enum NotificationError {
    #[error("Notification not found")]
    NotFound,

    #[error("Notification {0} does not belong to user {1}")]
    NotOwnedByUser(Uuid, UserId),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
}
```

### 5.2 Edge Cases

#### Cascading Permission Changes

- When folder share is revoked, all inherited permissions disappear immediately
- When folder permission is downgraded (Admin→Edit→View), child access updates
- WebSocket events sent to all affected users
- Frontend refetches permissions after receiving revocation event

#### Orphaned Shares

- **File/folder deleted**: Set `revoked_at` on all associated shares
- **Recipient deleted**: Shares remain in DB but filtered out in queries (`WHERE user_id IN (SELECT id FROM users)`)
- **Owner deleted**: Transfer shares to new owner OR revoke (depends on Phase 1 file ownership transfer logic)

#### Permission Conflicts

- Multiple shares on same resource (direct + inherited): Take highest permission
- Example: Direct "View" on file + inherited "Edit" from folder → User gets "Edit"
- Resolution order: Direct share > Nearest ancestor folder share

#### Circular Permission Checks

- Folder tree is acyclic (enforced by Phase 1 foreign key constraints)
- Permission resolution cannot loop infinitely
- Safety: Max depth of 50 levels (return error if exceeded - matches implementation)

#### Admin Permission Edge Cases

- Admin can add other Admins (delegation allowed)
- Admin cannot remove the owner (owner has implicit Admin)
- Admin cannot revoke entire share (only owner can via `DELETE /api/shares/{id}`)
- Owner always has Admin permission (implicit, not stored in DB)

#### Large Folder Shares

- Sharing folder with 10,000 files creates 1 share record (not 10,000)
- Permission checks happen on-demand (don't precompute for all children)
- Pagination required for "Shared with me" view (limit: 100 items per page)

#### Email Matching

- Case-insensitive lookup (normalize to lowercase before query)
- Trim whitespace before validation
- Reject if email not found in `users` table (no invitation system in Phase 3A)

---

## 6. Rate Limiting

Extend existing rate limiter from Phase 3B with new limits:

- **Share creation**: 20 share records per minute per user
  - Each API call to `POST /api/files/{id}/share` or `POST /api/folders/{id}/share` can create multiple share records (one per recipient)
  - Rate limit applies to the number of share records created, not API calls
  - Example: Sharing with 5 users consumes 5 from the limit

- **Notification creation**: 100 per minute per user (prevent abuse)
- **Recipient management**: 10 API calls per minute per user (add/remove/update recipients)
- **Notification fetches**: 100 API calls per minute per user

Use same Redis-backed rate limiter as Phase 3B.

---

## 7. Testing Strategy

### 7.1 Unit Tests

**Domain models:**
- `Share`: Test `is_public_share()`, `is_user_share()`, `is_folder_share()`, `is_file_share()`
- `Notification`: Test creation, validation, serialization
- `SharePermissions`: Test ordering (View < Edit < Admin), max() function

**Services:**
- `UserShareService`:
  - Create file/folder share with valid/invalid recipient
  - List received shares with pagination
  - Update recipient permission (Admin only)
  - Remove recipient (Admin only)
  - Permission validation (owner checks, admin checks)
- `PermissionResolver`:
  - Direct file share → returns permission
  - Direct folder share → returns permission
  - Inherited permission from parent folder → returns permission
  - Multiple permissions (direct + inherited) → returns highest
  - No permission → returns None
  - Max depth exceeded → returns error
- `NotificationService`:
  - Create notification with all fields
  - Mark as read (owner only)
  - Get unread count
  - List notifications with pagination
  - Delete notification (owner only)

### 7.2 Integration Tests

**API endpoints:**
- `POST /api/files/{id}/share`:
  - Share with valid user → 201, share created
  - Share with non-existent email → 404
  - Share file not owned → 403
  - Share with self → 400
  - Share with duplicate recipient → update existing share
- `POST /api/folders/{id}/share`: Same as above for folders
- `GET /api/shares/received`:
  - List shares for user → 200, paginated results
  - Empty list for user with no shares → 200, empty array
- `PUT /api/shares/{id}/recipients`:
  - Update as Admin → 200, permission updated
  - Update as non-Admin → 403
  - Update non-existent recipient → 404
- `DELETE /api/shares/{id}/recipients/{user_id}`:
  - Remove as Admin → 204
  - Remove as non-Admin → 403
  - Remove owner → 400
- `GET /api/notifications`: List notifications, test pagination, unread filter
- `PUT /api/notifications/{id}/read`: Mark as read → 204
- `DELETE /api/notifications/{id}`: Delete → 204

**Permission checks:**
- User can access file with direct share
- User can access file inside shared folder (inherited)
- User cannot access file without share
- User with View permission cannot upload
- User with Edit permission can upload
- User with Admin permission can add recipients

### 7.3 WebSocket Tests

- Connect two clients (User A, User B)
- A shares file with B → B receives `ShareReceivedByUser` + `NotificationCreated`
- A changes B's permission → B receives `SharePermissionChanged`
- A removes B from share → B receives `ShareRevokedFromUser`
- A shares folder → B receives events for folder (not individual files)

### 7.4 Repository Tests

- Insert/update/delete shares with `recipient_user_id` and `folder_id`
- Query shares by recipient: `WHERE recipient_user_id = ? AND revoked_at IS NULL`
- Query shares by file: `WHERE file_id = ? AND revoked_at IS NULL`
- Query shares by folder: `WHERE folder_id = ? AND revoked_at IS NULL`
- Verify indexes used (check `EXPLAIN` output)
- Insert/update/delete notifications
- Query notifications by user: `WHERE user_id = ? ORDER BY created_at DESC`
- Query unread notifications: `WHERE user_id = ? AND read = FALSE`

### 7.5 Performance Tests

- Permission resolution for deeply nested folder (10 levels): <10ms
- Share folder with 1,000 files: Single DB operation (verify with query log)
- 100 concurrent users receiving notifications: WebSocket scales, no dropped messages
- Permission check with per-request caching: Second check is instant (cached)

### 7.6 Error Case Tests

- Share with non-existent user → 404
- Share file not owned → 403
- Non-Admin adds recipient → 403
- Deleted file/folder → shares have `revoked_at` set
- Cascading revocation → all inherited permissions removed
- Permission conflict (direct + inherited) → highest permission returned

---

## 8. Migration Path

### 8.1 Database Migrations

**Migration 1: Extend shares table**

**CRITICAL:** These schema changes are BREAKING changes to the existing Phase 3B shares table. Requires coordinated deployment.

```sql
-- Step 1: Make file_id nullable (was NOT NULL in Phase 3B)
ALTER TABLE shares ALTER COLUMN file_id DROP NOT NULL;

-- Step 2: Make share_token nullable (was NOT NULL in Phase 3B)
ALTER TABLE shares ALTER COLUMN share_token DROP NOT NULL;

-- Step 3: Drop old UNIQUE constraint on share_token
ALTER TABLE shares DROP CONSTRAINT IF EXISTS shares_share_token_key;

-- Step 4: Add new columns
ALTER TABLE shares
  ADD COLUMN recipient_user_id UUID REFERENCES users(id),
  ADD COLUMN folder_id UUID REFERENCES folders(id);

-- Step 5: Add CHECK constraints
ALTER TABLE shares
  ADD CONSTRAINT check_share_target CHECK (
    (file_id IS NOT NULL AND folder_id IS NULL) OR
    (file_id IS NULL AND folder_id IS NOT NULL)
  ),
  ADD CONSTRAINT check_share_token_for_public CHECK (
    (recipient_user_id IS NULL AND share_token IS NOT NULL) OR
    (recipient_user_id IS NOT NULL)
  );

-- Step 6: Add indexes
CREATE INDEX idx_shares_recipient ON shares(recipient_user_id, revoked_at);
CREATE INDEX idx_shares_folder ON shares(folder_id, revoked_at);
-- idx_shares_file already exists from Phase 3B

-- Step 7: Create partial unique index for share_token (only for public shares)
CREATE UNIQUE INDEX idx_shares_token_unique ON shares(share_token) WHERE share_token IS NOT NULL;

-- Step 8: Update existing shares to ensure compatibility
-- All existing Phase 3B shares are public shares (recipient_user_id = NULL)
-- No data migration needed - existing shares remain valid
```

**Deployment notes:**
- This migration must be applied BEFORE deploying Phase 3A code
- Existing Phase 3B public shares continue to work (no downtime required)
- However, Phase 3B code will break if it assumes file_id/share_token are NOT NULL
- Recommended: Update Phase 3B domain model to use `Option<FileId>` and `Option<String>` first, then apply migration

**Migration 2: Create notifications table**

```sql
CREATE TABLE notifications (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  notification_type VARCHAR(50) NOT NULL,
  title VARCHAR(255) NOT NULL,
  message TEXT NOT NULL,
  resource_id UUID NOT NULL,
  resource_type VARCHAR(50) NOT NULL,
  action_url VARCHAR(500),
  read BOOLEAN DEFAULT FALSE,
  created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_user_unread ON notifications(user_id, read, created_at);
CREATE INDEX idx_resource ON notifications(resource_id, resource_type);
```

**Migration 3: Update SharePermissions enum**

**BREAKING CHANGE:** This updates enum values for existing shares. Requires code deployment coordination.

```sql
-- Step 1: Rename Read to View
UPDATE shares SET permissions = 'View' WHERE permissions = 'Read';

-- Step 2: Rename ReadWrite to Edit
UPDATE shares SET permissions = 'Edit' WHERE permissions = 'ReadWrite';

-- Step 3: Update CHECK constraint to allow Admin
-- (Assumes permissions is validated via CHECK constraint, not PostgreSQL ENUM type)
ALTER TABLE shares DROP CONSTRAINT IF EXISTS check_permissions;
ALTER TABLE shares ADD CONSTRAINT check_permissions
  CHECK (permissions IN ('View', 'Edit', 'Admin'));
```

**If using PostgreSQL ENUM type instead of VARCHAR + CHECK:**
```sql
-- Create new enum with all three values
CREATE TYPE share_permissions_new AS ENUM ('View', 'Edit', 'Admin');

-- Add temporary column
ALTER TABLE shares ADD COLUMN permissions_new share_permissions_new;

-- Migrate data
UPDATE shares SET permissions_new =
  CASE permissions::text
    WHEN 'Read' THEN 'View'::share_permissions_new
    WHEN 'ReadWrite' THEN 'Edit'::share_permissions_new
    ELSE 'View'::share_permissions_new  -- Fallback
  END;

-- Drop old column and rename new
ALTER TABLE shares DROP COLUMN permissions;
ALTER TABLE shares RENAME COLUMN permissions_new TO permissions;
ALTER TABLE shares ALTER COLUMN permissions SET NOT NULL;

-- Drop old enum type
DROP TYPE IF EXISTS share_permissions_old;
```

**Deployment coordination:**
1. Deploy migration 3
2. Wait for all Phase 3B instances to drain active requests (or use blue-green deployment)
3. Deploy Phase 3A code with updated SharePermissions enum
4. Old code will break if it references Read/ReadWrite enum variants

### 8.2 Backward Compatibility

**IMPORTANT:** Phase 3A introduces BREAKING CHANGES to the Phase 3B data model and enum definitions.

**What breaks:**
- Phase 3B code assumes `Share.file_id` and `Share.share_token` are non-nullable
- Phase 3B code uses `SharePermissions::Read` and `SharePermissions::ReadWrite` enum variants
- After migrations, these assumptions no longer hold

**Migration strategy (recommended):**

**Option A: Two-phase deployment** (safer, zero downtime)
1. **Phase "3B.5"**: Update Phase 3B codebase to use `Option<FileId>`, `Option<String>`, and support both old/new enum values
   - Change Share domain model: `file_id: Option<FileId>`, `share_token: Option<String>`
   - Add enum variants: Keep Read/ReadWrite, add View/Edit/Admin (all coexist temporarily)
   - Add migration flag check: if `recipient_user_id IS NULL`, use old behavior
2. Deploy Phase 3B.5 (fully backward compatible)
3. Run database migrations (old code still works)
4. Deploy Phase 3A (remove old enum variants, enforce new model)

**Option B: Coordinated deployment** (faster, brief downtime)
1. Schedule maintenance window
2. Stop all Phase 3B instances
3. Run database migrations
4. Deploy Phase 3A code
5. Start Phase 3A instances

**What remains compatible:**
- Public share endpoints work unchanged (`/api/public/share/{token}/*`)
- Public share token resolution works (Phase 3A checks `recipient_user_id IS NULL` to identify public shares)
- WebSocket event handling (old clients ignore unknown event types)
- Frontend gracefully handles new notification bell (feature detection)

---

## 9. Security Considerations

### 9.1 Authorization

- **Share creation**: User must own the file/folder
- **Share management**: User must have Admin permission on the share OR be the owner
- **Recipient management**: Only Admin can add/remove recipients
- **Permission updates**: Only Admin can change permissions, cannot downgrade owner
- **Notification access**: Users can only read/delete their own notifications

### 9.2 Input Validation

- Email addresses: Validate format, normalize to lowercase, trim whitespace
- Permission levels: Validate against enum (View/Edit/Admin)
- Resource IDs: Validate UUIDs, check existence in DB
- Pagination params: Limit max values (limit ≤ 100, offset ≥ 0)

### 9.3 Data Leakage Prevention

- Recipients cannot see other recipients (only Admin can via `GET /api/shares/{id}/recipients`)
- Notifications do not leak resource content (only metadata: name, type)
- WebSocket events only sent to authorized users (recipient + owner)
- Permission checks on every resource access (no caching across requests)

### 9.4 Rate Limiting

- Share creation limited to prevent spam
- Notification creation limited to prevent abuse
- Use existing Redis-backed rate limiter from Phase 3B

---

## 10. Performance Considerations

### 10.1 Database Indexes

- `idx_shares_recipient (recipient_user_id, revoked_at)`: Fast lookup of user's shares
- `idx_shares_file (file_id, revoked_at)`: Fast lookup of file shares
- `idx_shares_folder (folder_id, revoked_at)`: Fast lookup of folder shares
- `idx_user_unread (user_id, read, created_at)`: Fast unread notification count

### 10.2 Query Optimization

- Permission resolution: Cache results per request (avoid repeated tree walks)
- Folder share queries: Use recursive CTE for ancestry lookup (single query)
- Notification queries: Paginate with `LIMIT`/`OFFSET`, filter unread in DB

### 10.3 WebSocket Scaling

- Events sent only to affected users (targeted routing)
- Use Redis pub/sub for multi-instance WebSocket scaling (same as Phase 3B)
- No broadcast to all connected users (prevents thundering herd)

### 10.4 Large Folder Shares

- Single share record per folder (not per file)
- Permission checks on-demand (don't precompute for all descendants)
- Pagination for "Shared with me" view (max 100 items per page)

---

## 11. Future Enhancements (Post-Phase 3A)

### 11.1 User Invitations

- Share with unregistered emails → send invitation email
- Create pending shares → activate when user registers
- Track invitation status (pending/accepted/expired)

### 11.2 Group Sharing

- Create groups/teams with members
- Share with group → all members get access
- Group permission management

### 11.3 Share Analytics

- Track view count, last accessed time
- Show "who viewed this file" for owners
- Export access logs

### 11.4 Advanced Notifications

- Email notifications (in addition to in-app)
- Push notifications (mobile apps)
- Notification preferences (per-user settings)
- Digest mode (daily summary instead of real-time)

### 11.5 Share Link Generation

- Generate public share link from user share (convert user share → public share)
- Share with both users and anonymous (hybrid mode)

### 11.6 Expiration & Auto-Revocation

- Set expiration dates on user shares
- Auto-revoke expired shares (background job)
- Reminder notifications before expiration

---

## 12. Success Metrics

### 12.1 Functional Metrics

- Users can share files/folders with other registered users ✅
- Three permission levels work correctly (View/Edit/Admin) ✅
- Folder permissions inherit to all contents ✅
- Real-time notifications delivered via WebSocket ✅
- Persistent notifications stored and queryable ✅
- Admin users can manage recipients ✅

### 12.2 Performance Metrics

- Permission resolution: <10ms for 5-level nesting
- Share creation: <100ms end-to-end
- Notification delivery: <500ms (creation → WebSocket → client)
- API response times: p95 <200ms

### 12.3 Quality Metrics

- All unit tests passing (target: 100+ tests)
- All integration tests passing (target: 50+ scenarios)
- WebSocket tests passing (target: 10+ event scenarios)
- Code coverage: >80% for new services
- No SQL injection vulnerabilities (use parameterized queries)
- No authorization bypasses (security review passes)

---

## 13. Implementation Order

### Phase 1: Data Model & Repositories (Week 1)
1. Database migrations (extend shares, create notifications)
2. Extend Share domain model
3. Create Notification domain model
4. Extend ShareRepository (user share queries)
5. Create NotificationRepository
6. Unit tests for repositories

### Phase 2: Core Services (Week 2)
1. Implement PermissionResolver with tests
2. Implement UserShareService with tests
3. Implement NotificationService with tests
4. Integration tests for permission resolution
5. Integration tests for share creation/management

### Phase 3: API & WebSocket (Week 3)
1. User share endpoints (create, list, manage)
2. Notification endpoints (list, read, delete)
3. WebSocket event handlers (new event types)
4. API integration tests
5. WebSocket integration tests

### Phase 4: Testing & Polish (Week 4)
1. End-to-end tests (full user journeys)
2. Performance tests (permission resolution, large folders)
3. Security review (authorization, input validation)
4. Documentation (API docs, user guides)
5. Deployment preparation

---

## 14. Open Questions

None at this time. Design is complete and ready for implementation planning.

---

## 15. References

- **Phase 1 Spec**: Foundation (users, files, folders, versioning)
- **Phase 3B Spec**: Public share links (session tokens, WebSocket notifications)
- **Industry Standards**: Dropbox, Google Drive, OneDrive (email-based sharing patterns)

---

## Appendix A: Example Workflows

### Workflow 1: Share File with Colleague

1. Alice owns `report.pdf`
2. Alice clicks "Share" → enters Bob's email (bob@example.com) → selects "Edit"
3. Backend:
   - Validates Bob exists, Alice owns file
   - Creates share: `{ file_id: report.pdf, recipient_user_id: Bob, permissions: Edit }`
   - Creates notification for Bob: "Alice shared report.pdf with you"
   - Emits WebSocket event: `ShareReceivedByUser` → Bob's clients
4. Bob sees notification bell light up (real-time)
5. Bob clicks notification → navigates to `report.pdf` → can download and upload new versions

### Workflow 2: Share Folder with Team

1. Alice shares `/Projects` folder with Bob (View), Carol (Edit), Dave (Admin)
2. Backend creates 3 shares:
   - `{ folder_id: Projects, recipient_user_id: Bob, permissions: View }`
   - `{ folder_id: Projects, recipient_user_id: Carol, permissions: Edit }`
   - `{ folder_id: Projects, recipient_user_id: Dave, permissions: Admin }`
3. All contents of `/Projects` inherit permissions
4. Bob can browse/download, Carol can upload, Dave can add more users

### Workflow 3: Admin Re-Shares

1. Dave (Admin on `/Projects`) adds Eve with Edit permission
2. Backend validates Dave has Admin permission
3. Creates share for Eve: `{ folder_id: Projects, recipient_user_id: Eve, permissions: Edit }`
4. Sends notification to Eve (no notification to Alice, original owner)
5. Alice can see Eve in recipient list but didn't initiate share

### Workflow 4: Permission Inheritance

1. Alice shares `/Projects` with Bob (Edit)
2. `/Projects` contains `/Projects/2024/report.pdf`
3. Bob accesses `report.pdf`:
   - PermissionResolver checks direct share on `report.pdf` → None
   - Walks up to `/Projects/2024` → None
   - Walks up to `/Projects` → Found share with Edit permission
   - Returns Edit permission
4. Bob can upload new version of `report.pdf`

### Workflow 5: Offline Notification

1. Bob is offline (no WebSocket connection)
2. Alice shares `document.pdf` with Bob
3. Notification created in database: `{ user_id: Bob, read: false }`
4. Bob logs in 2 hours later
5. Frontend calls `GET /api/notifications` → returns unread notifications
6. Bob sees badge with unread count, clicks to view

---

## Appendix B: Database Schema Summary

```sql
-- Extended shares table (final schema)
CREATE TABLE shares (
  id UUID PRIMARY KEY,
  file_id UUID REFERENCES files(id),           -- Nullable for folder shares
  folder_id UUID REFERENCES folders(id),       -- NEW: for folder shares
  share_token VARCHAR(255),                    -- Nullable for user shares
  permissions VARCHAR(50) NOT NULL,            -- View/Edit/Admin
  password_hash VARCHAR(255),                  -- For public shares only
  expires_at TIMESTAMP,                        -- For public shares only
  access_count INTEGER DEFAULT 0,              -- For public shares only
  recipient_user_id UUID REFERENCES users(id), -- NEW: for user shares (NULL = public)
  created_by UUID NOT NULL REFERENCES users(id),
  created_at TIMESTAMP NOT NULL,
  revoked_at TIMESTAMP,
  CONSTRAINT check_share_target CHECK (
    (file_id IS NOT NULL AND folder_id IS NULL) OR
    (file_id IS NULL AND folder_id IS NOT NULL)
  ),
  CONSTRAINT check_share_token_for_public CHECK (
    (recipient_user_id IS NULL AND share_token IS NOT NULL) OR
    (recipient_user_id IS NOT NULL)
  )
);

-- Indexes
CREATE INDEX idx_shares_recipient ON shares(recipient_user_id, revoked_at);
CREATE INDEX idx_shares_file ON shares(file_id, revoked_at);
CREATE INDEX idx_shares_folder ON shares(folder_id, revoked_at);
CREATE UNIQUE INDEX idx_shares_token_unique ON shares(share_token) WHERE share_token IS NOT NULL;

-- New notifications table
CREATE TABLE notifications (
  id UUID PRIMARY KEY,
  user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  notification_type VARCHAR(50) NOT NULL,
  title VARCHAR(255) NOT NULL,
  message TEXT NOT NULL,
  resource_id UUID NOT NULL,
  resource_type VARCHAR(50) NOT NULL,
  action_url VARCHAR(500),
  read BOOLEAN DEFAULT FALSE,
  created_at TIMESTAMP NOT NULL
);

-- Indexes
CREATE INDEX idx_user_unread ON notifications(user_id, read, created_at);
CREATE INDEX idx_resource ON notifications(resource_id, resource_type);
```

---

**End of Design Specification**
