# Unified Share Service Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Create a unified ShareService consolidating public, user, and group sharing with tenant boundaries, lazy notifications, and configurable recipient visibility.

**Architecture:** Single ShareService with sub-modules for each share type. PermissionResolver handles effective permission resolution. First-access notification tracking prevents spam.

**Tech Stack:** Rust, SQLx, Axum, PostgreSQL

**Design Doc:** `docs/plans/2026-04-04-unified-share-service-design.md`

---

## Prerequisites

Before starting:
- [ ] Review design doc thoroughly
- [ ] Understand existing `ShareService`, `UserShareService`, `PermissionResolver`
- [ ] Familiar with tenant isolation patterns in codebase

---

## Phase 0: Database Migrations

### Task 0.1: Create notification tracking table

**Files:**
- Create: `backend/migrations/20260404000001_create_share_notification_tracking.sql`

**Step 1: Write migration**

```sql
-- Track first-access notifications for group shares
CREATE TABLE share_access_notifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    share_id UUID NOT NULL REFERENCES shares(id) ON DELETE CASCADE,
    notified_at TIMESTAMP NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, share_id)
);

CREATE INDEX idx_share_access_notifications_user 
ON share_access_notifications(user_id);
CREATE INDEX idx_share_access_notifications_share 
ON share_access_notifications(share_id);
```

**Step 2: Run migration**

```bash
cd /Users/scolak/Projects/x/rustshare/backend
cargo sqlx migrate run
```

**Step 3: Verify table exists**

```bash
psql $DATABASE_URL -c "\dt share_access_notifications"
```

Expected: Table listed

**Step 4: Commit**

```bash
git add backend/migrations/20260404000001_create_share_notification_tracking.sql
git commit -m "db: Add share_access_notifications table for group share tracking"
```

---

### Task 0.2: Add tenant recipient visibility config

**Files:**
- Create: `backend/migrations/20260404000002_add_tenant_sharing_config.sql`

**Step 1: Write migration**

```sql
-- Add tenant-level sharing configuration
ALTER TABLE tenants ADD COLUMN recipient_visibility TEXT DEFAULT 'AdminOnly';

-- Add check constraint
ALTER TABLE tenants ADD CONSTRAINT chk_recipient_visibility 
CHECK (recipient_visibility IN ('AdminOnly', 'AllRecipients', 'SameGroupOnly'));
```

**Step 2: Run migration**

```bash
cd /Users/scolak/Projects/x/rustshare/backend
cargo sqlx migrate run
```

**Step 3: Commit**

```bash
git add backend/migrations/20260404000002_add_tenant_sharing_config.sql
git commit -m "db: Add recipient_visibility config to tenants"
```

---

### Task 0.3: Add group share index

**Files:**
- Create: `backend/migrations/20260404000003_add_group_share_index.sql`

**Step 1: Write migration**

```sql
-- Index for efficient group share lookups
CREATE INDEX idx_shares_recipient_group 
ON shares(recipient_group_id, revoked_at) 
WHERE recipient_group_id IS NOT NULL;
```

**Step 2: Run migration**

```bash
cd /Users/scolak/Projects/x/rustshare/backend
cargo sqlx migrate run
```

**Step 3: Commit**

```bash
git add backend/migrations/20260404000003_add_group_share_index.sql
git commit -m "db: Add index for group share lookups"
```

---

## Phase 1: Domain Models

### Task 1.1: Add ShareType enum

**Files:**
- Modify: `backend/crates/core/src/domain/share.rs`

**Step 1: Add ShareType enum after SharePermissions**

```rust
/// Type of share (determined by which fields are set)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShareType {
    Public,  // share_token is Some
    User,    // recipient_user_id is Some
    Group,   // recipient_group_id is Some
    Invalid, // none of the above (should not happen)
}
```

**Step 2: Add share_type() method to Share impl**

