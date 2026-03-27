//! Repository traits defining the contract for metadata operations

use async_trait::async_trait;
use rustshare_core::domain::{FileId, FolderId, ShareId, UserId};
use uuid::Uuid;

use super::RepositoryError;
use crate::metadata_v2::schemas::*;

/// Repository for folder operations
#[async_trait]
pub trait FolderRepository: Send + Sync {
    /// Get a folder by ID
    async fn get(&self, id: FolderId) -> Result<Option<FolderDocument>, RepositoryError>;
    
    /// Get a folder by ID, error if not found
    async fn get_required(&self, id: FolderId) -> Result<FolderDocument, RepositoryError> {
        self.get(id).await?.ok_or_else(|| {
            RepositoryError::NotFound(format!("Folder not found: {}", id))
        })
    }
    
    /// Create a new folder
    async fn create(&self, folder: &FolderDocument) -> Result<(), RepositoryError>;
    
    /// Update an existing folder
    async fn update(&self, folder: &FolderDocument) -> Result<(), RepositoryError>;
    
    /// Delete a folder (soft delete with tombstone)
    async fn delete(&self, id: FolderId, deleted_by: UserId) -> Result<(), RepositoryError>;
    
    /// Hard delete a folder (use with caution)
    async fn hard_delete(&self, id: FolderId) -> Result<(), RepositoryError>;
    
    /// List all descendant folders of a folder
    async fn list_descendants(&self, folder_id: FolderId) -> Result<Vec<FolderDocument>, RepositoryError>;
    
    /// Get root folders for a user
    async fn get_user_roots(&self, user_id: UserId) -> Result<Vec<FolderDocument>, RepositoryError>;
    
    /// Check if a folder name exists in a parent
    async fn name_exists(
        &self,
        parent_id: Option<FolderId>,
        name: &str,
        owner_id: UserId,
    ) -> Result<bool, RepositoryError>;
}

/// Repository for file operations
#[async_trait]
pub trait FileRepository: Send + Sync {
    /// Get a file by ID
    async fn get(&self, id: FileId) -> Result<Option<FileDocument>, RepositoryError>;
    
    /// Get a file by ID, error if not found
    async fn get_required(&self, id: FileId) -> Result<FileDocument, RepositoryError> {
        self.get(id).await?.ok_or_else(|| {
            RepositoryError::NotFound(format!("File not found: {}", id))
        })
    }
    
    /// Create a new file
    async fn create(&self, file: &FileDocument) -> Result<(), RepositoryError>;
    
    /// Update an existing file
    async fn update(&self, file: &FileDocument) -> Result<(), RepositoryError>;
    
    /// Delete a file (soft delete with tombstone)
    async fn delete(&self, id: FileId, deleted_by: UserId) -> Result<(), RepositoryError>;
    
    /// Hard delete a file (use with caution)
    async fn hard_delete(&self, id: FileId) -> Result<(), RepositoryError>;
    
    /// Check if a file name exists in a parent folder
    async fn name_exists(
        &self,
        parent_id: Option<FolderId>,
        name: &str,
        owner_id: UserId,
    ) -> Result<bool, RepositoryError>;
}

/// Repository for file version operations
#[async_trait]
pub trait FileVersionRepository: Send + Sync {
    /// Get a specific version
    async fn get(&self, version_id: Uuid) -> Result<Option<FileVersionDocument>, RepositoryError>;
    
    /// Get a version by file ID and version number
    async fn get_by_number(
        &self,
        file_id: FileId,
        version_number: i32,
    ) -> Result<Option<FileVersionDocument>, RepositoryError>;
    
    /// Create a new version
    async fn create(&self, version: &FileVersionDocument) -> Result<(), RepositoryError>;
    
    /// List all versions for a file
    async fn list_by_file(&self, file_id: FileId) -> Result<Vec<FileVersionDocument>, RepositoryError>;
    
    /// Get the latest version for a file
    async fn get_latest(&self, file_id: FileId) -> Result<Option<FileVersionDocument>, RepositoryError>;
}

/// Repository for share operations
#[async_trait]
pub trait ShareRepository: Send + Sync {
    /// Get a share by ID
    async fn get(&self, id: ShareId) -> Result<Option<ShareDocument>, RepositoryError>;
    
    /// Get a share by ID, error if not found
    async fn get_required(&self, id: ShareId) -> Result<ShareDocument, RepositoryError> {
        self.get(id).await?.ok_or_else(|| {
            RepositoryError::NotFound(format!("Share not found: {}", id))
        })
    }
    
