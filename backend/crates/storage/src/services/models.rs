//! V2 Object-Store-Native Document Models
//! 
//! These models define the document schemas for the User Storage Domain (USD),
//! where each user's data is stored in their own isolated bucket.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rustshare_core::domain::{File, FileVersion, Folder};
use crate::PortableStorageLocator;

/// Schema version for all V2 documents
pub const SCHEMA_VERSION: u32 = 2;

/// ============================================================================
/// CANONICAL DOCUMENTS (Source of Truth)
/// ============================================================================

/// File document - stored in user's owned/files/ prefix
/// Schema version 2 adds content_hash for integrity verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDocV2 {
    pub schema_version: u32,
    pub id: Uuid,
    pub owner_id: Uuid,
    pub parent_folder_id: Option<Uuid>,
    pub name: String,
    pub path: String,
    pub current_version_id: Uuid,
    pub version_number: i32,
    pub size: i64,
    pub mime_type: String,
    pub content_hash: String,
    pub deleted: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl FileDocV2 {
    /// Create a new file document with default schema version
    pub fn new(
        id: Uuid,
        owner_id: Uuid,
        parent_folder_id: Option<Uuid>,
        name: String,
        path: String,
        version_id: Uuid,
        size: i64,
        mime_type: String,
        content_hash: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            schema_version: SCHEMA_VERSION,
            id,
            owner_id,
            parent_folder_id,
            name,
            path,
            current_version_id: version_id,
            version_number: 1,
            size,
            mime_type,
            content_hash,
            deleted: false,
            created_at: now,
            updated_at: now,
        }
    }

    /// Convert to domain File model
    pub fn to_domain(&self) -> File {
        File {
            id: self.id,
            owner_id: self.owner_id,
            name: self.name.clone(),
            path: self.path.clone(),
            size: self.size,
            mime_type: self.mime_type.clone(),
            current_version: self.version_number,
            parent_folder_id: self.parent_folder_id,
            created_at: self.created_at,
            modified_at: self.updated_at,
            deleted: self.deleted,
            content_hash: self.content_hash.clone(),
        }
    }
}

/// Folder document - stored in user's owned/folders/ prefix
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderDocV2 {
    pub schema_version: u32,
    pub id: Uuid,
    pub owner_id: Uuid,
    pub parent_folder_id: Option<Uuid>,
    pub name: String,
    pub path: String,
    pub deleted: bool,
    pub version: i32,  // For optimistic concurrency control
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl FolderDocV2 {
    /// Create a new folder document with default schema version
    pub fn new(
        id: Uuid,
        owner_id: Uuid,
        parent_folder_id: Option<Uuid>,
        name: String,
        path: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            schema_version: SCHEMA_VERSION,
            id,
            owner_id,
            parent_folder_id,
            name,
            path,
            deleted: false,
            version: 1,
            created_at: now,
            updated_at: now,
        }
    }

    /// Convert to domain Folder model
    pub fn to_domain(&self) -> Folder {
        Folder {
            id: self.id,
            owner_id: self.owner_id,
            parent_folder_id: self.parent_folder_id,
            name: self.name.clone(),
            path: self.path.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted: self.deleted,
        }
    }
}

/// ============================================================================
/// VERSION DOCUMENTS
/// ============================================================================

/// File version document - stored in user's owned/file_versions/{file_id}/{version_id}.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileVersionDocV2 {
    pub schema_version: u32,
    pub id: Uuid,
    pub file_id: Uuid,
    pub version_number: i32,
    pub size: i64,
    pub content_hash: String,
    pub storage_key: String,  // Blob storage key for this version's content
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
}