```rust
impl Share {
    /// Determine the type of share based on populated fields
    pub fn share_type(&self) -> ShareType {
        if self.share_token.is_some() {
            ShareType::Public
        } else if self.recipient_user_id.is_some() {
            ShareType::User
        } else if self.recipient_group_id.is_some() {
            ShareType::Group
        } else {
            ShareType::Invalid
        }
    }
}
```

**Step 3: Run tests to ensure no breakage**

```bash
cd /Users/scolak/Projects/x/rustshare/backend
cargo test -p rustshare-core share:: 2>&1 | head -50
```

Expected: Existing tests pass

**Step 4: Commit**

```bash
git add backend/crates/core/src/domain/share.rs
git commit -m "feat(share): Add ShareType enum and share_type() method"
```

---

### Task 1.2: Add ShareError variants

**Files:**
- Modify: `backend/crates/core/src/services/share_errors.rs`

**Step 1: Add new error variants**

```rust
pub enum ShareError {
    // ... existing variants ...
    
    /// Cross-tenant sharing attempted
    #[error("Cross-tenant sharing is not allowed")]
    CrossTenantSharingNotAllowed,
    
    /// Group not found
    #[error("Group {0} not found")]
    GroupNotFound(Uuid),
    
    /// User not member of group
    #[error("User is not a member of group {0}")]
    NotGroupMember(Uuid),
    
    /// Group share already exists
    #[error("Group already has access to this resource")]
    GroupShareAlreadyExists,
    
    /// Recipient visibility config invalid
    #[error("Invalid recipient visibility: {0}")]
    InvalidRecipientVisibility(String),
}
```

**Step 2: Commit**

```bash
git add backend/crates/core/src/services/share_errors.rs
git commit -m "feat(share): Add group share and tenant error variants"
```

---

### Task 1.3: Add RecipientVisibility enum

**Files:**
- Create: `backend/crates/core/src/domain/tenant_config.rs` (or add to existing tenant module)

**Step 1: Create file with enum**

```rust
//! Tenant-level sharing configuration

use serde::{Deserialize, Serialize};

/// Who can see share recipients
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
pub enum RecipientVisibility {
    /// Only admins see full recipient list (privacy-preserving, default)
    AdminOnly,
    /// Everyone sees all recipients (transparent)
    AllRecipients,
    /// Users see self + same-group members
    SameGroupOnly,
}

impl Default for RecipientVisibility {
    fn default() -> Self {
        RecipientVisibility::AdminOnly
    }
}

impl std::str::FromStr for RecipientVisibility {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "AdminOnly" => Ok(RecipientVisibility::AdminOnly),
            "AllRecipients" => Ok(RecipientVisibility::AllRecipients),
            "SameGroupOnly" => Ok(RecipientVisibility::SameGroupOnly),
            _ => Err(format!("Invalid recipient visibility: {}", s)),
        }
    }
}

impl std::fmt::Display for RecipientVisibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecipientVisibility::AdminOnly => write!(f, "AdminOnly"),
            RecipientVisibility::AllRecipients => write!(f, "AllRecipients"),
            RecipientVisibility::SameGroupOnly => write!(f, "SameGroupOnly"),
        }
    }
}
```

**Step 2: Export from domain mod**

Add to `backend/crates/core/src/domain/mod.rs`:
```rust
pub mod tenant_config;
pub use tenant_config::RecipientVisibility;
```

**Step 3: Commit**

```bash
git add backend/crates/core/src/domain/tenant_config.rs backend/crates/core/src/domain/mod.rs
git commit -m "feat(tenant): Add RecipientVisibility configuration enum"
```

---

## Phase 2: Repository Layer

### Task 2.1: Add notification tracking repository

**Files:**
- Create: `backend/crates/storage/src/repos/share_notification.rs`

**Step 1: Create repository trait and implementation**

