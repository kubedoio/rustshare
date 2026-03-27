//! Versioned metadata document schemas
//!
//! All schemas include a `schema_version` field for forward/backward compatibility.
//! When reading, old versions are migrated to the current version.
//! When writing, always use the current version.

use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// Schema Version Constants
// ============================================================================

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

// ============================================================================
// Folder Document
// ============================================================================

/// Folder metadata document (canonical)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FolderDocument {
    /// Schema version for migration support
    pub schema_version: u32,
    /// Unique folder identifier
    pub id: Uuid,
    /// Namespace for multi-tenancy
    pub namespace_id: Uuid,
    /// Parent folder (None for root)
    pub parent_id: Option<Uuid>,
    /// Folder name (no path separators)
    pub name: String,
    /// Full path (computed, for convenience)
    pub path: String,
    /// Owner user ID
    pub owner_id: Uuid,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
    /// Document version for optimistic concurrency
    pub version: u64,
    /// Soft delete flag
    pub deleted: bool,
}

impl FolderDocument {
    /// Create a new folder document
    pub fn new(
        id: Uuid,
        namespace_id: Uuid,
        parent_id: Option<Uuid>,
        name: String,
        path: String,
        owner_id: Uuid,
    ) -> Self {
        let now = Utc::now();
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            id,
            namespace_id,
            parent_id,
            name,
            path,
            owner_id,
            created_at: now,
            updated_at: now,
            version: 1,
            deleted: false,
        }
    }
    
    /// Create a root folder for a user
    pub fn new_root(namespace_id: Uuid, owner_id: Uuid) -> Self {
        let id = Uuid::new_v4();
        Self::new(
            id,
            namespace_id,
            None,
            "Root".to_string(),
            "/".to_string(),
            owner_id,
        )
    }
    
    /// Create a child folder
    pub fn new_child(
        namespace_id: Uuid,
        parent_id: Uuid,
        parent_path: &str,
        name: String,
        owner_id: Uuid,
    ) -> Self {
        let id = Uuid::new_v4();
        let path = if parent_path == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", parent_path, name)
        };
        Self::new(id, namespace_id, Some(parent_id), name, path, owner_id)
    }
    
    /// Increment version on mutation
    pub fn bump_version(&mut self) {
        self.version += 1;
        self.updated_at = Utc::now();
    }
    
    /// Validate folder name
    pub fn validate_name(name: &str) -> Result<(), String> {
        if name.is_empty() {
            return Err("Folder name cannot be empty".to_string());
        }
        if name.contains('/') {
            return Err("Folder name cannot contain '/'".to_string());
        }
        if name.contains('\0') {
            return Err("Folder name cannot contain null character".to_string());
        }
        Ok(())
    }
}

// ============================================================================
// File Document
// ============================================================================

/// File head document (canonical metadata)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileDocument {
    /// Schema version
    pub schema_version: u32,
    /// Unique file identifier
    pub id: Uuid,
    /// Namespace for multi-tenancy
    pub namespace_id: Uuid,
    /// Parent folder (None for root)
    pub parent_id: Option<Uuid>,
    /// File name
    pub name: String,
    /// Full path (computed, for convenience)
    pub path: String,
    /// Owner user ID
    pub owner_id: Uuid,
    /// Current version ID (points to FileVersionDocument)
    pub current_version_id: Uuid,
    /// Version number (incremented on each update)
    pub version_number: i32,
    /// File size in bytes
    pub size: i64,
    /// MIME type
    pub mime_type: String,
    /// Content reference (e.g., "sha256:abc123...")
    pub content_ref: String,
    /// Content checksum for verification
    pub checksum: String,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
    /// Document version for optimistic concurrency
    pub version: u64,
    /// Soft delete flag
    pub deleted: bool,
}

impl FileDocument {
    /// Create a new file document
    pub fn new(
        id: Uuid,
        namespace_id: Uuid,
        parent_id: Option<Uuid>,
        name: String,
        path: String,
        owner_id: Uuid,
        version_id: Uuid,
        size: i64,
        mime_type: String,
        content_hash: String,
    ) -> Self {
        let now = Utc::now();
        let content_ref = format!("sha256:{}", content_hash);
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            id,
            namespace_id,
            parent_id,
            name,
            path,
            owner_id,
            current_version_id: version_id,
            version_number: 1,
            size,
            mime_type,
            content_ref: content_ref.clone(),
            checksum: content_hash,
            created_at: now,
            updated_at: now,
            version: 1,
            deleted: false,
        }
    }
    
    /// Update file with new version
    pub fn update_version(
        &mut self,
        version_id: Uuid,
        size: i64,
        content_hash: String,
        mime_type: Option<String>,
    ) {
        self.current_version_id = version_id;
        self.version_number += 1;
        self.size = size;
        self.content_ref = format!("sha256:{}", content_hash);
        self.checksum = content_hash;
        if let Some(mime) = mime_type {
            self.mime_type = mime;
        }
        self.bump_version();
    }
    
    /// Rename file (updates path)
    pub fn rename(&mut self, new_name: String, new_path: String) {
        self.name = new_name;
        self.path = new_path;
        self.bump_version();
    }
    
    /// Move file to new parent
    pub fn move_to(&mut self, new_parent_id: Option<Uuid>, new_path: String) {
        self.parent_id = new_parent_id;
        self.path = new_path;
        self.bump_version();
    }
    
    /// Increment version on mutation
    pub fn bump_version(&mut self) {
        self.version += 1;
        self.updated_at = Utc::now();
    }
    
    /// Validate file name
    pub fn validate_name(name: &str) -> Result<(), String> {
        if name.is_empty() {
            return Err("File name cannot be empty".to_string());
        }
        if name.contains('/') {
            return Err("File name cannot contain '/'".to_string());
        }
        if name.contains('\0') {
            return Err("File name cannot contain null character".to_string());
        }
        Ok(())
    }
    
    /// Extract the hash from content_ref
    pub fn content_hash(&self) -> &str {
        self.content_ref
            .strip_prefix("sha256:")
            .unwrap_or(&self.content_ref)
    }
}