impl FileVersionDocV2 {
    pub fn new(
        id: Uuid,
        file_id: Uuid,
        version_number: i32,
        size: i64,
        content_hash: String,
        storage_key: String,
        created_by: Uuid,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            id,
            file_id,
            version_number,
            size,
            content_hash,
            storage_key,
            created_by,
            created_at: Utc::now(),
        }
    }

    /// Convert to domain FileVersion model
    pub fn to_domain(&self) -> FileVersion {
        use rustshare_core::domain::ReplicationState;
        FileVersion {
            id: self.id,
            file_id: self.file_id,
            version_number: self.version_number,
            size: self.size,
            content_hash: self.content_hash.clone(),
            replication_state: ReplicationState::PrimaryWritten,
            change_description: None,
            created_by: self.created_by,
            created_at: self.created_at,
        }
    }
}

/// ============================================================================
/// SHARE DOCUMENTS (Dual-Sided Storage)
/// ============================================================================

/// Share permissions for V2
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SharePermissionV2 {
    Read,
    Write,
    Admin,
}

impl SharePermissionV2 {
    /// Check if this permission allows write access
    pub fn can_write(&self) -> bool {
        matches!(self, Self::Write | Self::Admin)
    }

    /// Check if this permission allows admin operations
    pub fn is_admin(&self) -> bool {
        matches!(self, Self::Admin)
    }
}

/// Outbound share document - stored in sharer's owned/shares/outbound/{share_id}.json
/// Records what the owner shared and to whom
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundShareDocV2 {
    pub schema_version: u32,
    pub id: Uuid,
    pub resource_type: ShareResourceTypeV2,
    pub resource_id: Uuid,
    pub resource_locator: PortableStorageLocator,
    pub shared_with_user_id: Uuid,
    pub permissions: SharePermissionV2,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Received share document - stored in recipient's received/shares/{share_id}.json
/// Records what the recipient received and from whom
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceivedShareDocV2 {
    pub schema_version: u32,
    pub share_id: Uuid,
    pub resource_type: ShareResourceTypeV2,
    pub resource_locator: PortableStorageLocator,
    pub permissions: SharePermissionV2,
    pub shared_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Type of resource being shared
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShareResourceTypeV2 {
    File,
    Folder,
}

/// ============================================================================
/// FAVOURITES (User Preference State)
/// ============================================================================

/// Entry in the user's favourites index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FavouriteEntry {
    pub resource_id: Uuid,
    pub resource_type: FavouriteResourceType,
    pub added_at: DateTime<Utc>,
}

/// Type of resource that can be favourited
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FavouriteResourceType {
    OwnedFile,
    OwnedFolder,
    ReceivedFile,    // Via share
    ReceivedFolder,  // Via share
}

/// Favourites index document - stored in user's indexes/favourites.json
/// 
/// This is derived state that can be rebuilt by scanning user's owned and received files.
/// Favourites are user preference state - adding a favourite never modifies the owner's file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FavouritesIndex {
    pub schema_version: u32,
    pub entries: Vec<FavouriteEntry>,
}

impl FavouritesIndex {
    pub fn new() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            entries: Vec::new(),
        }
    }

    /// Add a favourite entry (idempotent)
    pub fn add(&mut self, resource_id: Uuid, resource_type: FavouriteResourceType) {
        // Check if already exists
        if self.entries.iter().any(|e| e.resource_id == resource_id) {
            return;
        }
        self.entries.push(FavouriteEntry {
            resource_id,
            resource_type,
            added_at: Utc::now(),
        });
    }

    /// Remove a favourite entry
    pub fn remove(&mut self, resource_id: Uuid) -> bool {
        let original_len = self.entries.len();
        self.entries.retain(|e| e.resource_id != resource_id);
        self.entries.len() < original_len
    }

    /// Check if a resource is favourited
    pub fn contains(&self, resource_id: Uuid) -> bool {
        self.entries.iter().any(|e| e.resource_id == resource_id)
    }
}

/// ============================================================================
/// SHARED WITH ME INDEX (Recipient-Side Derived State)
/// ============================================================================

/// Entry in the user's "shared with me" index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedWithMeEntry {
    pub share_id: Uuid,
    pub resource_id: Uuid,
    pub resource_type: ShareResourceTypeV2,
    pub shared_by: Uuid,
    pub permissions: SharePermissionV2,
    pub shared_at: DateTime<Utc>,
}

