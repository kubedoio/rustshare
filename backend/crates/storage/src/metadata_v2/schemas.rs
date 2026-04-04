//! Versioned metadata document schemas
//!
//! All schemas include a `schema_version` field for forward/backward compatibility.
//! When reading, old versions are migrated to the current version.
//! When writing, always use the current version.

use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tracing::warn;
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
    /// Tenant ID
    pub tenant_id: Uuid,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
    /// Document version for optimistic concurrency
    pub version: u64,
    /// Soft delete flag
    pub deleted: bool,
    /// Ancestor folder IDs (parent, grandparent, etc. - root first)
    /// Used for efficient permission resolution without tree walking
    pub ancestor_ids: Vec<Uuid>,
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
        tenant_id: Uuid,
        ancestor_ids: Vec<Uuid>,
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
            tenant_id,
            created_at: now,
            updated_at: now,
            version: 1,
            deleted: false,
            ancestor_ids,
        }
    }
    
    /// Create a root folder for a user
    pub fn new_root(namespace_id: Uuid, owner_id: Uuid, tenant_id: Uuid) -> Self {
        let id = Uuid::new_v4();
        Self::new(
            id,
            namespace_id,
            None,
            "Root".to_string(),
            "/".to_string(),
            owner_id,
            tenant_id,
            Vec::new(), // Root has no ancestors
        )
    }
    
    /// Create a child folder
    pub fn new_child(
        namespace_id: Uuid,
        parent_id: Uuid,
        parent_path: &str,
        name: String,
        owner_id: Uuid,
        tenant_id: Uuid,
        parent_ancestor_ids: Vec<Uuid>,
    ) -> Self {
        let id = Uuid::new_v4();
        let path = if parent_path == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", parent_path, name)
        };
        let mut ancestor_ids = parent_ancestor_ids;
        ancestor_ids.push(parent_id);
        Self::new(id, namespace_id, Some(parent_id), name, path, owner_id, tenant_id, ancestor_ids)
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
    /// Tenant ID
    pub tenant_id: Uuid,
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
        tenant_id: Uuid,
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
            tenant_id,
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
    
    /// Rename file (updates path).
    ///
    /// Both new_name and new_path accept `&str` or `String` to avoid
    /// unnecessary allocations when the caller already owns the strings.
    pub fn rename(&mut self, new_name: impl Into<String>, new_path: impl Into<String>) {
        self.name = new_name.into();
        self.path = new_path.into();
        self.bump_version();
    }
    
    /// Move file to new parent.
    ///
    /// The new_path accepts `&str` or `String` to avoid unnecessary allocations.
    pub fn move_to(&mut self, new_parent_id: Option<Uuid>, new_path: impl Into<String>) {
        self.parent_id = new_parent_id;
        self.path = new_path.into();
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
    /// Tenant ID
    pub tenant_id: Uuid,
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
        tenant_id: Uuid,
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
            tenant_id,
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
    /// Tenant ID
    pub tenant_id: Uuid,
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
        tenant_id: Uuid,
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
            tenant_id,
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
        tenant_id: Uuid,
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
            tenant_id,
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
    /// Tenant ID
    pub tenant_id: Uuid,
}

impl EventDocument {
    /// Create a new event
    pub fn new(
        event_type: EventType,
        actor_id: Uuid,
        resource_type: String,
        resource_id: Uuid,
        payload: serde_json::Value,
        tenant_id: Uuid,
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
            tenant_id,
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
    /// Tenant ID
    pub tenant_id: Uuid,
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
            tenant_id: file.tenant_id,
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
            tenant_id: folder.tenant_id,
        }
    }
}

// ============================================================================
// Sync Cursor Document
// ============================================================================

/// Sync cursor document for device synchronization checkpoint
/// 
/// Each device maintains its own cursor that tracks the last event
/// the device has successfully processed. This enables reliable
/// incremental sync for desktop clients.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncCursorDocument {
    /// Schema version
    pub schema_version: u32,
    /// User ID (owner of this cursor)
    pub user_id: Uuid,
    /// Device ID (unique per device)
    pub device_id: Uuid,
    /// Opaque cursor token (base64 encoded timestamp+nonce)
    pub cursor: String,
    /// Last event ID processed by this device
    pub last_event_id: Uuid,
    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,
    /// Device info (optional, for display/management)
    pub device_info: Option<String>,
}