// ============================================================================
// File Version Document
// ============================================================================

/// File version document (immutable snapshot)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileVersionDocument {
    /// Schema version
    pub schema_version: u32,
    /// Unique version identifier
    pub id: Uuid,
    /// Parent file ID
    pub file_id: Uuid,
    /// Version number within the file
    pub version_number: i32,
    /// Content reference
    pub content_ref: String,
    /// File size
    pub size: i64,
    /// Content checksum
    pub checksum: String,
    /// User who created this version
    pub created_by: Uuid,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Optional change description
    pub change_description: Option<String>,
}

impl FileVersionDocument {
    /// Create a new file version
    pub fn new(
        id: Uuid,
        file_id: Uuid,
        version_number: i32,
        content_hash: String,
        size: i64,
        created_by: Uuid,
        change_description: Option<String>,
    ) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            id,
            file_id,
            version_number,
            content_ref: format!("sha256:{}", content_hash),
            size,
            checksum: content_hash,
            created_by,
            created_at: Utc::now(),
            change_description,
        }
    }
    
    /// Extract the hash from content_ref
    pub fn content_hash(&self) -> &str {
        self.content_ref
            .strip_prefix("sha256:")
            .unwrap_or(&self.content_ref)
    }
}

// ============================================================================
// Share Document
// ============================================================================

/// Share permission levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SharePermission {
    View,
    Edit,
    Admin,
}

impl SharePermission {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::View => "view",
            Self::Edit => "edit",
            Self::Admin => "admin",
        }
    }
}

impl Default for SharePermission {
    fn default() -> Self {
        Self::View
    }
}

/// Share scope (public link vs user share)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShareScope {
    Public,
    User,
}

impl Default for ShareScope {
    fn default() -> Self {
        Self::Public
    }
}

/// Share document (canonical)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShareDocument {
    /// Schema version
    pub schema_version: u32,
    /// Unique share identifier
    pub id: Uuid,
    /// Resource type being shared
    pub resource_type: String, // "file" or "folder"
    /// Resource ID being shared
    pub resource_id: Uuid,
    /// Share scope
    pub scope: ShareScope,
    /// Permission level
    pub permissions: SharePermission,
    /// Token hash for public shares (None for user shares)
    pub token_hash: Option<String>,
    /// Recipient user ID for user shares (None for public)
    pub recipient_user_id: Option<Uuid>,
    /// Password hash for protected shares
    pub password_hash: Option<String>,
    /// Expiration time
    pub expires_at: Option<DateTime<Utc>>,
    /// Upload-only flag for public folder shares
    pub upload_only: bool,
    /// Access count for public shares
    pub access_count: i32,
    /// Creator user ID
    pub created_by: Uuid,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Revocation timestamp
    pub revoked_at: Option<DateTime<Utc>>,
    /// Document version for optimistic concurrency
    pub version: u64,
}

impl ShareDocument {
    /// Create a new public share
    pub fn new_public(
        id: Uuid,
        resource_type: String,
        resource_id: Uuid,
        permissions: SharePermission,
        token_hash: String,
        password_hash: Option<String>,
        expires_at: Option<DateTime<Utc>>,
        created_by: Uuid,
    ) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            id,
            resource_type,
            resource_id,
            scope: ShareScope::Public,
            permissions,
            token_hash: Some(token_hash),
            recipient_user_id: None,
            password_hash,
            expires_at,
            upload_only: false,
            access_count: 0,
            created_by,
            created_at: Utc::now(),
            revoked_at: None,
            version: 1,
        }
    }
    
    /// Create a new user share
    pub fn new_user_share(
        id: Uuid,
        resource_type: String,
        resource_id: Uuid,
        permissions: SharePermission,
        recipient_user_id: Uuid,
        created_by: Uuid,
    ) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            id,
            resource_type,
            resource_id,
            scope: ShareScope::User,
            permissions,
            token_hash: None,
            recipient_user_id: Some(recipient_user_id),
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            created_by,
            created_at: Utc::now(),
            revoked_at: None,
            version: 1,
        }
    }
    
    /// Check if share is expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires) = self.expires_at {
            Utc::now() > expires
        } else {
            false
        }
    }
    
    /// Check if share is revoked
    pub fn is_revoked(&self) -> bool {
        self.revoked_at.is_some()
    }
    
    /// Check if share is active (not revoked, not expired)
    pub fn is_active(&self) -> bool {
        !self.is_revoked() && !self.is_expired()
    }
    
    /// Revoke the share
    pub fn revoke(&mut self) {
        self.revoked_at = Some(Utc::now());
        self.bump_version();
    }
    
    /// Increment access count
    pub fn record_access(&mut self) {
        self.access_count += 1;
    }
    
    /// Bump version
    pub fn bump_version(&mut self) {
        self.version += 1;
    }
}

// ============================================================================
// Event Document
// ============================================================================

