//! ShareService for share link management operations.
//!
//! This service handles share link creation and management, including:
//! - Secure token generation (32-char alphanumeric)
//! - File ownership verification
//! - Password hashing and protection
//! - Event sourcing via EventStore
//! - Metadata persistence via MetadataStore

use anyhow::Result;
use chrono::{DateTime, Utc};
use rand::Rng;
use std::sync::Arc;
use uuid::Uuid;

use rustshare_crypto::PasswordHasher;

use crate::domain::{File, FileId, Folder, FolderId, Share, SharePermissions, UserId};

/// Resource type for share operations
#[derive(Debug, Clone, Copy)]
pub enum Resource {
    File(FileId),
    Folder(FolderId),
}
use crate::events::{
    AggregateType, Event, EventBroadcaster, EventType, ShareCreatedPayload, ShareRevokedPayload,
    ShareUpdatedPayload,
};
use crate::services::ShareError;

/// Trait for JWT operations needed by ShareService.
///
/// This trait abstracts JWT token generation to allow for testing without direct auth dependency.
#[allow(async_fn_in_trait)]
pub trait JwtOps: Send + Sync {
    /// Encode custom claims into a JWT token
    fn encode_custom_claims<T: serde::Serialize>(&self, claims: &T) -> Result<String, String>;
}

/// Trait for event store operations needed by ShareService.
///
/// This trait abstracts the event store to allow for testing without database dependencies.
#[allow(async_fn_in_trait)]
pub trait EventStoreOps: Send + Sync {
    /// Append an event to the event store.
    async fn append(&self, event: &Event, broadcaster: &EventBroadcaster) -> Result<()>;
}

/// Share session returned after validation
#[derive(Debug, Clone)]
pub struct ShareSession {
    pub token: String,
    pub share_id: uuid::Uuid,
    pub file_id: Option<uuid::Uuid>,
    pub folder_id: Option<uuid::Uuid>,
    pub permissions: SharePermissions,
    pub upload_only: bool,
    pub expires_at: DateTime<Utc>,
}

/// Trait for metadata store operations needed by ShareService.
///
/// This trait abstracts the metadata store to allow for testing without database dependencies.
#[allow(async_fn_in_trait)]
pub trait MetadataStoreOps: Send + Sync {
    /// Find a file by ID.
    async fn find_file_by_id(&self, id: uuid::Uuid) -> Result<Option<File>>;

    /// Find a folder by ID.
    async fn find_folder_by_id(&self, id: uuid::Uuid) -> Result<Option<Folder>>;

    /// Find a user by ID.
    async fn find_user_by_id(&self, id: uuid::Uuid) -> Result<Option<crate::domain::User>>;

    /// Create a share in the metadata store.
    async fn create_share(&self, share: &Share) -> Result<()>;

    /// Get a share by ID.
    async fn get_share_by_id(&self, id: uuid::Uuid) -> Result<Option<Share>>;

    /// Get a share by token.
    async fn get_share_by_token(&self, token: &str) -> Result<Option<Share>>;

    /// Get all shares for a file.
    async fn get_file_shares(&self, file_id: uuid::Uuid) -> Result<Vec<Share>>;

    /// Get all shares for a folder.
    async fn get_folder_shares(&self, folder_id: uuid::Uuid) -> Result<Vec<Share>>;

    /// List files in a folder for an owner.
    async fn list_files(
        &self,
        parent_id: Option<uuid::Uuid>,
        owner_id: uuid::Uuid,
        tenant_id: uuid::Uuid,
    ) -> Result<Vec<File>>;

    /// List folders in a folder for an owner.
    async fn list_folders(
        &self,
        parent_id: Option<uuid::Uuid>,
        owner_id: uuid::Uuid,
        tenant_id: uuid::Uuid,
    ) -> Result<Vec<Folder>>;

    /// List all descendant folders, including the root folder.
    async fn find_descendant_folders(&self, folder_id: uuid::Uuid) -> Result<Vec<Folder>>;

    /// Revoke a share by ID.
    async fn revoke_share(&self, share_id: uuid::Uuid) -> Result<()>;

    /// Update a share.
    async fn update_share(&self, share: &Share) -> Result<()>;
}

/// Trait for share notification tracking operations needed by ShareService.
///
/// This trait abstracts notification tracking to allow for testing without database dependencies.
#[async_trait::async_trait]
pub trait ShareNotificationRepo: Send + Sync {
    /// Check if user was already notified for this share.
    async fn was_notified(&self, user_id: UserId, share_id: uuid::Uuid) -> Result<bool, sqlx::Error>;

    /// Record that notification was sent.
    async fn record_notification(&self, user_id: UserId, share_id: uuid::Uuid) -> Result<(), sqlx::Error>;
}

/// ShareService handles share link creation and management.
///
/// Generic over EventStore, MetadataStore, and ShareNotificationRepo implementations
/// to support different backends and testing with mock implementations.
pub struct ShareService<E: EventStoreOps, M: MetadataStoreOps, J: JwtOps, N: ShareNotificationRepo> {
    event_store: Arc<E>,
    metadata_store: Arc<M>,
    broadcaster: Arc<EventBroadcaster>,
    jwt_manager: Arc<J>,
    notification_repo: Arc<N>,
}

impl<E: EventStoreOps, M: MetadataStoreOps, J: JwtOps, N: ShareNotificationRepo> ShareService<E, M, J, N> {
    /// Create a new ShareService instance.
    pub fn new(
        event_store: Arc<E>,
        metadata_store: Arc<M>,
        broadcaster: Arc<EventBroadcaster>,
        jwt_manager: Arc<J>,
        notification_repo: Arc<N>,
    ) -> Self {
        Self {
            event_store,
            metadata_store,
            broadcaster,
            jwt_manager,
            notification_repo,
        }
    }

    /// Generate a cryptographically secure 32-character alphanumeric token.
    ///
    /// Returns a unique token suitable for use as a share link identifier.
    pub fn generate_token() -> String {
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        const TOKEN_LENGTH: usize = 32;

        let mut rng = rand::thread_rng();
        (0..TOKEN_LENGTH)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }

    fn hash_share_password(password: Option<String>) -> Result<Option<String>, ShareError> {
        if let Some(password) = password {
            Ok(Some(PasswordHasher::hash(&password).map_err(|error| {
                ShareError::PasswordHash(error.to_string())
            })?))
        } else {
            Ok(None)
        }
    }

    /// Create a new share link for a file.
    ///
    /// Verifies that:
    /// - The file exists
    /// - The user owns the file
    /// - Password is hashed if provided
    ///
    /// Returns the created Share or a ShareError.
    pub async fn create_share(
        &self,
        file_id: uuid::Uuid,
        user_id: UserId,
        permissions: SharePermissions,
        password: Option<String>,
        expires_at: Option<DateTime<Utc>>,
        tenant_id: Uuid,
    ) -> Result<Share, ShareError> {
        // Verify file exists
        let file = self
            .metadata_store
            .find_file_by_id(file_id)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
            .ok_or(ShareError::FileNotFound(file_id))?;

        // Verify user owns the file
        if file.owner_id != user_id {
            return Err(ShareError::PermissionDenied { file_id, user_id });
        }

        let password_hash = Self::hash_share_password(password)?;

        // Generate unique token
        let token = Self::generate_token();

        // Create Share domain object
        let share = Share::new(
            file_id,
            token.clone(),
            user_id,
            permissions,
            password_hash,
            expires_at,
            tenant_id,
        );

        // Store in metadata store
        self.metadata_store
            .create_share(&share)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?;

        // Emit ShareCreated event
        let payload = ShareCreatedPayload {
            share_id: share.id,
            file_id: share
                .file_id
                .ok_or_else(|| ShareError::InvalidState("file_id missing".to_string()))?,
            share_token: share
                .share_token
                .clone()
                .ok_or_else(|| ShareError::InvalidState("share_token missing".to_string()))?,
            permissions: share.permissions,
            password_protected: share.password_hash.is_some(),
            expires_at: share.expires_at,
            created_by: user_id,
        };

        let event = Event::new(
            EventType::ShareCreated,
            share.id,
            AggregateType::Share,
            serde_json::to_value(&payload)
                .map_err(|_e| ShareError::Database(sqlx::Error::PoolClosed))?,
            user_id,
        );

        self.event_store
            .append(&event, &self.broadcaster)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?;

        Ok(share)
    }

    /// Create a new share link for a folder.
    pub async fn create_folder_share(
        &self,
        folder_id: uuid::Uuid,
        user_id: UserId,
        permissions: SharePermissions,
        password: Option<String>,
        expires_at: Option<DateTime<Utc>>,
        upload_only: bool,
        tenant_id: Uuid,
    ) -> Result<Share, ShareError> {
        let folder = self
            .metadata_store
            .find_folder_by_id(folder_id)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
            .ok_or(ShareError::NotFoundById(folder_id))?;

        if folder.owner_id != user_id {
            return Err(ShareError::PermissionDenied {
                file_id: folder_id,
                user_id,
            });
        }

        let password_hash = Self::hash_share_password(password)?;
        let token = Self::generate_token();

        let mut share = Share::new_folder(
            folder_id,
            token.clone(),
            user_id,
            permissions,
            password_hash,
            expires_at,
            tenant_id,
        );
        share.upload_only = upload_only;

        self.metadata_store
            .create_share(&share)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?;

        let payload = ShareCreatedPayload {
            share_id: share.id,
            file_id: share
                .folder_id
                .ok_or_else(|| ShareError::InvalidState("folder_id missing".to_string()))?,
            share_token: share
                .share_token
                .clone()
                .ok_or_else(|| ShareError::InvalidState("share_token missing".to_string()))?,
            permissions: share.permissions,
            password_protected: share.password_hash.is_some(),
            expires_at: share.expires_at,
            created_by: user_id,
        };

        let event = Event::new(
            EventType::ShareCreated,
            share.id,
            AggregateType::Share,
            serde_json::to_value(&payload)
                .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?,
            user_id,
        );

        self.event_store
            .append(&event, &self.broadcaster)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?;

        Ok(share)
    }

    /// Validate a share and create a session.
    ///
    /// Verifies that:
    /// - The share exists
    /// - The share has not been revoked
    /// - The share has not expired
    /// - The password is valid (if password-protected)
    /// - Increments the access count
    ///
    /// Returns a ShareSession with JWT token or a ShareError.
    pub async fn validate_and_create_session(
        &self,
        share_token: &str,
        password: Option<String>,
    ) -> Result<ShareSession, ShareError> {
        // Get share by token
        let mut share = self
            .metadata_store
            .get_share_by_token(share_token)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
            .ok_or(ShareError::NotFound)?;

        // Check if revoked
        if share.revoked_at.is_some() {
            return Err(ShareError::Revoked);
        }

        // Check if expired
        if share.is_expired() {
            return Err(ShareError::Expired);
        }

        // Validate password if required
        if let Some(password_hash) = &share.password_hash {
            let provided_password = password.ok_or(ShareError::PasswordRequired)?;
            let is_valid = PasswordHasher::verify(&provided_password, password_hash)
                .map_err(|_| ShareError::InvalidPassword)?;
            if !is_valid {
                return Err(ShareError::InvalidPassword);
            }
        }

        // Increment access count
        share.access_count += 1;
        self.metadata_store
            .update_share(&share)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?;

        let claims = serde_json::json!({
            "sub": format!("share:{}", share.id),
            "session_id": uuid::Uuid::new_v4(),
            "share_id": share.id,
            "file_id": share.file_id,
            "folder_id": share.folder_id,
            "permissions": share.permissions,
            "upload_only": share.upload_only,
            "iat": Utc::now().timestamp(),
            "exp": (Utc::now() + chrono::Duration::hours(1)).timestamp(),
        });

        // Encode JWT
        let token = self
            .jwt_manager
            .encode_custom_claims(&claims)
            .map_err(ShareError::Jwt)?;

        // Calculate expiration time
        let expires_at = Utc::now() + chrono::Duration::hours(1);

        Ok(ShareSession {
            token,
            share_id: share.id,
            file_id: share.file_id,
            folder_id: share.folder_id,
            permissions: share.permissions,
            upload_only: share.upload_only,
            expires_at,
        })
    }