impl SyncCursorDocument {
    /// Create a new sync cursor document
    pub fn new(
        user_id: Uuid,
        device_id: Uuid,
        cursor: String,
        last_event_id: Uuid,
        device_info: Option<String>,
    ) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            user_id,
            device_id,
            cursor,
            last_event_id,
            updated_at: Utc::now(),
            device_info,
        }
    }
    
    /// Update the cursor and last event ID.
    ///
    /// The cursor can be any type that converts into a String,
    /// such as `&str` or `String`.
    pub fn update(&mut self, cursor: impl Into<String>, last_event_id: Uuid) {
        self.cursor = cursor.into();
        self.last_event_id = last_event_id;
        self.updated_at = Utc::now();
    }
    
    /// Parse cursor token to extract timestamp
    /// 
    /// Cursor format: base64(timestamp_millis + ":" + nonce)
    pub fn parse_cursor_timestamp(&self) -> Option<chrono::DateTime<Utc>> {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        
        let decoded = STANDARD.decode(&self.cursor).ok()?;
        let decoded_str = String::from_utf8(decoded).ok()?;
        let parts: Vec<&str> = decoded_str.split(':').collect();
        
        if parts.len() != 2 {
            return None;
        }
        
        let timestamp_millis: i64 = parts[0].parse().ok()?;
        chrono::DateTime::from_timestamp_millis(timestamp_millis)
    }
    
    /// Generate a new cursor token from a timestamp
    /// 
    /// Cursor format: base64(timestamp_millis + ":" + uuid_v4)
    pub fn generate_cursor(timestamp: DateTime<Utc>) -> String {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        
        let timestamp_millis = timestamp.timestamp_millis();
        let nonce = Uuid::new_v4();
        let token = format!("{}:{}", timestamp_millis, nonce);
        STANDARD.encode(token)
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
pub fn ensure_current_version<T>(_doc: &mut T, current_version: u32) 
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
    /// Tenant ID
    pub tenant_id: Uuid,
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
        tenant_id: Uuid,
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
            tenant_id,
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
    /// Tenant ID
    pub tenant_id: Uuid,
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
        tenant_id: Uuid,
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
            tenant_id,
            created_at: Utc::now(),
        }
    }
    
    /// Mark as read
    pub fn mark_read(&mut self) {
        self.read = true;
        self.read_at = Some(Utc::now());
    }
}

/// Notification reference (for indexes)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotificationRef {
    /// Notification ID
    pub notification_id: Uuid,
    /// Notification type
    pub notification_type: NotificationType,
    /// Resource type
    pub resource_type: String,
    /// Resource ID
    pub resource_id: Uuid,
    /// Read status
    pub read: bool,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
}

/// User notification index
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
    /// Notifications (sorted by created_at desc)
    pub notifications: Vec<NotificationRef>,
    /// Unread count
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
    
    /// Add a notification reference
    pub fn add_notification(&mut self, notification: &NotificationRef) {
        self.notifications.push(notification.clone());
        if !notification.read {
            self.unread_count += 1;
        }
        // Sort by created_at descending
        self.notifications.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        self.version += 1;
        self.updated_at = Utc::now();
    }
    
    /// Mark notification as read
    pub fn mark_read(&mut self, notification_id: Uuid) {
        if let Some(notif) = self.notifications.iter_mut().find(|n| n.notification_id == notification_id) {
            if !notif.read {
                notif.read = true;
                self.unread_count = (self.unread_count - 1).max(0);
                self.version += 1;
                self.updated_at = Utc::now();
            }
        }
    }
    
    /// Remove a notification
    pub fn remove_notification(&mut self, notification_id: Uuid) {
        if let Some(pos) = self.notifications.iter().position(|n| n.notification_id == notification_id) {
            let notif = &self.notifications[pos];
            if !notif.read {
                self.unread_count = (self.unread_count - 1).max(0);
            }
            self.notifications.remove(pos);
            self.version += 1;
            self.updated_at = Utc::now();
        }
    }
}