/// Event types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    FileUploaded,
    FileModified,
    FileMoved,
    FileRenamed,
    FileDeleted,
    FileRestored,
    FolderCreated,
    FolderMoved,
    FolderRenamed,
    FolderDeleted,
    FolderRestored,
    ShareCreated,
    ShareRevoked,
    ShareUpdated,
}

/// Event document (append-only)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventDocument {
    /// Schema version
    pub schema_version: u32,
    /// Unique event identifier
    pub id: Uuid,
    /// Event type
    pub event_type: EventType,
    /// Actor who performed the action
    pub actor_id: Uuid,
    /// Resource type affected
    pub resource_type: String,
    /// Resource ID affected
    pub resource_id: Uuid,
    /// Event timestamp
    pub occurred_at: DateTime<Utc>,
    /// Correlation ID for request tracing
    pub correlation_id: Option<Uuid>,
    /// Event payload (type-specific)
    pub payload: serde_json::Value,
}

impl EventDocument {
    /// Create a new event
    pub fn new(
        event_type: EventType,
        actor_id: Uuid,
        resource_type: String,
        resource_id: Uuid,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            id: Uuid::new_v4(),
            event_type,
            actor_id,
            resource_type,
            resource_id,
            occurred_at: Utc::now(),
            correlation_id: None,
            payload,
        }
    }
    
    /// Set correlation ID
    pub fn with_correlation_id(mut self, correlation_id: Uuid) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }
}

// ============================================================================
// Tombstone Document
// ============================================================================

/// Tombstone for soft-deleted resources
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TombstoneDocument {
    /// Schema version
    pub schema_version: u32,
    /// Tombstone ID (same as resource ID)
    pub id: Uuid,
    /// Resource type
    pub resource_type: String,
    /// Original resource ID
    pub resource_id: Uuid,
    /// Deletion timestamp
    pub deleted_at: DateTime<Utc>,
    /// User who deleted
    pub deleted_by: Uuid,
    /// Previous parent ID (for restore)
    pub previous_parent_id: Option<Uuid>,
    /// Original path (for restore)
    pub original_path: Option<String>,
    /// Serialized original document (for restore)
    pub restore_data: Option<serde_json::Value>,
}

impl TombstoneDocument {
    /// Create a tombstone from a file document
    pub fn from_file(file: &FileDocument, deleted_by: Uuid) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            id: file.id,
            resource_type: "file".to_string(),
            resource_id: file.id,
            deleted_at: Utc::now(),
            deleted_by,
            previous_parent_id: file.parent_id,
            original_path: Some(file.path.clone()),
            restore_data: Some(serde_json::to_value(file).unwrap_or_default()),
        }
    }
    
    /// Create a tombstone from a folder document
    pub fn from_folder(folder: &FolderDocument, deleted_by: Uuid) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            id: folder.id,
            resource_type: "folder".to_string(),
            resource_id: folder.id,
            deleted_at: Utc::now(),
            deleted_by,
            previous_parent_id: folder.parent_id,
            original_path: Some(folder.path.clone()),
            restore_data: Some(serde_json::to_value(folder).unwrap_or_default()),
        }
    }
}

// ============================================================================
// Index Documents
// ============================================================================

/// Entry in a folder children index
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FolderChildEntry {
    /// Child ID
    pub id: Uuid,
    /// Child type
    pub kind: String, // "file" or "folder"
    /// Child name
    pub name: String,
    /// Deleted flag
    pub deleted: bool,
    /// Size (for files)
    pub size: Option<i64>,
    /// MIME type (for files)
    pub mime: Option<String>,
    /// Last updated
    pub updated_at: DateTime<Utc>,
}

/// Folder children index (rebuildable projection)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FolderChildrenIndex {
    /// Schema version
    pub schema_version: u32,
    /// Folder ID
    pub folder_id: Uuid,
    /// Index version (incremented on each update)
    pub version: u64,
    /// Last updated
    pub updated_at: DateTime<Utc>,
    /// Children entries
    pub children: Vec<FolderChildEntry>,
}

impl FolderChildrenIndex {
    /// Create a new empty index
    pub fn new(folder_id: Uuid) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            folder_id,
            version: 1,
            updated_at: Utc::now(),
            children: Vec::new(),
        }
    }
    
    /// Add or update a child
    pub fn upsert_child(&mut self, entry: FolderChildEntry) {
        if let Some(existing) = self.children.iter_mut().find(|c| c.id == entry.id) {
            *existing = entry;
        } else {
            self.children.push(entry);
        }
        self.children.sort_by(|a, b| a.name.cmp(&b.name));
        self.version += 1;
        self.updated_at = Utc::now();
    }
    
    /// Remove a child
    pub fn remove_child(&mut self, child_id: Uuid) {
        self.children.retain(|c| c.id != child_id);
        self.version += 1;
        self.updated_at = Utc::now();
    }
    
    /// Mark a child as deleted
    pub fn mark_deleted(&mut self, child_id: Uuid) {
        if let Some(child) = self.children.iter_mut().find(|c| c.id == child_id) {
            child.deleted = true;
            self.version += 1;
            self.updated_at = Utc::now();
        }
    }
}

/// User roots index
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserRootsIndex {
    /// Schema version
    pub schema_version: u32,
    /// User ID
    pub user_id: Uuid,
    /// Index version
    pub version: u64,
    /// Last updated
    pub updated_at: DateTime<Utc>,
    /// Root folder IDs
    pub root_folder_ids: Vec<Uuid>,
}

/// Shared with me index
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedWithMeIndex {
    /// Schema version
    pub schema_version: u32,
    /// User ID
    pub user_id: Uuid,
    /// Index version
    pub version: u64,
    /// Last updated
    pub updated_at: DateTime<Utc>,
    /// Share entries
    pub shares: Vec<ShareEntry>,
}

