# Unified Share Service Design

**Date:** 2026-04-04  
**Status:** Implementation Complete  
**Approach:** B (Unified Service Refactor)

---

## 1. Overview

### 1.1 Purpose

Consolidate all share functionality (public links, user-to-user, group) into a single `ShareService` with consistent tenant boundaries, notification behavior, and access control.

### 1.2 Goals

- **Unified service layer**: Single `ShareService` handles all share types
- **Consistent tenant enforcement**: All share operations validate tenant boundaries
- **Complete group sharing**: Full CRUD for group shares (currently missing revoke/update)
- **Lazy notifications**: Group share members notified on first access, not share creation (with deduplication tracking)
- **Configurable recipient visibility**: Admin-only by default, tenant-configurable
- **OIDC integration ready**: Local groups mirror OIDC groups, both usable for sharing
- **Principle of least privilege**: Non-admin users can only share with groups they're members of

### 1.3 Non-Goals

- Cross-tenant sharing (deferred to future)
- External user invitations
- Complex share analytics
- Real-time group membership sync from OIDC

---

## 2. Architecture

### 2.1 Service Structure

```
backend/crates/core/src/services/
├── share_service.rs          # UNIFIED - all share operations
├── share_service/
│   ├── public_shares.rs      # Public link methods
│   ├── user_shares.rs        # User-to-user methods  
│   └── group_shares.rs       # Group share methods (NEW)
├── share_errors.rs           # Consolidated error types
└── permission_resolver.rs    # Effective permission resolution
```

### 2.2 Unified ShareService Interface

```rust
pub struct ShareService<E, M, N, G>
where
    E: EventStoreOps,
    M: MetadataStoreOps,
    N: NotificationServiceOps,
    G: GroupRepositoryOps,
{
    event_store: Arc<E>,
    metadata_store: Arc<M>,
    notification_service: Arc<N>,
    group_repo: Arc<G>,
    broadcaster: Arc<EventBroadcaster>,
}

// === Public Shares ===
impl ShareService {
    pub async fn create_public_share(
        &self,
        resource: Resource,
        created_by: UserId,
        permissions: SharePermissions,
        password: Option<String>,
        expires_at: Option<DateTime<Utc>>,
        tenant_id: Uuid,
    ) -> Result<Share, ShareError>;

    pub async fn validate_public_share(
        &self,
        token: &str,
        password: Option<String>,
    ) -> Result<ShareSession, ShareError>;
}

// === User Shares ===
impl ShareService {
    pub async fn create_user_share(
        &self,
        resource: Resource,
        recipient_email: &str,
        permissions: SharePermissions,
        created_by: UserId,
    ) -> Result<Share, ShareError>;
}

// === Group Shares (NEW) ===
impl ShareService {
    pub async fn create_group_share(
        &self,
        resource: Resource,
        group_id: Uuid,
        permissions: SharePermissions,
        created_by: UserId,
    ) -> Result<Share, ShareError>;

    pub async fn revoke_group_share(
        &self,
        share_id: ShareId,
        requesting_user: UserId,
    ) -> Result<(), ShareError>;

    pub async fn update_group_share_permission(
        &self,
        share_id: ShareId,
        new_permission: SharePermissions,
        requesting_user: UserId,
    ) -> Result<Share, ShareError>;

    /// List all group shares for a resource
    pub async fn list_group_shares(
        &self,
        resource: Resource,
        requesting_user: UserId,
    ) -> Result<Vec<GroupShareInfo>, ShareError>;
}

// === Common Operations ===
impl ShareService {
    pub async fn revoke_share(
        &self,
        share_id: ShareId,
        requesting_user: UserId,
    ) -> Result<(), ShareError>;

    /// List recipients - ADMIN ONLY sees all
    pub async fn list_recipients(
        &self,
        resource: Resource,
        requesting_user: UserId,
    ) -> Result<RecipientsList, ShareError>;

    /// List shares received by user
    pub async fn list_received_shares(
        &self,
        user_id: UserId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ShareWithResource>, ShareError>;

    /// Resolve effective permission (checks user + group shares)
    pub async fn resolve_effective_permission(
        &self,
        user_id: UserId,
        resource: Resource,
    ) -> Result<Option<SharePermissions>, ShareError>;
}
```

---

## 3. Data Model

### 3.1 Share Record (Unified)

The existing `shares` table already supports this design fully:

```rust
pub struct Share {
    pub id: ShareId,
    pub file_id: Option<FileId>,           // null for folder shares
    pub folder_id: Option<FolderId>,       // null for file shares
    pub share_token: Option<String>,       // null for user/group shares
    pub permissions: SharePermissions,     // View | Edit | Admin
    pub password_hash: Option<String>,     // public shares only
    pub expires_at: Option<DateTime<Utc>>, // public shares only
    pub upload_only: bool,                 // public shares only
    pub access_count: i32,                 // public shares only
    pub recipient_user_id: Option<UserId>, // user shares
    pub recipient_group_id: Option<Uuid>,  // group shares (NEW)
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub tenant_id: Uuid,
}
```

**Share Type Detection:**
```rust
impl Share {
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

### 3.2 Notification Behavior

| Share Type | Notification Timing | Recipients |
|------------|---------------------|------------|
| Public | N/A (anonymous) | None |
| User | Immediately on share creation | Recipient user |
| Group | **On first access** | Accessing user only |

**First-Access Notification Tracking:**

```sql
-- Tracks which users have been notified for group shares
CREATE TABLE share_access_notifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    share_id UUID NOT NULL REFERENCES shares(id) ON DELETE CASCADE,
    notified_at TIMESTAMP NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, share_id)
);

CREATE INDEX idx_share_access_notifications_user 
ON share_access_notifications(user_id);
```

**First-Access Notification Logic:**
```rust
async fn check_access_and_notify(
    &self,
    user_id: UserId,
    resource: Resource,
) -> Result<Option<SharePermissions>, ShareError> {
    let permission = self.resolve_effective_permission(user_id, resource).await?;
    
    // If access via group share and first time, send notification
    if let Some(perm) = permission {
        if self.access_via_group_share(user_id, resource).await? {
            self.send_first_access_notification_if_needed(user_id, resource).await?;
        }
    }
    
    Ok(permission)
}

async fn send_first_access_notification_if_needed(
    &self,
    user_id: UserId,
    resource: Resource,
) -> Result<(), ShareError> {
    // Check if already notified
    let already_notified = self.notification_repo
        .was_notified_for_share(user_id, resource)
        .await?;
    
    if !already_notified {
        // Send notification
        self.notification_service.create_notification(...).await?;
        
        // Record that notification was sent
        self.notification_repo
            .record_share_notification(user_id, resource)
            .await?;
    }
    
    Ok(())
}
```

---

## 4. Tenant Boundary Enforcement

### 4.1 Rules

1. **All shares are tenant-scoped**: `tenant_id` is required on all share records
2. **Same-tenant only**: Recipients (users or groups) must be in same tenant as resource
3. **Validation on creation**:
   ```rust
   if resource.tenant_id != recipient.tenant_id {
       return Err(ShareError::CrossTenantSharingNotAllowed);
   }
   ```

### 4.2 Enforcement Points

| Operation | Tenant Check |
|-----------|--------------|
| Create user share | Verify recipient user.tenant_id == resource.tenant_id |
| Create group share | Verify group.tenant_id == resource.tenant_id |
| List received shares | Filter by requesting user's tenant_id |
| Resolve permission | Implicit - resource lookup filters by tenant |

---

## 5. Group Integration

### 5.1 Group Sources

| Source | Managed By | Sync |
|--------|------------|------|
| OIDC Groups | External IdP | Periodic sync to `user_groups` table |
| Local Groups | RustShare admin | Direct DB operations |

### 5.2 Group Share Permissions

**Who can create group shares:**
- Resource owner (implicit Admin) - can share with ANY group in tenant
- Users with Admin permission on resource - can share with ANY group in tenant
- Regular users with Edit/View permission - can only share with groups they are members of

**Permission Check Logic:**
```rust
async fn can_create_group_share(
    &self,
    user_id: UserId,
    group_id: Uuid,
    resource: Resource,
) -> Result<bool, ShareError> {
    // Check if user has Admin permission on resource
    let user_perm = self.resolve_permission(user_id, resource).await?;
    let is_admin = user_perm == Some(SharePermissions::Admin);
    
    if is_admin {
        // Admins can share with any group in tenant
        return Ok(true);
    }
    
    // Non-admins must be members of the group
    let is_group_member = self.group_repo
        .is_member(user_id, group_id)
        .await?;
    
    Ok(is_group_member)
}
```

**Who can revoke/update group shares:**
- Resource owner
- Users with Admin permission on resource
- Group admin (future enhancement, not in scope)

### 5.3 Group Membership Resolution

```rust
// Always use local group_members table
async fn is_user_in_group(
    &self,
    user_id: UserId,
    group_id: Uuid,
) -> Result<bool, ShareError> {
    self.group_repo
        .is_member(user_id, group_id)
        .await
        .map_err(ShareError::Database)
}
```

---

## 6. Recipient List Behavior

### 6.1 Access Control

| User Role | Can See |
|-----------|---------|
| Regular user with share access | Only themselves (if user share) or "Group: X" (if group share) |
| Admin | All recipients including group members |

### 6.2 API Response Structure

```rust
pub struct RecipientsList {
    /// For admins: detailed list
    pub detailed: Option<Vec<RecipientDetail>>,
    /// For all users: summary
    pub summary: Vec<RecipientSummary>,
}