/// Shared with me index - stored in user's indexes/shared_with_me.json
/// 
/// This is derived state that can be rebuilt by scanning user's received/shares/.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SharedWithMeIndex {
    pub schema_version: u32,
    pub entries: Vec<SharedWithMeEntry>,
}

impl SharedWithMeIndex {
    pub fn new() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            entries: Vec::new(),
        }
    }

    /// Add or update an entry
    pub fn upsert(&mut self, entry: SharedWithMeEntry) {
        // Remove existing entry for this share
        self.entries.retain(|e| e.share_id != entry.share_id);
        self.entries.push(entry);
    }

    /// Remove an entry
    pub fn remove(&mut self, share_id: Uuid) -> bool {
        let original_len = self.entries.len();
        self.entries.retain(|e| e.share_id != share_id);
        self.entries.len() < original_len
    }
}

/// ============================================================================
/// FOLDER CHILDREN INDEX (No-Scan Listing)
/// ============================================================================

/// Child reference for folder children index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderChildRef {
    pub id: Uuid,
    pub name: String,
    pub resource_type: FolderChildType,
    pub deleted: bool,
}

/// Type of folder child
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FolderChildType {
    File,
    Folder,
}

/// Folder children index - stored in user's indexes/folder_children/{folder_id}.json
/// 
/// This is derived state that allows O(1) folder listing without scanning.
/// Can be rebuilt by scanning owned/files/ and owned/folders/.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FolderChildrenIndex {
    pub schema_version: u32,
    pub folder_id: Uuid,
    pub files: Vec<FolderChildRef>,
    pub folders: Vec<FolderChildRef>,
}

impl FolderChildrenIndex {
    pub fn new(folder_id: Uuid) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            folder_id,
            files: Vec::new(),
            folders: Vec::new(),
        }
    }

    /// Add a file child
    pub fn add_file(&mut self, id: Uuid, name: String) {
        self.files.push(FolderChildRef {
            id,
            name,
            resource_type: FolderChildType::File,
            deleted: false,
        });
    }

    /// Add a folder child
    pub fn add_folder(&mut self, id: Uuid, name: String) {
        self.folders.push(FolderChildRef {
            id,
            name,
            resource_type: FolderChildType::Folder,
            deleted: false,
        });
    }

    /// Mark a child as deleted
    pub fn mark_deleted(&mut self, id: Uuid) {
        if let Some(file) = self.files.iter_mut().find(|f| f.id == id) {
            file.deleted = true;
        }
        if let Some(folder) = self.folders.iter_mut().find(|f| f.id == id) {
            folder.deleted = true;
        }
    }

    /// Remove a deleted child's reference (permanent deletion)
    pub fn remove(&mut self, id: Uuid) {
        self.files.retain(|f| f.id != id);
        self.folders.retain(|f| f.id != id);
    }

    /// Get non-deleted children count
    pub fn active_count(&self) -> usize {
        self.files.iter().filter(|f| !f.deleted).count()
            + self.folders.iter().filter(|f| !f.deleted).count()
    }
}

/// ============================================================================
/// USER ROOTS INDEX (No-Scan Root Listing)
/// ============================================================================

/// User roots index - stored in user's indexes/roots.json
/// 
/// Tracks root-level files and folders for O(1) listing.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserRootsIndex {
    pub schema_version: u32,
    pub root_files: Vec<Uuid>,
    pub root_folders: Vec<Uuid>,
}

impl UserRootsIndex {
    pub fn new() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            root_files: Vec::new(),
            root_folders: Vec::new(),
        }
    }

    pub fn add_file(&mut self, id: Uuid) {
        if !self.root_files.contains(&id) {
            self.root_files.push(id);
        }
    }

    pub fn add_folder(&mut self, id: Uuid) {
        if !self.root_folders.contains(&id) {
            self.root_folders.push(id);
        }
    }

    pub fn remove_file(&mut self, id: Uuid) {
        self.root_files.retain(|&fid| fid != id);
    }

    pub fn remove_folder(&mut self, id: Uuid) {
        self.root_folders.retain(|&fid| fid != id);
    }
}