/// Entry in shared with me index
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShareEntry {
    /// Share ID
    pub share_id: Uuid,
    /// Resource type
    pub resource_type: String,
    /// Resource ID
    pub resource_id: Uuid,
    /// Resource name
    pub resource_name: String,
    /// Permissions
    pub permissions: SharePermission,
    /// Shared by
    pub shared_by: Uuid,
    /// Shared at
    pub shared_at: DateTime<Utc>,
}

// ============================================================================
// Schema Migration Support
// ============================================================================

/// Trait for schema migrations
pub trait SchemaMigration<T> {
    /// Target schema version
    const TARGET_VERSION: u32;
    
    /// Migrate from a previous version
    fn migrate(value: serde_json::Value) -> Result<T, String>;
}

/// Helper to handle schema version mismatches
pub fn ensure_current_version<T>(doc: &mut T, current_version: u32) 
where
    T: Serialize + DeserializeOwned,
{
    // This is a placeholder for future migration logic
    // For now, we just ensure the schema_version field is set
    if current_version != CURRENT_SCHEMA_VERSION {
        // In the future, implement actual migration here
        tracing::warn!(
            "Schema version mismatch: {} vs current {}",
            current_version,
            CURRENT_SCHEMA_VERSION
        );
    }
}

// ============================================================================
// User Document
// ============================================================================

/// User account document (canonical)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserDocument {
    /// Schema version for migration support
    pub schema_version: u32,
    /// Unique user identifier
    pub id: Uuid,
    /// Username (unique, immutable)
    pub username: String,
    /// Display name (mutable)
    pub display_name: String,
    /// Email address (unique)
    pub email: String,
    /// Argon2 password hash
    pub password_hash: String,
    /// Whether user is administrator
    pub is_admin: bool,
    /// Whether account is disabled
    pub disabled: bool,
    /// When account was disabled
    pub disabled_at: Option<DateTime<Utc>>,
    /// Reason for disable
    pub disabled_reason: Option<String>,
    /// Storage quota in bytes
    pub storage_quota_bytes: i64,
    /// Theme preference
    pub theme: String, // "light", "dark", "system"
    /// When email was verified
    pub email_verified_at: Option<DateTime<Utc>>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
    /// Document version for optimistic concurrency
    pub version: u64,
}

impl UserDocument {
    /// Create a new user document
    pub fn new(
        id: Uuid,
        username: String,
        display_name: String,
        email: String,
        password_hash: String,
        is_admin: bool,
        storage_quota_bytes: i64,
    ) -> Self {
        let now = Utc::now();
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            id,
            username,
            display_name,
            email,
            password_hash,
            is_admin,
            disabled: false,
            disabled_at: None,
            disabled_reason: None,
            storage_quota_bytes,
            theme: "system".to_string(),
            email_verified_at: None,
            created_at: now,
            updated_at: now,
            version: 1,
        }
    }
    
    /// Bump version on mutation
    pub fn bump_version(&mut self) {
        self.version += 1;
        self.updated_at = Utc::now();
    }
    
    /// Disable the account
    pub fn disable(&mut self, reason: Option<String>) {
        self.disabled = true;
        self.disabled_at = Some(Utc::now());
        self.disabled_reason = reason;
        self.bump_version();
    }
    
    /// Enable the account
    pub fn enable(&mut self) {
        self.disabled = false;
        self.disabled_at = None;
        self.disabled_reason = None;
        self.bump_version();
    }
    
    /// Check if user can authenticate
    pub fn can_authenticate(&self) -> bool {
        !self.disabled
    }
}

// ============================================================================
// Notification Types and Document
// ============================================================================

/// Notification types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationType {
    FileShared,
    FolderShared,
    FileModified,
    ShareRevoked,
    AccessRequested,
    AccessGranted,
}

/// Notification document (canonical, but derived from events)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotificationDocument {
    /// Schema version
    pub schema_version: u32,
    /// Unique notification identifier
    pub id: Uuid,
    /// Recipient user ID
    pub user_id: Uuid,
    /// Source event ID
    pub event_id: Uuid,
    /// Resource type
    pub resource_type: String,
    /// Resource ID
    pub resource_id: Uuid,
    /// Notification type
    pub notification_type: NotificationType,
    /// Notification title
    pub title: String,
    /// Notification message
    pub message: String,
    /// Whether notification has been read
    pub read: bool,
    /// When notification was read
    pub read_at: Option<DateTime<Utc>>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
}

impl NotificationDocument {
    /// Create a new notification
    pub fn new(
        id: Uuid,
        user_id: Uuid,
        event_id: Uuid,
        resource_type: String,
        resource_id: Uuid,
        notification_type: NotificationType,
        title: String,
        message: String,
    ) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            id,
            user_id,
            event_id,
            resource_type,
            resource_id,
            notification_type,
            title,
            message,
            read: false,
            read_at: None,
            created_at: Utc::now(),
        }
    }
    
    /// Mark as read
    pub fn mark_read(&mut self) {
        self.read = true;
        self.read_at = Some(Utc::now());
    }
}

/// Reference to a notification in an index
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotificationRef {
    pub notification_id: Uuid,
    pub notification_type: NotificationType,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub read: bool,
    pub created_at: DateTime<Utc>,
}