pub struct RecipientDetail {
    pub share_id: ShareId,
    pub recipient_type: RecipientType, // User | Group
    pub recipient_id: Uuid,
    pub email: Option<String>,          // for users
    pub group_name: Option<String>,     // for groups
    pub permission: SharePermissions,
    pub added_at: DateTime<Utc>,
}

pub struct RecipientSummary {
    pub recipient_type: RecipientType,
    pub display_name: String, // "You" | "Group: Engineering" | "user@example.com"
    pub permission: SharePermissions,
}
```

---

## 7. API Endpoints

### 7.1 Consolidated Share Endpoints

| Method | Endpoint | Auth | Description |
|--------|----------|------|-------------|
| POST | `/api/files/{id}/shares` | User | Create share (type in body) |
| POST | `/api/folders/{id}/shares` | User | Create folder share |
| GET | `/api/files/{id}/shares` | User | List shares for file |
| GET | `/api/folders/{id}/shares` | User | List shares for folder |
| DELETE | `/api/shares/{id}` | User | Revoke any share type |
| PUT | `/api/shares/{id}/permission` | Admin | Update permission |
| GET | `/api/files/{id}/recipients` | User | Get recipients (admin=full) |
| GET | `/api/folders/{id}/recipients` | User | Get recipients (admin=full) |
| GET | `/api/shares/received` | User | List my received shares |

### 7.2 Request/Response Examples

**Create Group Share:**
```http
POST /api/files/{file_id}/shares
Content-Type: application/json

{
  "share_type": "group",
  "group_id": "550e8400-e29b-41d4-a716-446655440000",
  "permission": "Edit"
}

Response 201:
{
  "share_id": "660e8400-e29b-41d4-a716-446655440001",
  "share_type": "group",
  "resource_id": "file-uuid",
  "resource_type": "file",
  "group_id": "550e8400-e29b-41d4-a716-446655440000",
  "group_name": "Engineering",
  "permission": "Edit",
  "created_at": "2026-04-04T10:00:00Z"
}
```

---

## 8. Migration Path

### 8.1 Service Consolidation Steps

1. **Create new `ShareService`** with unified interface
2. **Migrate public share logic** from existing `ShareService`
3. **Migrate user share logic** from `UserShareService`
4. **Implement group share methods** (new functionality)
5. **Update handlers** to use new unified service
6. **Deprecate old services** (keep for backward compat during transition)

### 8.2 Database Migrations

**Migration 1: Ensure tenant_id on all shares**
```sql
-- Backfill tenant_id from resource (file/folder)
UPDATE shares s
SET tenant_id = COALESCE(
    (SELECT tenant_id FROM files WHERE id = s.file_id),
    (SELECT tenant_id FROM folders WHERE id = s.folder_id)
)
WHERE tenant_id IS NULL;

-- Make tenant_id NOT NULL
ALTER TABLE shares ALTER COLUMN tenant_id SET NOT NULL;
```

**Migration 2: Add group share indexes**
```sql
CREATE INDEX idx_shares_recipient_group 
ON shares(recipient_group_id, revoked_at) 
WHERE recipient_group_id IS NOT NULL;
```

**Migration 3: Create notification tracking table**
```sql
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

**Migration 4: Add tenant sharing config**
```sql
ALTER TABLE tenants ADD COLUMN recipient_visibility TEXT DEFAULT 'AdminOnly';
-- Valid values: 'AdminOnly', 'AllRecipients', 'SameGroupOnly'
```

---

## 9. Testing Strategy

### 9.1 Unit Tests

- Share type detection (public/user/group)
- Tenant boundary validation
- Permission resolution (user + group + inheritance)
- First-access notification logic

### 9.2 Integration Tests

- Create group share → verify notification on first access only
- Regular user cannot see other recipients
- Admin can see all recipients including group members
- Cross-tenant sharing blocked
- Group share revocation removes access for all members

### 9.3 Contract Tests

- ShareCreated events emitted correctly
- PermissionResolver correctly computes effective permissions
- Tenant isolation maintained

---

## 10. Error Handling

### 10.1 ShareError Extensions