    /// Get a share by token (for public shares)
    async fn get_by_token(&self, token_hash: &str) -> Result<Option<ShareDocument>, RepositoryError>;
    
    /// Create a new share
    async fn create(&self, share: &ShareDocument) -> Result<(), RepositoryError>;
    
    /// Update an existing share
    async fn update(&self, share: &ShareDocument) -> Result<(), RepositoryError>;
    
    /// Revoke a share
    async fn revoke(&self, id: ShareId, revoked_by: UserId) -> Result<(), RepositoryError>;
    
    /// Delete a share (hard delete)
    async fn delete(&self, id: ShareId) -> Result<(), RepositoryError>;
    
    /// List shares for a resource (file or folder)
    async fn list_by_resource(
        &self,
        resource_type: &str,
        resource_id: Uuid,
    ) -> Result<Vec<ShareDocument>, RepositoryError>;
    
    /// List shares created by a user
    async fn list_by_creator(&self, user_id: UserId) -> Result<Vec<ShareDocument>, RepositoryError>;
    
    /// List shares received by a user
    async fn list_by_recipient(&self, user_id: UserId) -> Result<Vec<ShareDocument>, RepositoryError>;
}

/// Repository for event operations
#[async_trait]
pub trait EventRepository: Send + Sync {
    /// Append an event
    async fn append(&self, event: &EventDocument) -> Result<(), RepositoryError>;
    
    /// Read events for a resource
    async fn read_for_resource(
        &self,
        resource_type: &str,
        resource_id: Uuid,
        limit: usize,
    ) -> Result<Vec<EventDocument>, RepositoryError>;
    
    /// Read events by time range
    async fn read_range(
        &self,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
        limit: usize,
    ) -> Result<Vec<EventDocument>, RepositoryError>;
}

/// Repository for folder children index operations
#[async_trait]
pub trait FolderChildrenIndexRepository: Send + Sync {
    /// Get the children index for a folder
    async fn get(&self, folder_id: FolderId) -> Result<Option<FolderChildrenIndex>, RepositoryError>;
    
    /// Get or create the children index
    async fn get_or_create(&self, folder_id: FolderId) -> Result<FolderChildrenIndex, RepositoryError> {
        match self.get(folder_id).await? {
            Some(index) => Ok(index),
            None => Ok(FolderChildrenIndex::new(folder_id)),
        }
    }
    
    /// Save the children index
    async fn save(&self, index: &FolderChildrenIndex) -> Result<(), RepositoryError>;
    
    /// Add a child to the index
    async fn add_child(
        &self,
        folder_id: FolderId,
        entry: FolderChildEntry,
    ) -> Result<(), RepositoryError> {
        let mut index = self.get_or_create(folder_id).await?;
        index.upsert_child(entry);
        self.save(&index).await
    }
    
    /// Remove a child from the index
    async fn remove_child(
        &self,
        folder_id: FolderId,
        child_id: Uuid,
    ) -> Result<(), RepositoryError> {
        let mut index = self.get_or_create(folder_id).await?;
        index.remove_child(child_id);
        self.save(&index).await
    }
    
    /// Mark a child as deleted
    async fn mark_deleted(
        &self,
        folder_id: FolderId,
        child_id: Uuid,
    ) -> Result<(), RepositoryError> {
        let mut index = self.get_or_create(folder_id).await?;
        index.mark_deleted(child_id);
        self.save(&index).await
    }
    
    /// Rebuild the index from canonical documents
    async fn rebuild(&self, folder_id: FolderId) -> Result<FolderChildrenIndex, RepositoryError>;
}

/// Repository for tombstone operations
#[async_trait]
pub trait TombstoneRepository: Send + Sync {
    /// Get a tombstone
    async fn get(&self, resource_type: &str, resource_id: Uuid) -> Result<Option<TombstoneDocument>, RepositoryError>;
    
    /// Create a tombstone
    async fn create(&self, tombstone: &TombstoneDocument) -> Result<(), RepositoryError>;
    
    /// List tombstones for a user
    async fn list_by_user(&self, user_id: UserId) -> Result<Vec<TombstoneDocument>, RepositoryError>;
    
    /// Hard delete a tombstone (after permanent deletion or restore)
    async fn delete(&self, resource_type: &str, resource_id: Uuid) -> Result<(), RepositoryError>;
}