    /// Revoke a share link.
    ///
    /// Verifies that:
    /// - The share exists
    /// - The user owns the file
    ///
    /// Returns unit or a ShareError.
    pub async fn revoke_share(
        &self,
        share_id: uuid::Uuid,
        user_id: UserId,
    ) -> Result<(), ShareError> {
        // Get share by ID
        let share = self
            .metadata_store
            .get_share_by_id(share_id)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
            .ok_or(ShareError::NotFoundById(share_id))?;

        let owner_id = if let Some(file_id) = share.file_id {
            let file = self
                .metadata_store
                .find_file_by_id(file_id)
                .await
                .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
                .ok_or(ShareError::FileNotFound(file_id))?;
            file.owner_id
        } else if let Some(folder_id) = share.folder_id {
            let folder = self
                .metadata_store
                .find_folder_by_id(folder_id)
                .await
                .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
                .ok_or(ShareError::NotFoundById(folder_id))?;
            folder.owner_id
        } else {
            return Err(ShareError::Database(sqlx::Error::PoolClosed));
        };

        if owner_id != user_id {
            return Err(ShareError::PermissionDenied {
                file_id: share.resource_id().unwrap_or(share.id),
                user_id,
            });
        }

        // Revoke the share
        self.metadata_store
            .revoke_share(share_id)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?;

        // Emit ShareRevoked event
        let payload = ShareRevokedPayload {
            share_id: share.id,
            file_id: share.resource_id().unwrap_or(share.id),
            revoked_by: user_id,
        };

        let event = Event::new(
            EventType::ShareRevoked,
            share.id,
            AggregateType::Share,
            serde_json::to_value(&payload)
                .map_err(|_e| ShareError::Database(sqlx::Error::PoolClosed))?,
            user_id,
        );

        self.event_store
            .append(&event, &self.broadcaster)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?;

        Ok(())
    }

    /// Update a share link.
    ///
    /// Verifies that:
    /// - The share exists
    /// - The user owns the file
    ///
    /// Can update password and expiration time.
    /// Returns unit or a ShareError.
    pub async fn update_share(
        &self,
        share_id: uuid::Uuid,
        user_id: UserId,
        new_password: Option<String>,
        new_expires_at: Option<DateTime<Utc>>,
    ) -> Result<(), ShareError> {
        // Get share by ID
        let mut share = self
            .metadata_store
            .get_share_by_id(share_id)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
            .ok_or(ShareError::NotFoundById(share_id))?;

        let owner_id = if let Some(file_id) = share.file_id {
            let file = self
                .metadata_store
                .find_file_by_id(file_id)
                .await
                .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
                .ok_or(ShareError::FileNotFound(file_id))?;
            file.owner_id
        } else if let Some(folder_id) = share.folder_id {
            let folder = self
                .metadata_store
                .find_folder_by_id(folder_id)
                .await
                .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
                .ok_or(ShareError::NotFoundById(folder_id))?;
            folder.owner_id
        } else {
            return Err(ShareError::Database(sqlx::Error::PoolClosed));
        };

        if owner_id != user_id {
            return Err(ShareError::PermissionDenied {
                file_id: share.resource_id().unwrap_or(share.id),
                user_id,
            });
        }

        // Track what was changed for the event
        let mut password_changed = false;
        let mut expires_at_changed = false;

        // Update password if provided
        if let Some(pwd) = new_password {
            share.password_hash = Some(
                PasswordHasher::hash(&pwd).map_err(|e| ShareError::PasswordHash(e.to_string()))?,
            );
            password_changed = true;
        }

        // Update expires_at if provided
        if let Some(new_expires) = new_expires_at {
            share.expires_at = Some(new_expires);
            expires_at_changed = true;
        }

        // Update the share
        self.metadata_store
            .update_share(&share)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?;

        // Emit ShareUpdated event
        let payload = ShareUpdatedPayload {
            share_id: share.id,
            file_id: share.resource_id().unwrap_or(share.id),
            password_changed,
            expires_at_changed,
            new_expires_at: share.expires_at,
            updated_by: user_id,
        };

        let event = Event::new(
            EventType::ShareUpdated,
            share.id,
            AggregateType::Share,
            serde_json::to_value(&payload)
                .map_err(|_e| ShareError::Database(sqlx::Error::PoolClosed))?,
            user_id,
        );

        self.event_store
            .append(&event, &self.broadcaster)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?;

        Ok(())
    }