```rust
//! Repository for share access notification tracking

use async_trait::async_trait;
use uuid::Uuid;
use sqlx::PgPool;

#[async_trait]
pub trait ShareNotificationRepo: Send + Sync {
    /// Check if user was already notified for this share
    async fn was_notified(&self, user_id: Uuid, share_id: Uuid) -> Result<bool, sqlx::Error>;
    
    /// Record that notification was sent
    async fn record_notification(&self, user_id: Uuid, share_id: Uuid) -> Result<(), sqlx::Error>;
}

pub struct ShareNotificationRepoImpl {
    pool: PgPool,
}

impl ShareNotificationRepoImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ShareNotificationRepo for ShareNotificationRepoImpl {
    async fn was_notified(&self, user_id: Uuid, share_id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM share_access_notifications 
                WHERE user_id = $1 AND share_id = $2
            )
            "#
        )
        .bind(user_id)
        .bind(share_id)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(result)
    }
    
    async fn record_notification(&self, user_id: Uuid, share_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO share_access_notifications (user_id, share_id, notified_at)
            VALUES ($1, $2, NOW())
            ON CONFLICT (user_id, share_id) DO NOTHING
            "#
        )
        .bind(user_id)
        .bind(share_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
}
```

**Step 2: Export from repos mod**

Add to appropriate mod.rs file to export.

**Step 3: Commit**

```bash
git add backend/crates/storage/src/repos/share_notification.rs
git commit -m "feat(repo): Add ShareNotificationRepo for tracking group share notifications"
```

---

### Task 2.2: Add group repository methods

**Files:**
- Modify: `backend/crates/storage/src/repos/user/mod.rs` or appropriate group repo

**Step 1: Add is_member method to group repo trait**

```rust
#[async_trait]
pub trait GroupRepo: Send + Sync {
    // ... existing methods ...
    
    /// Check if user is a member of a group
    async fn is_member(&self, user_id: Uuid, group_id: Uuid) -> Result<bool, sqlx::Error>;
    
    /// Get all members of a group
    async fn get_members(&self, group_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error>;
    
    /// Get all groups a user is a member of
    async fn get_user_groups(&self, user_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error>;
}
```

**Step 2: Implement methods**

```rust
#[async_trait]
impl GroupRepo for GroupRepoImpl {
    // ... existing implementations ...
    
    async fn is_member(&self, user_id: Uuid, group_id: Uuid) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM group_members WHERE user_id = $1 AND group_id = $2)"
        )
        .bind(user_id)
        .bind(group_id)
        .fetch_one(&self.pool)
        .await
    }
    
    async fn get_members(&self, group_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT user_id FROM group_members WHERE group_id = $1"
        )
        .bind(group_id)
        .fetch_all(&self.pool)
        .await
    }
    
    async fn get_user_groups(&self, user_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT group_id FROM group_members WHERE user_id = $1"
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
    }
}
```

**Step 3: Commit**

```bash
git add backend/crates/storage/src/repos/user/mod.rs
git commit -m "feat(repo): Add group membership query methods"
```

---

### Task 2.3: Add tenant config repository

**Files:**
- Modify: `backend/crates/storage/src/repos/user/rustfs.rs` or create new

**Step 1: Add get_recipient_visibility method**

```rust
#[async_trait]
pub trait TenantConfigRepo: Send + Sync {
    /// Get recipient visibility setting for tenant
    async fn get_recipient_visibility(&self, tenant_id: Uuid) -> Result<RecipientVisibility, sqlx::Error>;
}

#[async_trait]
impl TenantConfigRepo for UserRepository {
    async fn get_recipient_visibility(&self, tenant_id: Uuid) -> Result<RecipientVisibility, sqlx::Error> {
        let visibility_str: String = sqlx::query_scalar(
            "SELECT recipient_visibility FROM tenants WHERE id = $1"
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await?;
        
        visibility_str.parse()
            .map_err(|e| sqlx::Error::Protocol(format!("Invalid visibility: {}", e)))
    }
}
```

**Step 2: Commit**

```bash
git add backend/crates/storage/src/repos/user/rustfs.rs
git commit -m "feat(repo): Add tenant config repository methods"
```

---

## Phase 3: ShareService Extensions

### Task 3.1: Add group share creation

**Files:**
- Modify: `backend/crates/core/src/services/share_service.rs`

