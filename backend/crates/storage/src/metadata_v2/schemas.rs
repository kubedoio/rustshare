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
}