// ============================================================================
// Job Types and Document
// ============================================================================

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

/// Job priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobPriority {
    Low,
    Normal,
    High,
    Critical,
}

impl Default for JobPriority {
    fn default() -> Self {
        Self::Normal
    }
}

/// Job reference (for queue index)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobRef {
    /// Job ID
    pub job_id: Uuid,
    /// Job type
    pub job_type: JobType,
    /// Resource type
    pub resource_type: String,
    /// Resource ID
    pub resource_id: Uuid,
    /// Priority (higher = more important)
    pub priority: i32,
    /// Created at
    pub created_at: DateTime<Utc>,
}

/// Job queue index
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobQueueIndex {
    /// Schema version
    pub schema_version: u32,
    /// Index version
    pub version: u64,
    /// Last updated
    pub updated_at: DateTime<Utc>,
    /// Pending jobs (sorted by priority desc, created_at asc)
    pub pending: Vec<JobRef>,
    /// Running jobs
    pub running: Vec<JobRef>,
    /// Recently completed jobs (for tracking)
    pub completed_recent: Vec<JobRef>,
}

impl JobQueueIndex {
    /// Create a new empty index
    pub fn new(_namespace: String) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            version: 1,
            updated_at: Utc::now(),
            pending: Vec::new(),
            running: Vec::new(),
            completed_recent: Vec::new(),
        }
    }
    
    /// Add a pending job
    pub fn add_pending(&mut self, job: JobRef) {
        self.pending.push(job);
        self.sort_pending();
        self.version += 1;
        self.updated_at = Utc::now();
    }
    
    /// Mark job as running
    pub fn mark_running(&mut self, job_id: Uuid) {
        if let Some(pos) = self.pending.iter().position(|j| j.job_id == job_id) {
            let job = self.pending.remove(pos);
            self.running.push(job);
            self.version += 1;
            self.updated_at = Utc::now();
        }
    }
    
    /// Mark job as completed/failed/cancelled
    pub fn mark_completed(&mut self, job_id: Uuid) {
        if let Some(pos) = self.running.iter().position(|j| j.job_id == job_id) {
            let job = self.running.remove(pos);
            self.completed_recent.push(job);
            // Keep only recent 100 completed jobs
            if self.completed_recent.len() > 100 {
                self.completed_recent.remove(0);
            }
            self.version += 1;
            self.updated_at = Utc::now();
        }
    }
    
    /// Remove a job from any queue
    pub fn remove_job(&mut self, job_id: Uuid) {
        self.pending.retain(|j| j.job_id != job_id);
        self.running.retain(|j| j.job_id != job_id);
        self.completed_recent.retain(|j| j.job_id != job_id);
        self.version += 1;
        self.updated_at = Utc::now();
    }
    
    /// Sort pending jobs by priority
    fn sort_pending(&mut self) {
        self.pending.sort_by(|a, b| {
            let priority_order = b.priority - a.priority;
            if priority_order != 0 {
                return priority_order.cmp(&0);
            }
            a.created_at.cmp(&b.created_at)
        });
    }
}

impl Default for JobQueueIndex {
    fn default() -> Self {
        Self::new("default".to_string())
    }
}

/// Job type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    Replication,
    ThumbnailGeneration,
    VirusScan,
    MetadataExtraction,
}

/// Correct Job document schema for job queue
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobDocument {
    /// Schema version
    pub schema_version: u32,
    /// Unique job identifier
    pub id: Uuid,
    /// Job type
    pub job_type: JobType,
    /// Resource type
    pub resource_type: String,
    /// Resource ID
    pub resource_id: Uuid,
    /// Job status
    pub status: JobStatus,
    /// Priority (higher = more important)
    pub priority: i32,
    /// Job payload
    pub payload: serde_json::Value,
    /// Job result (if completed)
    pub result: Option<serde_json::Value>,
    /// Retry count
    pub retry_count: u32,
    /// Maximum retries
    pub max_retries: u32,
    /// Tenant ID
    pub tenant_id: Uuid,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Scheduled run time
    pub scheduled_at: DateTime<Utc>,
    /// Started at
    pub started_at: Option<DateTime<Utc>>,
    /// Completed at
    pub completed_at: Option<DateTime<Utc>>,
    /// Worker ID that claimed the job
    pub worker_id: Option<String>,
    /// Error message (if failed)
    pub error_message: Option<String>,
    /// Document version for optimistic concurrency
    pub version: u64,
}