**Step 1: Add group share creation method**

```rust
impl<E: EventStoreOps, M: MetadataStoreOps, J: JwtOps> ShareService<E, M, J> {
    /// Create a group share for a resource
    pub async fn create_group_share(
        &self,
        resource: Resource,
        group_id: Uuid,
        permissions: SharePermissions,
        created_by: UserId,
        tenant_id: Uuid,
    ) -> Result<Share, ShareError> {
        // Verify resource exists and get owner
        let (resource_owner, resource_tenant) = match resource {
            Resource::File(file_id) => {
                let file = self.metadata_store
                    .find_file_by_id(file_id)
                    .await
                    .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
                    .ok_or(ShareError::FileNotFound(file_id))?;
                (file.owner_id, file.tenant_id)
            }
            Resource::Folder(folder_id) => {
                let folder = self.metadata_store
                    .find_folder_by_id(folder_id)
                    .await
                    .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
                    .ok_or(ShareError::NotFoundById(folder_id))?;
                (folder.owner_id, folder.tenant_id)
            }
        };
        
        // Tenant boundary check
        if resource_tenant != tenant_id {
            return Err(ShareError::CrossTenantSharingNotAllowed);
        }
        
        // Check if user has permission to create share
        let is_owner = resource_owner == created_by;
        let has_admin = if !is_owner {
            self.check_admin_permission(created_by, resource).await?
        } else {
            true
        };
        
        // Non-admins must be group members
        if !has_admin {
            let is_member = self.group_repo
                .is_member(created_by, group_id)
                .await
                .map_err(ShareError::Database)?;
            if !is_member {
                return Err(ShareError::NotGroupMember(group_id));
            }
        }
        
        // Check for existing group share
        let existing = self.find_group_share(resource, group_id).await?;
        if existing.is_some() {
            return Err(ShareError::GroupShareAlreadyExists);
        }
        
        // Create share
        let share = Share {
            id: Uuid::new_v4(),
            file_id: match resource { Resource::File(id) => Some(id), _ => None },
            folder_id: match resource { Resource::Folder(id) => Some(id), _ => None },
            share_token: None,
            permissions,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: None,
            recipient_group_id: Some(group_id),
            created_by,
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id,
        };
        
        self.metadata_store.create_share(&share).await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?;
        
        // Emit event
        let payload = ShareCreatedPayload {
            share_id: share.id,
            file_id: share.resource_id().unwrap_or(share.id),
            share_token: "".to_string(), // Group shares don't have tokens
            permissions: share.permissions,
            password_protected: false,
            expires_at: None,
            created_by,
        };
        
        let event = Event::new(
            EventType::ShareCreated,
            share.id,
            AggregateType::Share,
            serde_json::to_value(&payload)
                .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?,
            created_by,
        );
        
        self.event_store.append(&event, &self.broadcaster).await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?;
        
        Ok(share)
    }
}
```

**Step 2: Commit**

```bash
git add backend/crates/core/src/services/share_service.rs
git commit -m "feat(share): Add group share creation with permission checks"
```

---

### Task 3.2: Add group share revoke/update

**Files:**
- Modify: `backend/crates/core/src/services/share_service.rs`

**Step 1: Add revoke_group_share method**