/// ============================================================================
/// TOMBSTONE DOCUMENTS (Restore Support)
/// ============================================================================

/// Tombstone document - stored when resource is soft-deleted
/// Contains all information needed to restore the resource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TombstoneDocV2 {
    pub schema_version: u32,
    pub id: Uuid,
    pub resource_type: TombstoneResourceType,
    pub resource_id: Uuid,
    pub deleted_at: DateTime<Utc>,
    pub deleted_by: Uuid,
    pub original_doc: serde_json::Value,  // The full original document as JSON
}

/// Type of resource in tombstone
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TombstoneResourceType {
    File,
    Folder,
    Share,
}

impl TombstoneDocV2 {
    /// Create tombstone from a file document
    pub fn from_file(file: &FileDocV2, deleted_by: Uuid) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            id: Uuid::new_v4(),
            resource_type: TombstoneResourceType::File,
            resource_id: file.id,
            deleted_at: Utc::now(),
            deleted_by,
            original_doc: serde_json::to_value(file).unwrap_or_default(),
        }
    }

    /// Create tombstone from a folder document
    pub fn from_folder(folder: &FolderDocV2, deleted_by: Uuid) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            id: Uuid::new_v4(),
            resource_type: TombstoneResourceType::Folder,
            resource_id: folder.id,
            deleted_at: Utc::now(),
            deleted_by,
            original_doc: serde_json::to_value(folder).unwrap_or_default(),
        }
    }
}

/// ============================================================================
/// LEGACY TYPES (for file_service.rs compatibility)
/// ============================================================================

/// Legacy file document for file_service.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDocumentV2 {
    pub schema_version: u32,
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub path: String,
    pub owner_id: Uuid,
    pub current_version_id: Uuid,
    pub version_number: i32,
    pub size: i64,
    pub mime_type: String,
    pub content_ref: String,
    pub checksum: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: i32,
    pub deleted: bool,
}

impl FileDocumentV2 {
    /// Convert to domain File model
    pub fn to_domain(&self) -> File {
        File {
            id: self.id,
            owner_id: self.owner_id,
            name: self.name.clone(),
            path: self.path.clone(),
            size: self.size,
            mime_type: self.mime_type.clone(),
            current_version: self.version_number,
            parent_folder_id: self.parent_id,
            created_at: self.created_at,
            modified_at: self.updated_at,
            deleted: self.deleted,
            content_hash: self.checksum.clone(),
        }
    }
}

/// Legacy folder document for file_service.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderDocumentV2 {
    pub schema_version: u32,
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub path: String,
    pub owner_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: i32,
    pub deleted: bool,
}

/// Legacy file version document for file_service.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileVersionDocumentV2 {
    pub schema_version: u32,
    pub id: Uuid,
    pub file_id: Uuid,
    pub version_number: i32,
    pub content_ref: String,
    pub size: i64,
    pub checksum: String,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub change_description: Option<String>,
}

impl FileVersionDocumentV2 {
    /// Convert to domain FileVersion model
    pub fn to_domain(&self) -> FileVersion {
        use rustshare_core::domain::ReplicationState;
        FileVersion {
            id: self.id,
            file_id: self.file_id,
            version_number: self.version_number,
            size: self.size,
            content_hash: self.checksum.clone(),
            replication_state: ReplicationState::PrimaryWritten,
            change_description: self.change_description.clone(),
            created_by: self.created_by,
            created_at: self.created_at,
        }
    }
}

/// Legacy tombstone document for file_service.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TombstoneDocumentV2 {
    pub schema_version: u32,
    pub id: Uuid,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub deleted_at: DateTime<Utc>,
    pub deleted_by: Uuid,
    pub previous_parent_id: Option<Uuid>,
    pub original_path: Option<String>,
    pub restore_data: serde_json::Value,
}