```rust
pub enum ShareError {
    // ... existing errors ...
    
    /// Cross-tenant sharing attempted
    #[error("Cross-tenant sharing is not allowed")]
    CrossTenantSharingNotAllowed,
    
    /// Group not found
    #[error("Group {0} not found")]
    GroupNotFound(Uuid),
    
    /// User not member of group (when required)
    #[error("User is not a member of group {0}")]
    NotGroupMember(Uuid),
    
    /// Group share already exists
    #[error("Group already has access to this resource")]
    GroupShareAlreadyExists,
}
```

---

## 11. Implementation Phases

### Phase 1: Foundation
- Create unified `ShareService` structure
- Migrate public share methods
- Add tenant boundary checks

### Phase 2: User Shares
- Migrate user share methods from `UserShareService`
- Update handlers
- Maintain backward compatibility

### Phase 3: Group Shares
- Implement group share CRUD
- Add lazy notification on first access
- Add admin-only recipient visibility

### Phase 4: Cleanup
- Deprecate old services
- Update tests
- Documentation

---

## 12. References

- Phase 3A User Sharing Design: `docs/superpowers/specs/2026-03-18-rustshare-phase3a-user-sharing.md`
- Phase 3B Public Sharing Design: `docs/superpowers/specs/2026-03-18-rustshare-phase3b-sharing.md`
- Architecture Design: `docs/adr/03-design.md`

---

## Implementation Notes

### Completed
- [x] Database migrations (share_access_notifications table, tenant config)
- [x] ShareType enum with Public/User/Group/Invalid variants
- [x] ShareError extensions for group shares and tenant boundaries
- [x] RecipientVisibility enum with AdminOnly/AllRecipients/SameGroupOnly
- [x] ShareNotificationRepo for tracking first-access notifications
- [x] GroupRepo methods for membership queries
- [x] TenantConfigRepo for recipient visibility settings
- [x] ShareService::create_group_share with tenant checks and permission validation
- [x] ShareService::revoke_group_share with admin checks
- [x] ShareService::update_group_share_permission with admin checks
- [x] ShareService::send_first_access_notification_if_needed for lazy notifications
- [x] PermissionResolver::resolve_permission_with_source for notification triggers
- [x] API handlers refactored to use unified ShareService
- [x] Routes added for group share revoke and update
- [x] Unit tests for ShareType
- [x] Integration test skeleton for group sharing
- [x] UserShareService deprecated in favor of ShareService

### Migration Guide

#### For code using UserShareService:

**Old:**
```rust
user_share_service.create_file_share(file_id, email, permission, created_by).await
```

**New:**
```rust
share_service.create_user_share(
    Resource::File(file_id), 
    email, 
    permission, 
    created_by
).await
```

**Old:**
```rust
user_share_service.create_folder_share(folder_id, email, permission, created_by).await
```

**New:**
```rust
share_service.create_user_share(
    Resource::Folder(folder_id), 
    email, 
    permission, 
    created_by
).await
```

#### For permission checks with notification triggers:

**Old:**
```rust
let permission = permission_resolver.resolve_permission(user_id, resource).await?;
```

**New:**
```rust
let result = permission_resolver.resolve_permission_with_source(user_id, resource).await?;
if result.source == PermissionSource::GroupShare {
    // Trigger first-access notification
    share_service.send_first_access_notification_if_needed(user_id, share).await?;
}
```

### API Changes

#### New Endpoints
- `DELETE /api/v1/shares/{id}/group` - Revoke a group share
- `PUT /api/v1/shares/{id}/group/permission` - Update group share permission

#### Modified Endpoints
- `POST /api/v1/files/{id}/share/group` - Now uses unified ShareService
- `POST /api/v1/folders/{id}/share/group` - Now uses unified ShareService

### Testing Status

| Test Type | Status | Notes |
|-----------|--------|-------|
| Unit tests | ✅ | ShareType detection tests added |
| Integration tests | 📝 | Skeleton created, needs full implementation |
| Contract tests | ⏳ | Pending |

### Known Limitations

1. **Group membership check in create_group_share**: Currently has a TODO comment. The check for non-admins to verify group membership requires access to GroupRepo, which needs to be wired into ShareService.

2. **PermissionResolver integration**: The resolve_permission_with_source method is implemented but not yet integrated into the main access control flow to automatically trigger notifications.

3. **OIDC group sync**: Groups from OIDC are expected to be synced to the local group_members table. This sync is outside the scope of this implementation.

### Next Steps

1. Complete integration tests with full test implementations
2. Integrate first-access notification triggering into the main request handling flow
3. Add group membership check to create_group_share
4. Add contract tests for share behavior
5. Monitor deprecation warnings and migrate remaining UserShareService usages