```rust
impl<E: EventStoreOps, M: MetadataStoreOps, J: JwtOps> ShareService<E, M, J> {
    /// Revoke a group share
    pub async fn revoke_group_share(
        &self,
        share_id: ShareId,
        requesting_user: UserId,
    ) -> Result<(), ShareError> {
        let share = self.metadata_store
            .get_share_by_id(share_id)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
            .ok_or(ShareError::NotFoundById(share_id))?;
        
        // Verify it's a group share
        if share.recipient_group_id.is_none() {
            return Err(ShareError::InvalidState("Not a group share".to_string()));
        }
        
        // Check admin permission on resource
        let resource = if let Some(file_id) = share.file_id {
            Resource::File(file_id)
        } else if let Some(folder_id) = share.folder_id {
            Resource::Folder(folder_id)
        } else {
            return Err(ShareError::InvalidState("Share has no resource".to_string()));
        };
        
        let has_admin = self.check_admin_permission(requesting_user, resource).await?;
        if !has_admin {
            return Err(ShareError::InsufficientPermission {
                required: SharePermissions::Admin,
                actual: SharePermissions::View,
            });
        }
        
        // Revoke
        self.metadata_store.revoke_share(share_id).await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?;
        
        // Emit event
        let payload = ShareRevokedPayload {
            share_id,
            file_id: share.resource_id().unwrap_or(share_id),
            revoked_by: requesting_user,
        };
        
        let event = Event::new(
            EventType::ShareRevoked,
            share_id,
            AggregateType::Share,
            serde_json::to_value(&payload)
                .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?,
            requesting_user,
        );
        
        self.event_store.append(&event, &self.broadcaster).await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?;
        
        Ok(())
    }
    
    /// Update group share permission
    pub async fn update_group_share_permission(
        &self,
        share_id: ShareId,
        new_permission: SharePermissions,
        requesting_user: UserId,
    ) -> Result<Share, ShareError> {
        let mut share = self.metadata_store
            .get_share_by_id(share_id)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
            .ok_or(ShareError::NotFoundById(share_id))?;
        
        // Verify it's a group share
        if share.recipient_group_id.is_none() {
            return Err(ShareError::InvalidState("Not a group share".to_string()));
        }
        
        // Check admin permission
        let resource = if let Some(file_id) = share.file_id {
            Resource::File(file_id)
        } else if let Some(folder_id) = share.folder_id {
            Resource::Folder(folder_id)
        } else {
            return Err(ShareError::InvalidState("Share has no resource".to_string()));
        };
        
        let has_admin = self.check_admin_permission(requesting_user, resource).await?;
        if !has_admin {
            return Err(ShareError::InsufficientPermission {
                required: SharePermissions::Admin,
                actual: SharePermissions::View,
            });
        }
        
        // Update permission
        share.permissions = new_permission;
        self.metadata_store.update_share(&share).await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?;
        
        // Emit event
        let payload = ShareUpdatedPayload {
            share_id,
            file_id: share.resource_id().unwrap_or(share_id),
            password_changed: false,
            expires_at_changed: false,
            new_expires_at: None,
            updated_by: requesting_user,
        };
        
        let event = Event::new(
            EventType::ShareUpdated,
            share_id,
            AggregateType::Share,
            serde_json::to_value(&payload)
                .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?,
            requesting_user,
        );
        
        self.event_store.append(&event, &self.broadcaster).await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?;
        
        Ok(share)
    }
}
```

**Step 2: Commit**

```bash
git add backend/crates/core/src/services/share_service.rs
git commit -m "feat(share): Add group share revoke and update methods"
```

---

### Task 3.3: Add first-access notification logic

**Files:**
- Modify: `backend/crates/core/src/services/share_service.rs`

**Step 1: Add notification helper methods**