/// Combined repository providing access to all operations
pub trait MetadataRepository: Send + Sync {
    fn folders(&self) -> &dyn FolderRepository;
    fn files(&self) -> &dyn FileRepository;
    fn file_versions(&self) -> &dyn FileVersionRepository;
    fn shares(&self) -> &dyn ShareRepository;
    fn events(&self) -> &dyn EventRepository;
    fn folder_children_index(&self) -> &dyn FolderChildrenIndexRepository;
    fn tombstones(&self) -> &dyn TombstoneRepository;
}

// ============================================================================
// Additional Repository Traits
// ============================================================================

use crate::metadata_v2::schemas::{
    AuditFilter, AuditLogEntryDocument, ConfigType, DeviceTokenDocument, PairingRequestDocument,
    SystemConfigDocument, UserDocument, UserFilter, UserGroupDocument, WebhookDocument,
    WebhookFilter, UserNotificationIndex, NotificationRef,
};

/// Repository for user operations
#[async_trait]
pub trait UserMetadataRepository: Send + Sync {
    /// Get a user by ID
    async fn get(&self, id: UserId) -> Result<Option<UserDocument>, RepositoryError>;
    
    /// Get a user by ID, error if not found
    async fn get_required(&self, id: UserId) -> Result<UserDocument, RepositoryError> {
        self.get(id).await?.ok_or_else(|| {
            RepositoryError::NotFound(format!("User not found: {}", id))
        })
    }
    
    /// Get a user by email
    async fn get_by_email(&self, email: &str) -> Result<Option<UserDocument>, RepositoryError>;
    
    /// Create a new user
    async fn create(&self, user: &UserDocument) -> Result<(), RepositoryError>;
    
    /// Update an existing user
    async fn update(&self, user: &UserDocument) -> Result<(), RepositoryError>;
    
    /// Delete a user
    async fn delete(&self, id: UserId) -> Result<(), RepositoryError>;
    
    /// List users with filter
    async fn list(&self, filter: UserFilter) -> Result<Vec<UserDocument>, RepositoryError>;
}

/// Repository for device token operations
#[async_trait]
pub trait DeviceRepository: Send + Sync {
    /// Get a device by ID
    async fn get(&self, id: Uuid) -> Result<Option<DeviceTokenDocument>, RepositoryError>;
    
    /// Get a device by token hash
    async fn get_by_token_hash(&self, token_hash: &str) -> Result<Option<DeviceTokenDocument>, RepositoryError>;
    
    /// Create a new device
    async fn create(&self, device: &DeviceTokenDocument) -> Result<(), RepositoryError>;
    
    /// Update an existing device
    async fn update(&self, device: &DeviceTokenDocument) -> Result<(), RepositoryError>;
    
    /// Delete a device
    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError>;
    
    /// List devices for a user
    async fn list_by_user(&self, user_id: UserId) -> Result<Vec<DeviceTokenDocument>, RepositoryError>;
}

/// Repository for group operations
#[async_trait]
pub trait GroupRepository: Send + Sync {
    /// Get a group by ID
    async fn get(&self, id: Uuid) -> Result<Option<UserGroupDocument>, RepositoryError>;
    
    /// Get a group by ID, error if not found
    async fn get_required(&self, id: Uuid) -> Result<UserGroupDocument, RepositoryError> {
        self.get(id).await?.ok_or_else(|| {
            RepositoryError::NotFound(format!("Group not found: {}", id))
        })
    }
    
    /// Create a new group
    async fn create(&self, group: &UserGroupDocument) -> Result<(), RepositoryError>;
    
    /// Update an existing group
    async fn update(&self, group: &UserGroupDocument) -> Result<(), RepositoryError>;
    
    /// Delete a group
    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError>;
    
    /// List all groups
    async fn list(&self) -> Result<Vec<UserGroupDocument>, RepositoryError>;
    
    /// Add a member to a group
    async fn add_member(&self, group_id: Uuid, user_id: UserId, added_by: UserId) -> Result<(), RepositoryError>;
    
    /// Remove a member from a group
    async fn remove_member(&self, group_id: Uuid, user_id: UserId) -> Result<(), RepositoryError>;
    
    /// List members of a group
    async fn list_members(&self, group_id: Uuid) -> Result<Vec<Uuid>, RepositoryError>;
    
    /// List groups a user is a member of
    async fn list_by_user(&self, user_id: UserId) -> Result<Vec<UserGroupDocument>, RepositoryError>;
}

/// Repository for audit log operations
#[async_trait]
pub trait AuditRepository: Send + Sync {
    /// Append an audit log entry
    async fn append(&self, entry: &AuditLogEntryDocument) -> Result<(), RepositoryError>;
    