/// User notification index (rebuildable projection)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserNotificationIndex {
    /// Schema version
    pub schema_version: u32,
    /// User ID
    pub user_id: Uuid,
    /// Index version
    pub version: u64,
    /// Last updated
    pub updated_at: DateTime<Utc>,
    /// Notification references (sorted by created_at desc)
    pub notifications: Vec<NotificationRef>,
    /// Unread count (denormalized for efficiency)
    pub unread_count: u32,
}

impl UserNotificationIndex {
    /// Create a new empty index
    pub fn new(user_id: Uuid) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            user_id,
            version: 1,
            updated_at: Utc::now(),
            notifications: Vec::new(),
            unread_count: 0,
        }
    }
    
    /// Add a notification
    pub fn add_notification(&mut self, notif: &NotificationRef) {
        self.notifications.push(notif.clone());
        // Sort by created_at desc
        self.notifications.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        if !notif.read {
            self.unread_count += 1;
        }
        self.version += 1;
        self.updated_at = Utc::now();
    }
    
    /// Mark a notification as read
    pub fn mark_read(&mut self, notification_id: Uuid) {
        if let Some(notif) = self.notifications.iter_mut().find(|n| n.notification_id == notification_id) {
            if !notif.read {
                notif.read = true;
                self.unread_count = self.unread_count.saturating_sub(1);
                self.version += 1;
                self.updated_at = Utc::now();
            }
        }
    }
    
    /// Remove a notification
    pub fn remove_notification(&mut self, notification_id: Uuid) {
        if let Some(pos) = self.notifications.iter().position(|n| n.notification_id == notification_id) {
            let notif = self.notifications.remove(pos);
            if !notif.read {
                self.unread_count = self.unread_count.saturating_sub(1);
            }
            self.version += 1;
            self.updated_at = Utc::now();
        }
    }
}

// ============================================================================
// Job Types and Document
// ============================================================================

/// Job types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    Replication,
    ThumbnailGeneration,
    VirusScan,
    MetadataExtraction,
}

/// Job status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Job result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobResult {
    pub success: bool,
    pub message: Option<String>,
    pub output: Option<serde_json::Value>,
}

/// Job document (canonical)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobDocument {
    /// Schema version
    pub schema_version: u32,
    /// Unique job identifier
    pub id: Uuid,
    /// Job type
    pub job_type: JobType,
    /// Resource type being processed
    pub resource_type: String,
    /// Resource ID being processed
    pub resource_id: Uuid,
    /// Job status
    pub status: JobStatus,
    /// Priority (higher = more important)
    pub priority: i32,
    /// Job-specific payload
    pub payload: serde_json::Value,
    /// Job result (if completed)
    pub result: Option<JobResult>,
    /// Retry count
    pub retry_count: u32,
    /// Maximum retries
    pub max_retries: u32,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Scheduled execution time
    pub scheduled_at: DateTime<Utc>,
    /// When job started
    pub started_at: Option<DateTime<Utc>>,
    /// When job completed
    pub completed_at: Option<DateTime<Utc>>,
    /// Error message (if failed)
    pub error_message: Option<String>,
    /// Worker ID that claimed the job
    pub worker_id: Option<String>,
    /// Document version
    pub version: u64,
}

impl JobDocument {
    /// Create a new pending job
    pub fn new(
        id: Uuid,
        job_type: JobType,
        resource_type: String,
        resource_id: Uuid,
        priority: i32,
        payload: serde_json::Value,
        max_retries: u32,
    ) -> Self {
        let now = Utc::now();
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            id,
            job_type,
            resource_type,
            resource_id,
            status: JobStatus::Pending,
            priority,
            payload,
            result: None,
            retry_count: 0,
            max_retries,
            created_at: now,
            scheduled_at: now,
            started_at: None,
            completed_at: None,
            error_message: None,
            worker_id: None,
            version: 1,
        }
    }
    
    /// Mark job as running
    pub fn mark_running(&mut self, worker_id: String) {
        self.status = JobStatus::Running;
        self.worker_id = Some(worker_id);
        self.started_at = Some(Utc::now());
        self.version += 1;
    }
    
    /// Mark job as completed
    pub fn mark_completed(&mut self, result: JobResult) {
        self.status = JobStatus::Completed;
        self.result = Some(result);
        self.completed_at = Some(Utc::now());
        self.version += 1;
    }
    
    /// Mark job as failed
    pub fn mark_failed(&mut self, error_message: String) {
        self.status = JobStatus::Failed;
        self.error_message = Some(error_message);
        self.completed_at = Some(Utc::now());
        self.retry_count += 1;
        self.version += 1;
        
        // Reset to pending if retries remain
        if self.retry_count < self.max_retries {
            self.status = JobStatus::Pending;
            self.scheduled_at = Utc::now() + chrono::Duration::seconds(60 * (self.retry_count as i64));
            self.worker_id = None;
            self.started_at = None;
            self.completed_at = None;
        }
    }
    
    /// Check if job can be retried
    pub fn can_retry(&self) -> bool {
        self.status == JobStatus::Failed && self.retry_count < self.max_retries
    }
    
    /// Check if job is in terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self.status, JobStatus::Completed | JobStatus::Cancelled)
            || (matches!(self.status, JobStatus::Failed) && !self.can_retry())
    }
}

/// Reference to a job in a queue
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobRef {
    pub job_id: Uuid,
    pub job_type: JobType,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub priority: i32,
    pub created_at: DateTime<Utc>,
}