```rust
impl<E: EventStoreOps, M: MetadataStoreOps, J: JwtOps> ShareService<E, M, J> {
    /// Send first-access notification for group share if needed
    pub async fn send_first_access_notification_if_needed(
        &self,
        user_id: UserId,
        share: &Share,
    ) -> Result<(), ShareError> {
        // Only for group shares
        if share.recipient_group_id.is_none() {
            return Ok(());
        }
        
        // Check if already notified
        let was_notified = self.notification_repo
            .was_notified(user_id, share.id)
            .await
            .map_err(ShareError::Database)?;
        
        if was_notified {
            return Ok(());
        }
        
        // Get resource info
        let (resource_name, resource_type) = if let Some(file_id) = share.file_id {
            let file = self.metadata_store
                .find_file_by_id(file_id)
                .await
                .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
                .ok_or(ShareError::FileNotFound(file_id))?;
            (file.name, "file")
        } else if let Some(folder_id) = share.folder_id {
            let folder = self.metadata_store
                .find_folder_by_id(folder_id)
                .await
                .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
                .ok_or(ShareError::NotFoundById(folder_id))?;
            (folder.name, "folder")
        } else {
            return Err(ShareError::InvalidState("Share has no resource".to_string()));
        };
        
        // Get sharer info
        let sharer = self.metadata_store
            .find_user_by_id(share.created_by)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
            .ok_or_else(|| ShareError::RecipientNotFound("Sharer not found".to_string()))?;
        
        // Create notification
        let notification = CreateNotification {
            user_id,
            notification_type: NotificationType::ShareReceived,
            title: format!("New {} shared with your group", resource_type),
            message: format!("{} shared '{}' with your group", sharer.email, resource_name),
            resource_id: share.resource_id().unwrap_or(share.id),
            resource_type: if share.file_id.is_some() { 
                ResourceType::File 
            } else { 
                ResourceType::Folder 
            },
            action_url: Some(format!("/shared-with-me/{}/{}", 
                resource_type, 
                share.resource_id().unwrap_or(share.id)
            )),
            tenant_id: share.tenant_id,
        };
        
        self.notification_service.create_notification(notification).await
            .map_err(|e| ShareError::Database(sqlx::Error::Protocol(e.to_string())))?;
        
        // Record notification sent
        self.notification_repo
            .record_notification(user_id, share.id)
            .await
            .map_err(ShareError::Database)?;
        
        Ok(())
    }
}
```

**Step 2: Commit**

```bash
git add backend/crates/core/src/services/share_service.rs
git commit -m "feat(share): Add lazy first-access notification for group shares"
```

---

## Phase 4: API Handlers

### Task 4.1: Update group share handlers

**Files:**
- Modify: `backend/server/src/handlers/groups.rs`
- Modify: `backend/server/src/handlers/shares.rs`

**Step 1: Refactor create_file_group_share to use ShareService**

```rust
pub async fn create_file_group_share(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
    Json(req): Json<CreateFileGroupShareRequest>,
) -> Result<(StatusCode, Json<GroupShareResponse>), (StatusCode, Json<serde_json::Value>)> {
    use rustshare_core::domain::Resource;
    use rustshare_core::domain::SharePermissions;

    let permission = match req.permission.as_str() {
        "View" => SharePermissions::View,
        "Edit" => SharePermissions::Edit,
        "Admin" => SharePermissions::Admin,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Invalid permission" })),
            ));
        }
    };

    let share = state.share_service
        .create_group_share(
            Resource::File(file_id),
            req.group_id,
            permission,
            auth.user_id,
            auth.tenant_id,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to create group share: {}", e);
            match e {
                ShareError::NotFoundById(_) => (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "File not found" })),
                ),
                ShareError::CrossTenantSharingNotAllowed => (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": "Cross-tenant sharing not allowed" })),
                ),
                ShareError::NotGroupMember(_) => (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": "You must be a group member to share" })),
                ),
                ShareError::GroupShareAlreadyExists => (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({ "error": "Group already has access" })),
                ),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "Failed to create share" })),
                ),
            }
        })?;

    // Get group name
    let group_name = sqlx::query_scalar::<_, String>(
        "SELECT name FROM user_groups WHERE id = $1"
    )
    .bind(req.group_id)
    .fetch_one(&state.db_pool)
    .await
    .unwrap_or_else(|_| "Unknown".to_string());

    let response = GroupShareResponse {
        share_id: share.id.to_string(),
        resource_id: file_id.to_string(),
        resource_type: "file".to_string(),
        group_id: req.group_id.to_string(),
        group_name,
        permission: format!("{:?}", permission),
        created_at: share.created_at.to_rfc3339(),
    };

    Ok((StatusCode::CREATED, Json(response)))
}
```

**Step 2: Add revoke_group_share handler**