/// Search result entry
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    /// Resource ID
    pub id: Uuid,
    /// Resource type ("file" or "folder")
    pub resource_type: String,
    /// Resource name
    pub name: String,
    /// Full path
    pub path: String,
    /// Parent folder ID
    pub parent_id: Option<Uuid>,
    /// Owner ID
    pub owner_id: Uuid,
    /// Updated at
    pub updated_at: DateTime<Utc>,
}

impl SearchResult {
    /// Create a new search result
    pub fn new(
        id: Uuid,
        resource_type: String,
        name: String,
        path: String,
        owner_id: Uuid,
        updated_at: DateTime<Utc>,
    ) -> Self {
        // Extract parent_id from path
        let parent_id = None; // Would need to look up based on path
        Self {
            id,
            resource_type,
            name,
            path,
            parent_id,
            owner_id,
            updated_at,
        }
    }
}

/// Search index entry for a resource
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchIndexEntry {
    /// Resource ID
    pub resource_id: Uuid,
    /// Resource type
    pub resource_type: String,
    /// Resource name
    pub name: String,
    /// Full path
    pub path: String,
    /// Owner ID
    pub owner_id: Uuid,
    /// Updated at
    pub updated_at: DateTime<Utc>,
}

/// Search index document (per term)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchIndexDocument {
    /// Schema version
    pub schema_version: u32,
    /// Search term
    pub term: String,
    /// Tenant ID
    pub tenant_id: Uuid,
    /// Matching entries
    pub entries: Vec<SearchIndexEntry>,
    /// Last updated
    pub updated_at: DateTime<Utc>,
    /// Document version
    pub version: u64,
}

impl SearchIndexDocument {
    /// Create a new search index document
    pub fn new(tenant_id: Uuid, term: String) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            term,
            tenant_id,
            entries: Vec::new(),
            updated_at: Utc::now(),
            version: 1,
        }
    }
    
    /// Add or update an entry
    pub fn upsert_entry(&mut self, entry: SearchIndexEntry) {
        if let Some(existing) = self.entries.iter_mut().find(|e| e.resource_id == entry.resource_id) {
            *existing = entry;
        } else {
            self.entries.push(entry);
        }
        self.updated_at = Utc::now();
        self.version += 1;
    }
    
    /// Remove an entry
    pub fn remove(&mut self, resource_id: Uuid) {
        self.entries.retain(|e| e.resource_id != resource_id);
        self.updated_at = Utc::now();
        self.version += 1;
    }
    
    /// Remove an entry (alias for compatibility)
    pub fn remove_entry(&mut self, resource_id: Uuid) {
        self.remove(resource_id);
    }
    
    /// Check if the index is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Tokenize a search query into normalized terms
pub fn tokenize_search_query(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split_whitespace()
        .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

// ============================================================================
// Tenant Config Document
// ============================================================================

/// Tenant configuration document
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TenantConfigDocument {
    /// Schema version
    pub schema_version: u32,
    /// Tenant ID
    pub tenant_id: Uuid,
    /// Recipient visibility setting
    pub recipient_visibility: RecipientVisibility,
    /// Document version for optimistic concurrency
    pub version: u64,
    /// Last updated
    pub updated_at: DateTime<Utc>,
}

use rustshare_core::domain::RecipientVisibility;

impl TenantConfigDocument {
    /// Create a new tenant config document with default settings
    pub fn new(tenant_id: Uuid) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            tenant_id,
            recipient_visibility: RecipientVisibility::default(),
            version: 1,
            updated_at: Utc::now(),
        }
    }

    /// Update recipient visibility
    pub fn set_recipient_visibility(&mut self, visibility: RecipientVisibility) {
        self.recipient_visibility = visibility;
        self.version += 1;
        self.updated_at = Utc::now();
    }
}