    /// List audit log entries with filter
    async fn list(&self, filter: AuditFilter) -> Result<Vec<AuditLogEntryDocument>, RepositoryError>;
    
    /// Count audit log entries with filter
    async fn count(&self, filter: AuditFilter) -> Result<i64, RepositoryError>;
}

/// Repository for system configuration operations
#[async_trait]
pub trait ConfigRepository: Send + Sync {
    /// Get configuration by type
    async fn get(&self, config_type: ConfigType) -> Result<Option<SystemConfigDocument>, RepositoryError>;
    
    /// Get OIDC configuration
    async fn get_oidc(&self) -> Result<Option<SystemConfigDocument>, RepositoryError>;
    
    /// Get SMTP configuration
    async fn get_smtp(&self) -> Result<Option<SystemConfigDocument>, RepositoryError>;
    
    /// Set configuration
    async fn set(&self, config: &SystemConfigDocument) -> Result<(), RepositoryError>;
}

/// Repository for device pairing operations
#[async_trait]
pub trait PairingRepository: Send + Sync {
    /// Get a pairing request by ID
    async fn get(&self, id: Uuid) -> Result<Option<PairingRequestDocument>, RepositoryError>;
    
    /// Get a pairing request by user code
    async fn get_by_user_code(&self, user_code: &str) -> Result<Option<PairingRequestDocument>, RepositoryError>;
    
    /// Get a pairing request by device code
    async fn get_by_device_code(&self, device_code: &str) -> Result<Option<PairingRequestDocument>, RepositoryError>;
    
    /// Create a new pairing request
    async fn create(&self, pairing: &PairingRequestDocument) -> Result<(), RepositoryError>;
    
    /// Update a pairing request
    async fn update(&self, pairing: &PairingRequestDocument) -> Result<(), RepositoryError>;
    
    /// Delete a pairing request
    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError>;
    
    /// Clean up expired pairing requests
    async fn cleanup_expired(&self) -> Result<u64, RepositoryError>;
}

/// Repository for webhook operations
#[async_trait]
pub trait WebhookRepository: Send + Sync {
    /// Get a webhook by ID
    async fn get(&self, id: Uuid) -> Result<Option<WebhookDocument>, RepositoryError>;
    
    /// Get a webhook by ID, error if not found
    async fn get_required(&self, id: Uuid) -> Result<WebhookDocument, RepositoryError> {
        self.get(id).await?.ok_or_else(|| {
            RepositoryError::NotFound(format!("Webhook not found: {}", id))
        })
    }
    
    /// Create a new webhook
    async fn create(&self, webhook: &WebhookDocument) -> Result<(), RepositoryError>;
    
    /// Update an existing webhook
    async fn update(&self, webhook: &WebhookDocument) -> Result<(), RepositoryError>;
    
    /// Delete a webhook
    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError>;
    
    /// List webhooks with filter
    async fn list(&self, filter: WebhookFilter) -> Result<Vec<WebhookDocument>, RepositoryError>;
}

/// Repository for notification operations
#[async_trait]
pub trait NotificationRepository: Send + Sync {
    /// Get notification index for a user
    async fn get_index(&self, user_id: UserId) -> Result<UserNotificationIndex, RepositoryError>;
    
    /// Save notification index
    async fn save_index(&self, index: &UserNotificationIndex) -> Result<(), RepositoryError>;
    
    /// List notifications for a user
    async fn list(&self, user_id: UserId, unread_only: bool, offset: usize, limit: usize) -> Result<Vec<NotificationRef>, RepositoryError>;
    
    /// Count unread notifications for a user
    async fn count_unread(&self, user_id: UserId) -> Result<u32, RepositoryError>;
    
    /// Mark a notification as read
    async fn mark_read(&self, user_id: UserId, notification_id: Uuid) -> Result<(), RepositoryError>;
    
    /// Delete a notification
    async fn delete(&self, user_id: UserId, notification_id: Uuid) -> Result<(), RepositoryError>;
}

/// Extended combined repository with new operations
pub trait ExtendedMetadataRepository: MetadataRepository {
    fn users(&self) -> &dyn UserMetadataRepository;
    fn devices(&self) -> &dyn DeviceRepository;
    fn groups(&self) -> &dyn GroupRepository;
    fn audit(&self) -> &dyn AuditRepository;
    fn config(&self) -> &dyn ConfigRepository;
    fn pairing(&self) -> &dyn PairingRepository;
    fn webhooks(&self) -> &dyn WebhookRepository;
    fn notifications(&self) -> &dyn NotificationRepository;
}