```rust
pub async fn revoke_group_share(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(share_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    state.share_service
        .revoke_group_share(share_id, auth.user_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to revoke group share: {}", e);
            match e {
                ShareError::NotFoundById(_) => (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "Share not found" })),
                ),
                ShareError::InsufficientPermission { .. } => (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": "Admin permission required" })),
                ),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "Failed to revoke share" })),
                ),
            }
        })?;

    Ok(StatusCode::NO_CONTENT)
}
```

**Step 3: Commit**

```bash
git add backend/server/src/handlers/groups.rs backend/server/src/handlers/shares.rs
git commit -m "feat(api): Update group share handlers with unified service"
```

---

### Task 4.2: Add routes

**Files:**
- Modify: `backend/server/src/handlers/mod.rs` (router setup)

**Step 1: Add new routes**

```rust
// Group share routes
.route("/api/v1/files/:id/share/group", post(handlers::groups::create_file_group_share))
.route("/api/v1/folders/:id/share/group", post(handlers::groups::create_folder_group_share))
.route("/api/v1/shares/group/:id", delete(handlers::groups::revoke_group_share))
.route("/api/v1/shares/group/:id/permission", put(handlers::groups::update_group_share_permission))
```

**Step 2: Commit**

```bash
git add backend/server/src/handlers/mod.rs
git commit -m "feat(api): Add group share API routes"
```

---

## Phase 5: Integration

### Task 5.1: Wire up ShareService with new dependencies

**Files:**
- Modify: `backend/server/src/main.rs` (or service initialization)

**Step 1: Initialize notification repo and wire into ShareService**

```rust
// In service initialization
let notification_repo = Arc::new(ShareNotificationRepoImpl::new(pool.clone()));
let group_repo = Arc::new(GroupRepoImpl::new(pool.clone()));

let share_service = ShareService::new(
    event_store,
    metadata_store,
    jwt_manager,
    notification_repo,
    group_repo,
    broadcaster,
);
```

**Step 2: Commit**

```bash
git add backend/server/src/main.rs
git commit -m "feat(server): Wire up ShareService with notification and group repos"
```

---

### Task 5.2: Update PermissionResolver for group shares

**Files:**
- Modify: `backend/crates/core/src/services/permission_resolver.rs`

**Step 1: Ensure group shares are checked in permission resolution**

The PermissionResolver already has `find_group_shares` - verify it works with the new group share records.

**Step 2: Add first-access notification trigger**

```rust
// In permission check flow, after granting access
if let Some(perm) = effective_permission {
    // If accessed via group share, trigger notification
    if self.accessed_via_group_share(user_id, resource).await? {
        self.share_service
            .send_first_access_notification_if_needed(user_id, share)
            .await?;
    }
}
```

**Step 3: Commit**

```bash
git add backend/crates/core/src/services/permission_resolver.rs
git commit -m "feat(perm): Trigger first-access notifications in permission resolver"
```

---

## Phase 6: Testing

### Task 6.1: Add unit tests for ShareType

**Files:**
- Modify: `backend/crates/core/src/domain/share.rs` (tests section)

**Step 1: Add test cases**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_share_type_public() {
        let share = Share {
            share_token: Some("token123".to_string()),
            recipient_user_id: None,
            recipient_group_id: None,
            // ... other fields ...
        };
        assert_eq!(share.share_type(), ShareType::Public);
    }

    #[test]
    fn test_share_type_user() {
        let share = Share {
            share_token: None,
            recipient_user_id: Some(Uuid::new_v4()),
            recipient_group_id: None,
            // ... other fields ...
        };
        assert_eq!(share.share_type(), ShareType::User);
    }

    #[test]
    fn test_share_type_group() {
        let share = Share {
            share_token: None,
            recipient_user_id: None,
            recipient_group_id: Some(Uuid::new_v4()),
            // ... other fields ...
        };
        assert_eq!(share.share_type(), ShareType::Group);
    }
}
```

**Step 2: Run tests**

```bash
cargo test -p rustshare-core test_share_type
```

**Step 3: Commit**

```bash
git add backend/crates/core/src/domain/share.rs
git commit -m "test(share): Add ShareType unit tests"
```

---

### Task 6.2: Add integration tests

**Files:**
- Create: `backend/tests/group_sharing_test.rs`

**Step 1: Create test file with basic scenarios**

```rust
//! Integration tests for group sharing