/// Job queue index (rebuildable projection)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobQueueIndex {
    /// Schema version
    pub schema_version: u32,
    /// Namespace
    pub namespace: String,
    /// Index version
    pub version: u64,
    /// Last updated
    pub updated_at: DateTime<Utc>,
    /// Pending jobs (sorted by priority desc, created_at asc)
    pub pending: Vec<JobRef>,
    /// Running jobs
    pub running: Vec<JobRef>,
    /// Recently completed (last 100)
    pub completed_recent: Vec<JobRef>,
}

impl JobQueueIndex {
    /// Create a new empty queue index
    pub fn new(namespace: String) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            namespace,
            version: 1,
            updated_at: Utc::now(),
            pending: Vec::new(),
            running: Vec::new(),
            completed_recent: Vec::new(),
        }
    }
    
    /// Add a job to pending
    pub fn add_pending(&mut self, job_ref: JobRef) {
        self.pending.push(job_ref);
        self.sort_pending();
        self.version += 1;
        self.updated_at = Utc::now();
    }
    
    /// Move job to running
    pub fn mark_running(&mut self, job_id: Uuid) {
        if let Some(pos) = self.pending.iter().position(|j| j.job_id == job_id) {
            let job = self.pending.remove(pos);
            self.running.push(job);
            self.version += 1;
            self.updated_at = Utc::now();
        }
    }
    
    /// Move job to completed
    pub fn mark_completed(&mut self, job_id: Uuid) {
        if let Some(pos) = self.running.iter().position(|j| j.job_id == job_id) {
            let job = self.running.remove(pos);
            self.completed_recent.insert(0, job);
            // Keep only last 100
            if self.completed_recent.len() > 100 {
                self.completed_recent.truncate(100);
            }
            self.version += 1;
            self.updated_at = Utc::now();
        }
    }
    
    /// Remove a job (e.g., cancelled)
    pub fn remove_job(&mut self, job_id: Uuid) {
        self.pending.retain(|j| j.job_id != job_id);
        self.running.retain(|j| j.job_id != job_id);
        self.version += 1;
        self.updated_at = Utc::now();
    }
    
    fn sort_pending(&mut self) {
        // Sort by priority desc, then created_at asc
        self.pending.sort_by(|a, b| {
            b.priority.cmp(&a.priority)
                .then_with(|| a.created_at.cmp(&b.created_at))
        });
    }
}

// ============================================================================
// Device Token Document
// ============================================================================

/// Device token document (canonical)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceTokenDocument {
    /// Schema version
    pub schema_version: u32,
    /// Unique token identifier
    pub id: Uuid,
    /// User ID
    pub user_id: Uuid,
    /// Token hash (for lookup)
    pub token_hash: String,
    /// Device name
    pub device_name: String,
    /// Device type
    pub device_type: String, // "ios", "android", "desktop", "web"
    /// Last used timestamp
    pub last_used_at: DateTime<Utc>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Expiration timestamp
    pub expires_at: Option<DateTime<Utc>>,
    /// Revocation timestamp
    pub revoked_at: Option<DateTime<Utc>>,
}

impl DeviceTokenDocument {
    /// Create a new device token
    pub fn new(
        id: Uuid,
        user_id: Uuid,
        token_hash: String,
        device_name: String,
        device_type: String,
        expires_at: Option<DateTime<Utc>>,
    ) -> Self {
        let now = Utc::now();
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            id,
            user_id,
            token_hash,
            device_name,
            device_type,
            last_used_at: now,
            created_at: now,
            expires_at,
            revoked_at: None,
        }
    }
    
    /// Check if token is valid
    pub fn is_valid(&self) -> bool {
        if self.revoked_at.is_some() {
            return false;
        }
        if let Some(expires) = self.expires_at {
            return Utc::now() < expires;
        }
        true
    }
    
    /// Revoke the token
    pub fn revoke(&mut self) {
        self.revoked_at = Some(Utc::now());
    }
    
    /// Update last used
    pub fn touch(&mut self) {
        self.last_used_at = Utc::now();
    }
}

// ============================================================================
// User Group Document
// ============================================================================

/// User group document (canonical)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserGroupDocument {
    /// Schema version
    pub schema_version: u32,
    /// Unique group identifier
    pub id: Uuid,
    /// Group name
    pub name: String,
    /// Group description
    pub description: String,
    /// Creator user ID
    pub created_by: Uuid,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
    /// Member user IDs
    pub member_ids: Vec<Uuid>,
    /// Document version
    pub version: u64,
}

impl UserGroupDocument {
    /// Create a new user group
    pub fn new(id: Uuid, name: String, description: String, created_by: Uuid) -> Self {
        let now = Utc::now();
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            id,
            name,
            description,
            created_by,
            created_at: now,
            updated_at: now,
            member_ids: Vec::new(),
            version: 1,
        }
    }
    
    /// Add a member
    pub fn add_member(&mut self, user_id: Uuid) {
        if !self.member_ids.contains(&user_id) {
            self.member_ids.push(user_id);
            self.bump_version();
        }
    }
    
    /// Remove a member
    pub fn remove_member(&mut self, user_id: Uuid) {
        if let Some(pos) = self.member_ids.iter().position(|&id| id == user_id) {
            self.member_ids.remove(pos);
            self.bump_version();
        }
    }
    
    /// Check if user is member
    pub fn is_member(&self, user_id: Uuid) -> bool {
        self.member_ids.contains(&user_id)
    }
    
    fn bump_version(&mut self) {
        self.version += 1;
        self.updated_at = Utc::now();
    }
}

// ============================================================================
// System Configuration Documents
// ============================================================================

/// Configuration types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigType {
    Oidc,
    Smtp,
    Webhooks,
    Server,
    Security,
}