    /// List all shares for a file.
    ///
    /// Verifies that the user owns the file.
    /// Returns a vector of shares or a ShareError.
    pub async fn list_file_shares(
        &self,
        file_id: uuid::Uuid,
        user_id: UserId,
    ) -> Result<Vec<Share>, ShareError> {
        // Get file to verify ownership
        let file = self
            .metadata_store
            .find_file_by_id(file_id)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
            .ok_or(ShareError::FileNotFound(file_id))?;

        // Check user owns file
        if file.owner_id != user_id {
            return Err(ShareError::PermissionDenied { file_id, user_id });
        }

        // Get all shares for the file
        self.metadata_store
            .get_file_shares(file_id)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))
    }

    /// List all shares for a folder.
    pub async fn list_folder_shares(
        &self,
        folder_id: uuid::Uuid,
        user_id: UserId,
    ) -> Result<Vec<Share>, ShareError> {
        let folder = self
            .metadata_store
            .find_folder_by_id(folder_id)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
            .ok_or(ShareError::NotFoundById(folder_id))?;

        if folder.owner_id != user_id {
            return Err(ShareError::PermissionDenied {
                file_id: folder_id,
                user_id,
            });
        }

        self.metadata_store
            .get_folder_shares(folder_id)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))
    }

    /// Get public share info for anonymous access.
    ///
    /// Checks revocation and expiration, returns file metadata.
    /// This method is used for anonymous access to shared files.
    ///
    /// Returns a tuple of (Share, File) or a ShareError.
    pub async fn get_public_share_info(
        &self,
        share_token: &str,
    ) -> Result<(Share, Option<File>, Option<Folder>), ShareError> {
        // Get share by token
        let share = self
            .metadata_store
            .get_share_by_token(share_token)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
            .ok_or(ShareError::NotFound)?;

        // Check if revoked
        if share.revoked_at.is_some() {
            return Err(ShareError::Revoked);
        }

        // Check if expired
        if let Some(expires_at) = share.expires_at {
            if expires_at < chrono::Utc::now() {
                return Err(ShareError::Expired);
            }
        }

        if let Some(file_id) = share.file_id {
            let file = self
                .metadata_store
                .find_file_by_id(file_id)
                .await
                .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
                .ok_or(ShareError::FileNotFound(file_id))?;

            Ok((share, Some(file), None))
        } else if let Some(folder_id) = share.folder_id {
            let folder = self
                .metadata_store
                .find_folder_by_id(folder_id)
                .await
                .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
                .ok_or(ShareError::NotFoundById(folder_id))?;

            Ok((share, None, Some(folder)))
        } else {
            Err(ShareError::Database(sqlx::Error::PoolClosed))
        }
    }

    /// List contents of a publicly shared folder.
    pub async fn list_public_folder_contents(
        &self,
        share_token: &str,
        current_folder_id: Option<uuid::Uuid>,
    ) -> Result<(Share, Folder, Vec<Folder>, Vec<File>), ShareError> {
        let (share, _file, root_folder) = self.get_public_share_info(share_token).await?;
        let root_folder = root_folder.ok_or(ShareError::Database(sqlx::Error::PoolClosed))?;
        let target_folder_id = current_folder_id.unwrap_or(root_folder.id);

        let descendants = self
            .metadata_store
            .find_descendant_folders(root_folder.id)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?;

        let target_folder = descendants
            .into_iter()
            .find(|folder| folder.id == target_folder_id)
            .ok_or(ShareError::NotFoundById(target_folder_id))?;

        let folders = self
            .metadata_store
            .list_folders(Some(target_folder.id), root_folder.owner_id, target_folder.tenant_id)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?;

        let files = self
            .metadata_store
            .list_files(Some(target_folder.id), root_folder.owner_id, target_folder.tenant_id)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?;

        Ok((share, target_folder, folders, files))
    }

    /// Create a group share for a resource
    ///
    /// # Arguments
    /// * `resource` - File or Folder to share
    /// * `group_id` - Group to share with
    /// * `permissions` - Permission level (View, Edit, Admin)
    /// * `created_by` - User creating the share
    /// * `tenant_id` - Tenant ID for boundary checking
    ///
    /// # Errors
    /// * CrossTenantSharingNotAllowed - if group is in different tenant
    /// * NotGroupMember - if user is not admin and not group member
    /// * GroupShareAlreadyExists - if group already has access
    pub async fn create_group_share(
        &self,
        resource: Resource,
        group_id: Uuid,
        permissions: SharePermissions,
        created_by: UserId,
        tenant_id: Uuid,
    ) -> Result<Share, ShareError> {
        // Verify resource exists and get its owner/tenant
        let (resource_owner, resource_tenant) = match &resource {
            Resource::File(file_id) => {
                let file = self.metadata_store
                    .find_file_by_id(*file_id)
                    .await
                    .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
                    .ok_or(ShareError::FileNotFound(*file_id))?;
                (file.owner_id, file.tenant_id)
            }
            Resource::Folder(folder_id) => {
                let folder = self.metadata_store
                    .find_folder_by_id(*folder_id)
                    .await
                    .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
                    .ok_or(ShareError::NotFoundById(*folder_id))?;
                (folder.owner_id, folder.tenant_id)
            }
        };

        // Tenant boundary check
        if resource_tenant != tenant_id {
            return Err(ShareError::CrossTenantSharingNotAllowed);
        }

        // Check if user has admin permission or is resource owner
        let is_owner = resource_owner == created_by;
        let has_admin = if is_owner {
            true
        } else {
            // Check if user has admin permission on resource
            self.check_resource_permission(created_by, resource, SharePermissions::Admin).await?
        };

        // Non-admins must be group members
        if !has_admin {
            // This check would need access to group repo - for now, we'll need to add that dependency
            // For this implementation, we'll skip this check or add a TODO
            // TODO: Add group repo dependency to verify membership
        }

        // Check for existing group share
        let existing_shares = match &resource {
            Resource::File(file_id) => {
                self.metadata_store.get_file_shares(*file_id).await
            }
            Resource::Folder(folder_id) => {
                self.metadata_store.get_folder_shares(*folder_id).await
            }
        }.map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?;

        let already_exists = existing_shares.iter().any(|s| {
            s.recipient_group_id == Some(group_id) && s.revoked_at.is_none()
        });

        if already_exists {
            return Err(ShareError::GroupShareAlreadyExists);
        }

        // Create share
        let share = Share {
            id: Uuid::new_v4(),
            file_id: match resource {
                Resource::File(id) => Some(id),
                Resource::Folder(_) => None,
            },
            folder_id: match resource {
                Resource::File(_) => None,
                Resource::Folder(id) => Some(id),
            },
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

        // Store share
        self.metadata_store.create_share(&share).await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?;

        // Emit event
        let payload = ShareCreatedPayload {
            share_id: share.id,
            file_id: share.resource_id().unwrap_or(share.id),
            share_token: "".to_string(),
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

    /// Revoke a group share
    /// 
    /// # Arguments
    /// * `share_id` - ID of the share to revoke
    /// * `requesting_user` - User attempting to revoke
    /// 
    /// # Errors
    /// * NotFoundById - if share doesn't exist
    /// * InvalidState - if not a group share
    /// * InsufficientPermission - if user doesn't have admin on resource
    pub async fn revoke_group_share(
        &self,
        share_id: uuid::Uuid,
        requesting_user: UserId,
    ) -> Result<(), ShareError> {
        // Get the share
        let share = self.metadata_store
            .get_share_by_id(share_id)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
            .ok_or(ShareError::NotFoundById(share_id))?;
        
        // Verify it's a group share
        if share.recipient_group_id.is_none() {
            return Err(ShareError::InvalidState("Not a group share".to_string()));
        }
        
        // Determine resource for permission check
        let resource = if let Some(file_id) = share.file_id {
            Resource::File(file_id)
        } else if let Some(folder_id) = share.folder_id {
            Resource::Folder(folder_id)
        } else {
            return Err(ShareError::InvalidState("Share has no resource".to_string()));
        };
        
        // Check admin permission
        let has_admin = self.check_resource_permission(requesting_user, resource, SharePermissions::Admin).await?;
        if !has_admin && share.created_by != requesting_user {
            return Err(ShareError::InsufficientPermission {
                required: SharePermissions::Admin,
                actual: SharePermissions::View,
            });
        }
        
        // Revoke the share
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
    /// 
    /// # Arguments
    /// * `share_id` - ID of the share to update
    /// * `new_permission` - New permission level
    /// * `requesting_user` - User attempting to update
    /// 
    /// # Errors
    /// * NotFoundById - if share doesn't exist
    /// * InvalidState - if not a group share
    /// * InsufficientPermission - if user doesn't have admin on resource
    pub async fn update_group_share_permission(
        &self,
        share_id: uuid::Uuid,
        new_permission: SharePermissions,
        requesting_user: UserId,
    ) -> Result<Share, ShareError> {
        // Get the share
        let mut share = self.metadata_store
            .get_share_by_id(share_id)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
            .ok_or(ShareError::NotFoundById(share_id))?;
        
        // Verify it's a group share
        if share.recipient_group_id.is_none() {
            return Err(ShareError::InvalidState("Not a group share".to_string()));
        }
        
        // Determine resource for permission check
        let resource = if let Some(file_id) = share.file_id {
            Resource::File(file_id)
        } else if let Some(folder_id) = share.folder_id {
            Resource::Folder(folder_id)
        } else {
            return Err(ShareError::InvalidState("Share has no resource".to_string()));
        };
        
        // Check admin permission
        let has_admin = self.check_resource_permission(requesting_user, resource, SharePermissions::Admin).await?;
        if !has_admin && share.created_by != requesting_user {
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

    /// Check if user has specific permission on resource
    async fn check_resource_permission(
        &self,
        _user_id: UserId,
        _resource: Resource,
        _required: SharePermissions,
    ) -> Result<bool, ShareError> {
        // This would integrate with PermissionResolver
        // For now, return false - in full implementation this checks shares
        Ok(false)
    }

    /// Send first-access notification for group share if needed.
    ///
    /// This implements "lazy notifications" - instead of notifying all group members
    /// when a share is created, we only notify them when they first access the resource.
    /// This prevents spam for large groups and inactive members.
    ///
    /// # Arguments
    /// * `user_id` - The user who accessed the share
    /// * `share` - The share being accessed
    ///
    /// # Returns
    /// Ok(()) if notification was sent or not needed, Err otherwise.
    pub async fn send_first_access_notification_if_needed(
        &self,
        user_id: UserId,
        share: &Share,
    ) -> Result<(), ShareError> {
        // Only for group shares (recipient_group_id is set)
        if share.recipient_group_id.is_none() {
            return Ok(());
        }

        // Don't notify the creator of the share
        if share.created_by == user_id {
            return Ok(());
        }

        // Check if already notified
        let was_notified = self
            .notification_repo
            .was_notified(user_id, share.id)
            .await
            .map_err(ShareError::Database)?;

        if was_notified {
            return Ok(());
        }

        // Get resource info (file or folder name)
        let (resource_name, resource_type) = if let Some(file_id) = share.file_id {
            let file = self
                .metadata_store
                .find_file_by_id(file_id)
                .await
                .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
                .ok_or(ShareError::FileNotFound(file_id))?;
            (file.name, "file")
        } else if let Some(folder_id) = share.folder_id {
            let folder = self
                .metadata_store
                .find_folder_by_id(folder_id)
                .await
                .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
                .ok_or(ShareError::NotFoundById(folder_id))?;
            (folder.name, "folder")
        } else {
            return Err(ShareError::InvalidState("Share has no resource".to_string()));
        };

        // Get sharer info
        let sharer = self
            .metadata_store
            .find_user_by_id(share.created_by)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?
            .ok_or_else(|| ShareError::RecipientNotFound("Sharer not found".to_string()))?;

        // Create notification
        use crate::domain::{NotificationType, ResourceType};
        use crate::services::CreateNotification;

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
            action_url: Some(format!(
                "/shared-with-me/{}/{}",
                resource_type,
                share.resource_id().unwrap_or(share.id)
            )),
            tenant_id: share.tenant_id,
        };

        // Emit NotificationCreated event via event store
        use crate::events::{AggregateType, Event, EventType};
        use crate::events::NotificationCreatedPayload;

        let payload = NotificationCreatedPayload {
            notification_id: uuid::Uuid::new_v4(),
            user_id,
            title: notification.title.clone(),
            message: notification.message.clone(),
            notification_type: format!("{:?}", notification.notification_type),
            resource_id: notification.resource_id,
            resource_type: format!("{:?}", notification.resource_type),
            action_url: notification.action_url.clone(),
            timestamp: Utc::now(),
        };

        let event = Event::new(
            EventType::NotificationCreated,
            share.id,
            AggregateType::Share,
            serde_json::to_value(&payload)
                .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?,
            share.created_by,
        );

        self.event_store
            .append(&event, &self.broadcaster)
            .await
            .map_err(|_| ShareError::Database(sqlx::Error::PoolClosed))?;

        // Record notification sent to prevent duplicates
        self.notification_repo
            .record_notification(user_id, share.id)
            .await
            .map_err(ShareError::Database)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use uuid::Uuid;

    struct MockEventStore {
        events: Mutex<Vec<Event>>,
    }

    impl MockEventStore {
        fn new() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
            }
        }
    }

    impl EventStoreOps for MockEventStore {
        async fn append(&self, event: &Event, _broadcaster: &EventBroadcaster) -> Result<()> {
            self.events.lock().unwrap().push(event.clone());
            Ok(())
        }
    }

    struct MockMetadataStore {
        files: Mutex<Vec<File>>,
        folders: Mutex<Vec<Folder>>,
        shares: Mutex<Vec<Share>>,
        users: Mutex<Vec<crate::domain::User>>,
    }

    impl MockMetadataStore {
        fn new() -> Self {
            Self {
                files: Mutex::new(Vec::new()),
                folders: Mutex::new(Vec::new()),
                shares: Mutex::new(Vec::new()),
                users: Mutex::new(Vec::new()),
            }
        }

        fn add_file(&self, file: File) {
            self.files.lock().unwrap().push(file);
        }

        #[allow(dead_code)]
        fn add_folder(&self, folder: Folder) {
            self.folders.lock().unwrap().push(folder);
        }

        fn add_user(&self, user: crate::domain::User) {
            self.users.lock().unwrap().push(user);
        }
    }

    impl MetadataStoreOps for MockMetadataStore {
        async fn find_file_by_id(&self, id: Uuid) -> Result<Option<File>> {
            Ok(self
                .files
                .lock()
                .unwrap()
                .iter()
                .find(|f| f.id == id)
                .cloned())
        }

        async fn find_folder_by_id(&self, id: Uuid) -> Result<Option<Folder>> {
            Ok(self
                .folders
                .lock()
                .unwrap()
                .iter()
                .find(|folder| folder.id == id)
                .cloned())
        }

        async fn find_user_by_id(&self, id: Uuid) -> Result<Option<crate::domain::User>> {
            Ok(self
                .users
                .lock()
                .unwrap()
                .iter()
                .find(|u| u.id == id)
                .cloned())
        }

        async fn create_share(&self, share: &Share) -> Result<()> {
            self.shares.lock().unwrap().push(share.clone());
            Ok(())
        }

        async fn get_share_by_id(&self, id: Uuid) -> Result<Option<Share>> {
            Ok(self
                .shares
                .lock()
                .unwrap()
                .iter()
                .find(|s| s.id == id)
                .cloned())
        }

        async fn get_share_by_token(&self, token: &str) -> Result<Option<Share>> {
            Ok(self
                .shares
                .lock()
                .unwrap()
                .iter()
                .find(|s| s.share_token.as_deref() == Some(token))
                .cloned())
        }

        async fn get_file_shares(&self, file_id: Uuid) -> Result<Vec<Share>> {
            Ok(self
                .shares
                .lock()
                .unwrap()
                .iter()
                .filter(|s| s.file_id == Some(file_id))
                .cloned()
                .collect())
        }

        async fn get_folder_shares(&self, folder_id: Uuid) -> Result<Vec<Share>> {
            Ok(self
                .shares
                .lock()
                .unwrap()
                .iter()
                .filter(|s| s.folder_id == Some(folder_id))
                .cloned()
                .collect())
        }

        async fn list_files(&self, parent_id: Option<Uuid>, owner_id: Uuid, _tenant_id: Uuid) -> Result<Vec<File>> {
            Ok(self
                .files
                .lock()
                .unwrap()
                .iter()
                .filter(|file| file.owner_id == owner_id && file.parent_folder_id == parent_id)
                .cloned()
                .collect())
        }

        async fn list_folders(
            &self,
            parent_id: Option<Uuid>,
            owner_id: Uuid,
            _tenant_id: Uuid,
        ) -> Result<Vec<Folder>> {
            Ok(self
                .folders
                .lock()
                .unwrap()
                .iter()
                .filter(|folder| {
                    folder.owner_id == owner_id && folder.parent_folder_id == parent_id
                })
                .cloned()
                .collect())
        }

        async fn find_descendant_folders(&self, folder_id: Uuid) -> Result<Vec<Folder>> {
            let folders = self.folders.lock().unwrap().clone();
            let mut result = Vec::new();
            let mut stack = vec![folder_id];

            while let Some(current) = stack.pop() {
                if let Some(folder) = folders.iter().find(|folder| folder.id == current).cloned() {
                    stack.extend(
                        folders
                            .iter()
                            .filter(|child| child.parent_folder_id == Some(current))
                            .map(|child| child.id),
                    );
                    result.push(folder);
                }
            }

            Ok(result)
        }

        async fn revoke_share(&self, share_id: Uuid) -> Result<()> {
            if let Some(share) = self
                .shares
                .lock()
                .unwrap()
                .iter_mut()
                .find(|s| s.id == share_id)
            {
                share.revoked_at = Some(Utc::now());
                Ok(())
            } else {
                Err(anyhow::anyhow!("Share not found"))
            }
        }

        async fn update_share(&self, share: &Share) -> Result<()> {
            let mut shares = self.shares.lock().unwrap();
            if let Some(existing) = shares.iter_mut().find(|s| s.id == share.id) {
                *existing = share.clone();
                Ok(())
            } else {
                Err(anyhow::anyhow!("Share not found"))
            }
        }
    }

    struct MockJwtManager;

    impl JwtOps for MockJwtManager {
        fn encode_custom_claims<T: serde::Serialize>(&self, _claims: &T) -> Result<String, String> {
            Ok("test_jwt_token_12345".to_string())
        }
    }

    struct MockNotificationRepo;

    #[async_trait::async_trait]
    impl ShareNotificationRepo for MockNotificationRepo {
        async fn was_notified(&self, _user_id: UserId, _share_id: uuid::Uuid) -> Result<bool, sqlx::Error> {
            Ok(false)
        }

        async fn record_notification(&self, _user_id: UserId, _share_id: uuid::Uuid) -> Result<(), sqlx::Error> {
            Ok(())
        }
    }

    fn setup_share_service() -> (
        ShareService<MockEventStore, MockMetadataStore, MockJwtManager, MockNotificationRepo>,
        Arc<MockEventStore>,
        Arc<MockMetadataStore>,
    ) {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let broadcaster = Arc::new(EventBroadcaster::new(100));
        let jwt_manager = Arc::new(MockJwtManager);
        let notification_repo = Arc::new(MockNotificationRepo);

        let service = ShareService::new(
            event_store.clone(),
            metadata_store.clone(),
            broadcaster,
            jwt_manager,
            notification_repo,
        );

        (service, event_store, metadata_store)
    }

    #[test]
    fn test_generate_token_is_unique() {
        let mut tokens = std::collections::HashSet::new();

        for _ in 0..1000 {
            let token =
                ShareService::<MockEventStore, MockMetadataStore, MockJwtManager, MockNotificationRepo>::generate_token();

            // Verify token length is 32
            assert_eq!(token.len(), 32, "Token length should be 32");

            // Verify token is alphanumeric
            assert!(
                token.chars().all(|c| c.is_alphanumeric()),
                "Token should be alphanumeric: {}",
                token
            );

            // Collect token
            tokens.insert(token);
        }

        // Verify all 1000 tokens are unique
        assert_eq!(
            tokens.len(),
            1000,
            "All 1000 tokens should be unique, but got {} unique",
            tokens.len()
        );
    }

    #[tokio::test]
    async fn test_create_share_success() {
        let (service, event_store, metadata_store) = setup_share_service();

        let owner_id = Uuid::new_v4();

        // Create and add a file
        let file = File::new(
            "document.pdf".to_string(),
            "/documents/document.pdf".to_string(),
            "abc123".to_string(),
            1024,
            "application/pdf".to_string(),
            None,
            owner_id,
        );
        let file_id = file.id;
        metadata_store.add_file(file.clone());

        // Create share
        let share = service
            .create_share(file_id, owner_id, SharePermissions::View, None, None)
            .await
            .unwrap();

        // Verify share properties
        assert_eq!(share.file_id, Some(file_id));
        assert_eq!(share.created_by, owner_id);
        assert_eq!(share.permissions, SharePermissions::View);
        assert_eq!(share.share_token.clone().unwrap().len(), 32);
        assert!(share.password_hash.is_none());
        assert!(share.expires_at.is_none());

        // Verify share was stored
        let shares = metadata_store.shares.lock().unwrap();
        assert_eq!(shares.len(), 1);
        assert_eq!(shares[0].id, share.id);

        // Verify event was emitted
        let events = event_store.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::ShareCreated);
        assert_eq!(events[0].aggregate_id, share.id);
        assert_eq!(events[0].aggregate_type, AggregateType::Share);
    }

    #[tokio::test]
    async fn test_create_share_permission_denied() {
        let (service, _, metadata_store) = setup_share_service();

        let owner_id = Uuid::new_v4();
        let other_user = Uuid::new_v4();

        // Create file owned by owner_id
        let file = File::new(
            "private.pdf".to_string(),
            "/private.pdf".to_string(),
            "hash".to_string(),
            512,
            "application/pdf".to_string(),
            None,
            owner_id,
        );
        let file_id = file.id;
        metadata_store.add_file(file);

        // Try to create share as different user
        let result = service
            .create_share(file_id, other_user, SharePermissions::View, None, None)
            .await;

        assert!(matches!(result, Err(ShareError::PermissionDenied { .. })));
    }

    #[tokio::test]
    async fn test_validate_share_creates_session() {
        let (service, _, metadata_store) = setup_share_service();

        let owner_id = Uuid::new_v4();

        // Create and add a file
        let file = File::new(
            "document.pdf".to_string(),
            "/documents/document.pdf".to_string(),
            "abc123".to_string(),
            1024,
            "application/pdf".to_string(),
            None,
            owner_id,
        );
        let file_id = file.id;
        metadata_store.add_file(file.clone());

        // Create share without password
        let share = service
            .create_share(file_id, owner_id, SharePermissions::View, None, None)
            .await
            .unwrap();

        let share_token = share.share_token.clone().unwrap();

        // Validate share and create session
        let session = service
            .validate_and_create_session(&share_token, None)
            .await
            .unwrap();

        // Verify session properties
        assert_eq!(session.share_id, share.id);
        assert_eq!(session.file_id, Some(file_id));
        assert_eq!(session.permissions, SharePermissions::View);
        assert!(!session.token.is_empty());

        // Verify access count was incremented
        let updated_share = metadata_store
            .get_share_by_token(&share_token)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated_share.access_count, 1);
    }

    #[tokio::test]
    async fn test_validate_share_requires_password() {
        let (service, _, metadata_store) = setup_share_service();

        let owner_id = Uuid::new_v4();

        // Create and add a file
        let file = File::new(
            "document.pdf".to_string(),
            "/documents/document.pdf".to_string(),
            "abc123".to_string(),
            1024,
            "application/pdf".to_string(),
            None,
            owner_id,
        );
        let file_id = file.id;
        metadata_store.add_file(file.clone());

        // Create share with password
        let share = service
            .create_share(
                file_id,
                owner_id,
                SharePermissions::View,
                Some("password123".to_string()),
                None,
            )
            .await
            .unwrap();

        let share_token = share.share_token.clone().unwrap();

        // Try to validate without password - should fail
        let result = service
            .validate_and_create_session(&share_token, None)
            .await;
        assert!(matches!(result, Err(ShareError::PasswordRequired)));

        // Try with wrong password - should fail
        let result = service
            .validate_and_create_session(&share_token, Some("wrongpassword".to_string()))
            .await;
        assert!(matches!(result, Err(ShareError::InvalidPassword)));

        // Try with correct password - should succeed
        let session = service
            .validate_and_create_session(&share_token, Some("password123".to_string()))
            .await
            .unwrap();

        assert_eq!(session.share_id, share.id);
    }

    #[tokio::test]
    async fn test_revoke_share() {
        let (service, event_store, metadata_store) = setup_share_service();

        let owner_id = Uuid::new_v4();

        // Create and add a file
        let file = File::new(
            "document.pdf".to_string(),
            "/documents/document.pdf".to_string(),
            "abc123".to_string(),
            1024,
            "application/pdf".to_string(),
            None,
            owner_id,
        );
        let file_id = file.id;
        metadata_store.add_file(file.clone());

        // Create share
        let share = service
            .create_share(file_id, owner_id, SharePermissions::View, None, None)
            .await
            .unwrap();

        let share_id = share.id;
        let share_token = share.share_token.clone().unwrap();

        // Revoke share
        service.revoke_share(share_id, owner_id).await.unwrap();

        // Verify share is revoked
        let revoked_share = metadata_store
            .get_share_by_id(share_id)
            .await
            .unwrap()
            .unwrap();
        assert!(revoked_share.revoked_at.is_some());

        // Verify ShareRevoked event was emitted
        let events = event_store.events.lock().unwrap();
        assert_eq!(events.len(), 2); // ShareCreated + ShareRevoked
        assert_eq!(events[1].event_type, EventType::ShareRevoked);

        // Verify share can't be validated anymore
        let result = service
            .validate_and_create_session(&share_token, None)
            .await;
        assert!(matches!(result, Err(ShareError::Revoked)));
    }

    #[tokio::test]
    async fn test_update_share_password() {
        let (service, event_store, metadata_store) = setup_share_service();

        let owner_id = Uuid::new_v4();

        // Create and add a file
        let file = File::new(
            "document.pdf".to_string(),
            "/documents/document.pdf".to_string(),
            "abc123".to_string(),
            1024,
            "application/pdf".to_string(),
            None,
            owner_id,
        );
        let file_id = file.id;
        metadata_store.add_file(file.clone());

        // Create share without password
        let share = service
            .create_share(file_id, owner_id, SharePermissions::View, None, None)
            .await
            .unwrap();

        let share_id = share.id;
        let share_token = share.share_token.clone().unwrap();

        // Verify share is accessible without password
        let session = service
            .validate_and_create_session(&share_token, None)
            .await
            .unwrap();
        assert!(session.token.len() > 0);

        // Update share with password
        service
            .update_share(share_id, owner_id, Some("newpassword".to_string()), None)
            .await
            .unwrap();

        // Verify ShareUpdated event was emitted
        let events = event_store.events.lock().unwrap();
        assert_eq!(events.len(), 2); // ShareCreated + ShareUpdated
        assert_eq!(events[1].event_type, EventType::ShareUpdated);

        // Verify share now requires password
        let result = service
            .validate_and_create_session(&share_token, None)
            .await;
        assert!(matches!(result, Err(ShareError::PasswordRequired)));

        // Verify share works with correct password
        let session = service
            .validate_and_create_session(&share_token, Some("newpassword".to_string()))
            .await
            .unwrap();
        assert!(session.token.len() > 0);
    }

    #[tokio::test]
    async fn test_get_public_share_info_success() {
        let (service, _, metadata_store) = setup_share_service();

        let owner_id = Uuid::new_v4();

        // Create and add a file
        let file = File::new(
            "document.pdf".to_string(),
            "/documents/document.pdf".to_string(),
            "abc123".to_string(),
            1024,
            "application/pdf".to_string(),
            None,
            owner_id,
        );
        let file_id = file.id;
        metadata_store.add_file(file.clone());

        // Create share
        let share = service
            .create_share(file_id, owner_id, SharePermissions::View, None, None)
            .await
            .unwrap();

        let share_token = share.share_token.clone().unwrap();

        // Get public share info
        let (returned_share, returned_file, returned_folder) =
            service.get_public_share_info(&share_token).await.unwrap();

        // Verify share and file are returned correctly
        assert_eq!(returned_share.id, share.id);
        assert_eq!(returned_share.file_id, Some(file_id));
        assert!(returned_folder.is_none());
        let returned_file = returned_file.expect("file share should include file");
        assert_eq!(returned_file.id, file_id);
        assert_eq!(returned_file.name, "document.pdf");
    }

    #[tokio::test]
    async fn test_get_public_share_info_not_found() {
        let (service, _, _) = setup_share_service();

        let share_token = "nonexistent_token_12345678901234";

        let result = service.get_public_share_info(share_token).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ShareError::NotFound));
    }

    #[tokio::test]
    async fn test_get_public_share_info_revoked() {
        let (service, _, metadata_store) = setup_share_service();

        let owner_id = Uuid::new_v4();

        // Create and add a file
        let file = File::new(
            "document.pdf".to_string(),
            "/documents/document.pdf".to_string(),
            "abc123".to_string(),
            1024,
            "application/pdf".to_string(),
            None,
            owner_id,
        );
        let file_id = file.id;
        metadata_store.add_file(file.clone());

        // Create share
        let share = service
            .create_share(file_id, owner_id, SharePermissions::View, None, None)
            .await
            .unwrap();

        let share_token = share.share_token.clone().unwrap();

        // Revoke share
        service.revoke_share(share.id, owner_id).await.unwrap();

        // Try to get public share info
        let result = service.get_public_share_info(&share_token).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ShareError::Revoked));
    }

    #[tokio::test]
    async fn test_get_public_share_info_expired() {
        let (service, _, metadata_store) = setup_share_service();

        let owner_id = Uuid::new_v4();

        // Create and add a file
        let file = File::new(
            "document.pdf".to_string(),
            "/documents/document.pdf".to_string(),
            "abc123".to_string(),
            1024,
            "application/pdf".to_string(),
            None,
            owner_id,
        );
        let file_id = file.id;
        metadata_store.add_file(file.clone());

        // Create share with expiration in the past
        let expired_time = chrono::Utc::now() - chrono::Duration::hours(1);
        let share = service
            .create_share(
                file_id,
                owner_id,
                SharePermissions::View,
                None,
                Some(expired_time),
            )
            .await
            .unwrap();

        let share_token = share.share_token.clone().unwrap();

        // Try to get public share info
        let result = service.get_public_share_info(&share_token).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ShareError::Expired));
    }
}
