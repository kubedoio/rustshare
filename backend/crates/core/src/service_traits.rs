//! Service traits for dependency injection
//!
//! These traits define the interface for services, allowing them to be
//! used as trait objects (dyn Trait) for dependency injection.

// Note: Using #[allow(async_fn_in_trait)] instead of async_trait crate
use uuid::Uuid;

use crate::domain::{File, FileVersion, Folder, Share};
use crate::services::{FileError, FolderError, ShareError};

/// Trait for file operations
#[allow(async_fn_in_trait)]
pub trait FileServiceTrait: Send + Sync {
    /// Upload a new file
    async fn upload_file(
        &self,
        name: String,
        data: bytes::Bytes,
        mime_type: String,
        parent_folder_id: Option<Uuid>,
        owner_id: Uuid,
    ) -> Result<File, FileError>;

    /// Get a file by ID
    async fn get_file(&self, file_id: Uuid, user_id: Uuid) -> Result<File, FileError>;

    /// Delete a file
    async fn delete_file(&self, file_id: Uuid, user_id: Uuid) -> Result<(), FileError>;

    /// Update a file (new version)
    async fn update_file(
        &self,
        file_id: Uuid,
        data: bytes::Bytes,
        user_id: Uuid,
    ) -> Result<FileVersion, FileError>;

    /// List file versions
    async fn get_file_versions(
        &self,
        file_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<FileVersion>, FileError>;

    /// Restore a file version
    async fn restore_file_version(
        &self,
        file_id: Uuid,
        version_id: Uuid,
        user_id: Uuid,
    ) -> Result<File, FileError>;

    /// Move a file to a different folder
    async fn move_file(
        &self,
        file_id: Uuid,
        target_folder_id: Option<Uuid>,
        user_id: Uuid,
    ) -> Result<File, FileError>;

    /// Rename a file
    async fn rename_file(
        &self,
        file_id: Uuid,
        new_name: String,
        user_id: Uuid,
    ) -> Result<File, FileError>;

    /// Get download URL for a file
    async fn get_download_url(
        &self,
        file_id: Uuid,
        version: Option<i32>,
        user_id: Uuid,
    ) -> Result<String, FileError>;

    /// Get preview/thumbnail URL for a file
    async fn get_preview_url(
        &self,
        file_id: Uuid,
        size: crate::domain::ThumbnailSize,
        user_id: Uuid,
    ) -> Result<String, FileError>;
}

/// Trait for folder operations
#[allow(async_fn_in_trait)]
pub trait FolderServiceTrait: Send + Sync {
    /// Create a new folder
    async fn create_folder(
        &self,
        name: String,
        parent_folder_id: Option<Uuid>,
        owner_id: Uuid,
    ) -> Result<Folder, FolderError>;

    /// Get a folder by ID
    async fn get_folder(&self, folder_id: Uuid, user_id: Uuid) -> Result<Folder, FolderError>;

    /// Delete a folder
    async fn delete_folder(&self, folder_id: Uuid, user_id: Uuid) -> Result<(), FolderError>;

    /// Move a folder to a different parent
    async fn move_folder(
        &self,
        folder_id: Uuid,
        target_parent_id: Option<Uuid>,
        user_id: Uuid,
    ) -> Result<Folder, FolderError>;

    /// Rename a folder
    async fn rename_folder(
        &self,
        folder_id: Uuid,
        new_name: String,
        user_id: Uuid,
    ) -> Result<Folder, FolderError>;

    /// Get folder tree recursively
    async fn get_tree(&self, folder_id: Uuid, user_id: Uuid) -> Result<crate::domain::FolderTree, FolderError>;
}

/// Trait for share operations
#[allow(async_fn_in_trait)]
pub trait ShareServiceTrait: Send + Sync {
    /// Create a new share for a file
    async fn create_share(
        &self,
        file_id: Uuid,
        created_by: Uuid,
        permissions: crate::domain::SharePermissions,
        password: Option<String>,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Share, ShareError>;

    /// Create a new share for a folder
    async fn create_folder_share(
        &self,
        folder_id: Uuid,
        created_by: Uuid,
        permissions: crate::domain::SharePermissions,
        password: Option<String>,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
        upload_only: bool,
    ) -> Result<Share, ShareError>;

    /// Revoke a share
    async fn revoke_share(&self, share_id: Uuid, revoked_by: Uuid) -> Result<(), ShareError>;

    /// Update share permissions
    async fn update_share_permissions(
        &self,
        share_id: Uuid,
        permissions: crate::domain::SharePermissions,
        updated_by: Uuid,
    ) -> Result<Share, ShareError>;

    /// Update share password
    async fn update_share_password(
        &self,
        share_id: Uuid,
        password: Option<String>,
        updated_by: Uuid,
    ) -> Result<Share, ShareError>;

    /// Update share expiration
    async fn update_share_expiration(
        &self,
        share_id: Uuid,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
        updated_by: Uuid,
    ) -> Result<Share, ShareError>;

    /// Validate a share token and get share info
    async fn validate_share(
        &self,
        share_token: &str,
        password: Option<&str>,
    ) -> Result<crate::services::ShareSession, ShareError>;

    /// Record share access
    async fn record_share_access(
        &self,
        share_id: Uuid,
        action: &str,
        success: bool,
        actor_type: Option<&str>,
        actor_label: Option<&str>,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<(), ShareError>;
}

// Re-export the traits
pub use self::{FileServiceTrait as FileService, FolderServiceTrait as FolderService, ShareServiceTrait as ShareService};