/// System configuration document (canonical)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemConfigDocument {
    /// Schema version
    pub schema_version: u32,
    /// Configuration type
    pub config_type: ConfigType,
    /// Configuration payload
    pub config: serde_json::Value,
    /// Last updated
    pub updated_at: DateTime<Utc>,
    /// Updated by
    pub updated_by: Option<Uuid>,
    /// Document version
    pub version: u64,
}

impl SystemConfigDocument {
    /// Create a new config document
    pub fn new(config_type: ConfigType, config: serde_json::Value, updated_by: Option<Uuid>) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            config_type,
            config,
            updated_at: Utc::now(),
            updated_by,
            version: 1,
        }
    }
    
    /// Update configuration
    pub fn update(&mut self, config: serde_json::Value, updated_by: Option<Uuid>) {
        self.config = config;
        self.updated_at = Utc::now();
        self.updated_by = updated_by;
        self.version += 1;
    }
}

// ============================================================================
// Replication Target Document
// ============================================================================

/// Replication target types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplicationTargetType {
    S3,
    S3Compatible,
    AzureBlob,
    Gcs,
}

/// Replication target document (canonical)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplicationTargetDocument {
    /// Schema version
    pub schema_version: u32,
    /// Unique target identifier
    pub id: Uuid,
    /// Target name
    pub name: String,
    /// Target type
    pub target_type: ReplicationTargetType,
    /// Endpoint URL (for S3-compatible)
    pub endpoint: String,
    /// Region
    pub region: String,
    /// Bucket/container name
    pub bucket: String,
    /// Path prefix
    pub path_prefix: String,
    /// Access key ID (encrypted)
    pub access_key_id: String,
    /// Secret access key (encrypted)
    pub secret_access_key: String,
    /// Whether target is enabled
    pub enabled: bool,
    /// Priority (lower = higher priority)
    pub priority: i32,
    /// Creator user ID
    pub created_by: Uuid,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
    /// Document version
    pub version: u64,
}

impl ReplicationTargetDocument {
    /// Create a new replication target
    pub fn new(
        id: Uuid,
        name: String,
        target_type: ReplicationTargetType,
        endpoint: String,
        region: String,
        bucket: String,
        created_by: Uuid,
    ) -> Self {
        let now = Utc::now();
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            id,
            name,
            target_type,
            endpoint,
            region,
            bucket,
            path_prefix: String::new(),
            access_key_id: String::new(),
            secret_access_key: String::new(),
            enabled: true,
            priority: 0,
            created_by,
            created_at: now,
            updated_at: now,
            version: 1,
        }
    }
    
    fn bump_version(&mut self) {
        self.version += 1;
        self.updated_at = Utc::now();
    }
}

// ============================================================================
// Thumbnail Document
// ============================================================================

/// Thumbnail metadata document (derived)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThumbnailDocument {
    /// Schema version
    pub schema_version: u32,
    /// Source file ID
    pub file_id: Uuid,
    /// Thumbnail blob key
    pub thumbnail_key: String,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Format
    pub format: String, // "webp", "jpeg", "png"
    /// Size in bytes
    pub size_bytes: u64,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
}