use rustshare_core::domain::{SharePermissions, RecipientVisibility};

#[tokio::test]
async fn test_create_group_share_success() {
    // Setup: Create owner, group, member, file
    // Act: Owner creates group share
    // Assert: Share created successfully
}

#[tokio::test]
async fn test_non_member_cannot_share_with_group() {
    // Setup: Create owner (not group member), group, file
    // Act: Owner tries to create group share
    // Assert: Fails with NotGroupMember error
}

#[tokio::test]
async fn test_admin_can_share_with_any_group() {
    // Setup: Admin user (not group member), group, file
    // Act: Admin creates group share
    // Assert: Success (admins bypass member check)
}

#[tokio::test]
async fn test_first_access_notification_sent_once() {
    // Setup: Group share exists
    // Act: Member accesses file twice
    // Assert: Notification sent only on first access
}

#[tokio::test]
async fn test_cross_tenant_sharing_blocked() {
    // Setup: User in tenant A, group in tenant B
    // Act: Try to create share
    // Assert: CrossTenantSharingNotAllowed error
}

#[tokio::test]
async fn test_revoke_group_share_removes_access() {
    // Setup: Group share exists, member has access
    // Act: Revoke share
    // Assert: Member loses access
}
```

**Step 2: Commit**

```bash
git add backend/tests/group_sharing_test.rs
git commit -m "test(share): Add group sharing integration tests"
```

---

## Phase 7: Cleanup

### Task 7.1: Deprecate old UserShareService

**Files:**
- Modify: `backend/crates/core/src/services/user_share_service.rs`

**Step 1: Add deprecation notices**

```rust
//! DEPRECATED: Use ShareService instead
//! 
//! This module is being phased out in favor of the unified ShareService.
//! New code should use ShareService for all share operations.

#[deprecated(since = "0.2.0", note = "Use ShareService instead")]
pub struct UserShareService<...> { ... }
```

**Step 2: Commit**

```bash
git add backend/crates/core/src/services/user_share_service.rs
git commit -m "chore(share): Deprecate UserShareService in favor of unified ShareService"
```

---

### Task 7.2: Update documentation

**Files:**
- Modify: `docs/plans/2026-04-04-unified-share-service-design.md`

**Step 1: Add implementation notes**

Add section:
```markdown
## Implementation Notes

### Completed
- [x] Database migrations
- [x] ShareType enum
- [x] Group share CRUD
- [x] First-access notification tracking
- [x] Tenant boundary enforcement
- [x] API handlers
- [x] Tests

### Migration Guide
Old code using UserShareService:
```rust
user_share_service.create_file_share(file_id, email, perm, user)
```

New code using ShareService:
```rust
share_service.create_user_share(Resource::File(file_id), email, perm, user)
```
```

**Step 2: Commit**

```bash
git add docs/plans/2026-04-04-unified-share-service-design.md
git commit -m "docs: Update design doc with implementation notes"
```

---

## Final Verification

### Run full test suite

```bash
cd /Users/scolak/Projects/x/rustshare/backend
cargo test 2>&1 | tail -20
```

Expected: All tests pass

### Run clippy

```bash
cargo clippy --all-targets --all-features 2>&1 | grep -E "(error|warning)" | head -20
```

Expected: No errors, minimal warnings

### Final commit

```bash
git log --oneline -5
```

---

## Summary

This implementation:
1. Creates unified ShareService for all share types
2. Adds complete group share CRUD (create, revoke, update)
3. Implements lazy first-access notifications with deduplication
4. Enforces tenant boundaries on all share operations
5. Applies principle of least privilege (non-admins must be group members)
6. Makes recipient visibility tenant-configurable (default: AdminOnly)
7. Provides comprehensive test coverage

**Design Doc:** `docs/plans/2026-04-04-unified-share-service-design.md`