impl ThumbnailDocument {
    /// Create a new thumbnail document
    pub fn new(
        file_id: Uuid,
        thumbnail_key: String,
        width: u32,
        height: u32,
        format: String,
        size_bytes: u64,
    ) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            file_id,
            thumbnail_key,
            width,
            height,
            format,
            size_bytes,
            created_at: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_folder_document_creation() {
        let folder = FolderDocument::new_root(Uuid::new_v4(), Uuid::new_v4());
        assert_eq!(folder.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(folder.name, "Root");
        assert_eq!(folder.path, "/");
        assert!(!folder.deleted);
    }
    
    #[test]
    fn test_file_document_version_update() {
        let mut file = FileDocument::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            None,
            "test.txt".to_string(),
            "/test.txt".to_string(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            100,
            "text/plain".to_string(),
            "abc123".to_string(),
        );
        
        let old_version = file.version;
        file.update_version(Uuid::new_v4(), 200, "def456".to_string(), None);
        
        assert_eq!(file.version_number, 2);
        assert_eq!(file.size, 200);
        assert!(file.version > old_version);
    }
    
    #[test]
    fn test_share_active_status() {
        let share = ShareDocument::new_public(
            Uuid::new_v4(),
            "file".to_string(),
            Uuid::new_v4(),
            SharePermission::View,
            "token_hash".to_string(),
            None,
            None,
            Uuid::new_v4(),
        );
        
        assert!(share.is_active());
        
        let mut expired = share.clone();
        expired.expires_at = Some(Utc::now() - chrono::Duration::hours(1));
        assert!(!expired.is_active());
        
        let mut revoked = share;
        revoked.revoke();
        assert!(!revoked.is_active());
    }
    
    #[test]
    fn test_folder_children_index() {
        let mut index = FolderChildrenIndex::new(Uuid::new_v4());
        let child = FolderChildEntry {
            id: Uuid::new_v4(),
            kind: "file".to_string(),
            name: "test.txt".to_string(),
            deleted: false,
            size: Some(100),
            mime: Some("text/plain".to_string()),
            updated_at: Utc::now(),
        };
        
        index.upsert_child(child.clone());
        assert_eq!(index.children.len(), 1);
        
        // Update same child
        let mut updated = child.clone();
        updated.size = Some(200);
        index.upsert_child(updated);
        assert_eq!(index.children.len(), 1);
        assert_eq!(index.children[0].size, Some(200));
    }

    #[test]
    fn test_user_document() {
        let user = UserDocument::new(
            Uuid::new_v4(),
            "johndoe".to_string(),
            "John Doe".to_string(),
            "john@example.com".to_string(),
            "argon2hash".to_string(),
            false,
            10_737_418_240,
        );
        
        assert_eq!(user.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(user.username, "johndoe");
        assert!(user.can_authenticate());
        
        let mut disabled_user = user.clone();
        disabled_user.disable(Some("Test disable".to_string()));
        assert!(!disabled_user.can_authenticate());
        assert!(disabled_user.disabled_at.is_some());
        
        disabled_user.enable();
        assert!(disabled_user.can_authenticate());
        assert!(!disabled_user.disabled);
    }

    #[test]
    fn test_notification_index() {
        let user_id = Uuid::new_v4();
        let mut index = UserNotificationIndex::new(user_id);
        
        let notif_ref = NotificationRef {
            notification_id: Uuid::new_v4(),
            notification_type: NotificationType::FileShared,
            resource_type: "file".to_string(),
            resource_id: Uuid::new_v4(),
            read: false,
            created_at: Utc::now(),
        };
        
        index.add_notification(&notif_ref);
        assert_eq!(index.notifications.len(), 1);
        assert_eq!(index.unread_count, 1);
        
        index.mark_read(notif_ref.notification_id);
        assert_eq!(index.unread_count, 0);
        assert!(index.notifications[0].read);
    }

    #[test]
    fn test_job_document_lifecycle() {
        let mut job = JobDocument::new(
            Uuid::new_v4(),
            JobType::Replication,
            "file_version".to_string(),
            Uuid::new_v4(),
            10,
            serde_json::json!({"target_id": Uuid::new_v4()}),
            3,
        );
        
        assert_eq!(job.status, JobStatus::Pending);
        assert!(!job.is_terminal());
        
        job.mark_running("worker1".to_string());
        assert_eq!(job.status, JobStatus::Running);
        assert!(job.worker_id.is_some());
        
        job.mark_completed(JobResult {
            success: true,
            message: Some("Done".to_string()),
            output: None,
        });
        assert_eq!(job.status, JobStatus::Completed);
        assert!(job.is_terminal());
    }

    #[test]
    fn test_job_retry_logic() {
        let mut job = JobDocument::new(
            Uuid::new_v4(),
            JobType::ThumbnailGeneration,
            "file".to_string(),
            Uuid::new_v4(),
            5,
            serde_json::Value::Null,
            3,
        );
        
        job.mark_running("worker1".to_string());
        job.mark_failed("Network error".to_string());
        
        // Should be pending again with retry
        assert_eq!(job.status, JobStatus::Pending);
        assert_eq!(job.retry_count, 1);
        assert!(job.can_retry());
        
        // Exhaust retries
        job.mark_running("worker1".to_string());
        job.mark_failed("Error 2".to_string());
        job.mark_running("worker1".to_string());
        job.mark_failed("Error 3".to_string());
        job.mark_running("worker1".to_string());
        job.mark_failed("Final error".to_string());
        
        assert_eq!(job.status, JobStatus::Failed);
        assert!(!job.can_retry());
        assert!(job.is_terminal());
    }

    #[test]
    fn test_job_queue_index() {
        let mut index = JobQueueIndex::new("default".to_string());
        
        let job1 = JobRef {
            job_id: Uuid::new_v4(),
            job_type: JobType::Replication,
            resource_type: "file".to_string(),
            resource_id: Uuid::new_v4(),
            priority: 10,
            created_at: Utc::now(),
        };
        
        let job2 = JobRef {
            job_id: Uuid::new_v4(),
            job_type: JobType::ThumbnailGeneration,
            resource_type: "file".to_string(),
            resource_id: Uuid::new_v4(),
            priority: 5,
            created_at: Utc::now(),
        };
        
        index.add_pending(job2);
        index.add_pending(job1);
        
        // Higher priority should be first
        assert_eq!(index.pending.len(), 2);
        assert_eq!(index.pending[0].priority, 10);
        
        index.mark_running(job1.job_id);
        assert_eq!(index.pending.len(), 1);
        assert_eq!(index.running.len(), 1);
    }

    #[test]
    fn test_device_token() {
        let mut token = DeviceTokenDocument::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "hash123".to_string(),
            "My iPhone".to_string(),
            "ios".to_string(),
            None,
        );
        
        assert!(token.is_valid());
        
        token.revoke();
        assert!(!token.is_valid());
    }

    #[test]
    fn test_user_group() {
        let mut group = UserGroupDocument::new(
            Uuid::new_v4(),
            "Admins".to_string(),
            "Administrators".to_string(),
            Uuid::new_v4(),
        );
        
        let user1 = Uuid::new_v4();
        let user2 = Uuid::new_v4();
        
        group.add_member(user1);
        group.add_member(user2);
        assert_eq!(group.member_ids.len(), 2);
        assert!(group.is_member(user1));
        
        group.remove_member(user1);
        assert_eq!(group.member_ids.len(), 1);
        assert!(!group.is_member(user1));
    }

    #[test]
    fn test_system_config() {
        let mut config = SystemConfigDocument::new(
            ConfigType::Oidc,
            serde_json::json!({"provider": "google"}),
            Some(Uuid::new_v4()),
        );
        
        assert_eq!(config.config_type, ConfigType::Oidc);
        
        config.update(serde_json::json!({"provider": "github"}), None);
        assert_eq!(config.config["provider"], "github");
        assert_eq!(config.version, 2);
    }
}
