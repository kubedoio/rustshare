use anyhow::Result;
use std::sync::Arc;

use crate::domain::{
    File, Folder, Share, ShareId, SharePermissions, ShareRecipient, User, FileId, FolderId, UserId,
};
use crate::services::{NotificationService, PermissionResolver, Resource, ShareError};

/// Trait for share repository operations needed by UserShareService.
#[allow(async_fn_in_trait)]
pub trait ShareOps: Send + Sync {
    async fn find_user_share(
        &self,
        file_id: Option<FileId>,
        folder_id: Option<FolderId>,
        recipient_user_id: UserId,
    ) -> Result<Option<Share>, sqlx::Error>;

    async fn create_user_share(
        &self,
        file_id: Option<FileId>,
        folder_id: Option<FolderId>,
        recipient_user_id: UserId,
        permissions: SharePermissions,
        created_by: UserId,
    ) -> Result<Share, sqlx::Error>;

    async fn update_share_permission(
        &self,
        share_id: ShareId,
        new_permission: SharePermissions,
    ) -> Result<Share, sqlx::Error>;

    async fn get_by_id(&self, share_id: ShareId) -> Result<Option<Share>, sqlx::Error>;

    async fn list_received_shares(
        &self,
        user_id: UserId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Share>, sqlx::Error>;

    async fn list_share_recipients(
        &self,
        file_id: Option<FileId>,
        folder_id: Option<FolderId>,
    ) -> Result<Vec<Share>, sqlx::Error>;

    async fn revoke_share(&self, share_id: ShareId) -> Result<(), sqlx::Error>;
}

/// Trait for user repository operations needed by UserShareService.
#[allow(async_fn_in_trait)]
pub trait UserOps: Send + Sync {
    async fn find_by_email(&self, email: &str) -> Result<Option<User>, sqlx::Error>;
    async fn get_by_id(&self, user_id: UserId) -> Result<Option<User>, sqlx::Error>;
}

/// Trait for file repository operations needed by UserShareService.
#[allow(async_fn_in_trait)]
pub trait FileOps: Send + Sync {
    async fn get_by_id(&self, file_id: FileId) -> Result<Option<File>, sqlx::Error>;
}

/// Trait for folder repository operations needed by UserShareService.
#[allow(async_fn_in_trait)]
pub trait FolderOps: Send + Sync {
    async fn get_by_id(&self, folder_id: FolderId) -> Result<Option<Folder>, sqlx::Error>;
}

pub struct UserShareService<SR, UR, FR, DR, S, F, D, N>
where
    SR: ShareOps,
    UR: UserOps,
    FR: FileOps,
    DR: FolderOps,
    S: crate::services::ShareResolverOps,
    F: crate::services::FileResolverOps,
    D: crate::services::FolderResolverOps,
    N: crate::services::NotificationRepositoryOps,
{
    share_repo: Arc<SR>,
    user_repo: Arc<UR>,
    file_repo: Arc<FR>,
    folder_repo: Arc<DR>,
    permission_resolver: Arc<PermissionResolver<S, F, D>>,
    notification_service: Arc<NotificationService<N>>,
}

impl<SR, UR, FR, DR, S, F, D, N> UserShareService<SR, UR, FR, DR, S, F, D, N>
where
    SR: ShareOps,
    UR: UserOps,
    FR: FileOps,
    DR: FolderOps,
    S: crate::services::ShareResolverOps,
    F: crate::services::FileResolverOps,
    D: crate::services::FolderResolverOps,
    N: crate::services::NotificationRepositoryOps,
{
    pub fn new(
        share_repo: Arc<SR>,
        user_repo: Arc<UR>,
        file_repo: Arc<FR>,
        folder_repo: Arc<DR>,
        permission_resolver: Arc<PermissionResolver<S, F, D>>,
        notification_service: Arc<NotificationService<N>>,
    ) -> Self {
        Self {
            share_repo,
            user_repo,
            file_repo,
            folder_repo,
            permission_resolver,
            notification_service,
        }
    }

    /// Create a share for a file with a specific user.
    pub async fn create_file_share(
        &self,
        file_id: FileId,
        recipient_email: &str,
        permission: SharePermissions,
        created_by: UserId,
    ) -> Result<Share, ShareError> {
        // Verify file exists
        let file = self
            .file_repo
            .get_by_id(file_id)
            .await
            .map_err(ShareError::Database)?
            .ok_or(ShareError::FileNotFound(file_id))?;

        // Verify creator owns the file
        if file.owner_id != created_by {
            return Err(ShareError::PermissionDenied {
                file_id,
                user_id: created_by,
            });
        }

        // Find recipient user by email
        let recipient_email_lower = recipient_email.trim().to_lowercase();
        let recipient = self
            .user_repo
            .find_by_email(&recipient_email_lower)
            .await
            .map_err(ShareError::Database)?
            .ok_or_else(|| ShareError::RecipientNotFound(recipient_email.to_string()))?;

        // Verify not sharing with self
        if recipient.id == created_by {
            return Err(ShareError::CannotShareWithSelf);
        }

        // Check if share already exists - if so, update permission
        if let Some(existing_share) = self
            .share_repo
            .find_user_share(Some(file_id), None, recipient.id)
            .await
            .map_err(ShareError::Database)?
        {
            if existing_share.revoked_at.is_none() {
                // Update existing share permission
                return self
                    .share_repo
                    .update_share_permission(existing_share.id, permission)
                    .await
                    .map_err(ShareError::from);
            }
        }

        // Create new share
        let share = self
            .share_repo
            .create_user_share(Some(file_id), None, recipient.id, permission, created_by)
            .await?;

        // Create notification for recipient (ignore errors - notifications are best-effort)
        let creator = self.user_repo.get_by_id(created_by).await.ok().flatten();
        let creator_email = creator.map(|u| u.email).unwrap_or_else(|| "Someone".to_string());

        let _ = self
            .notification_service
            .create_notification(
                recipient.id,
                crate::domain::NotificationType::ShareReceived,
                "New file shared with you".to_string(),
                format!("{} shared '{}' with you", creator_email, file.name),
                file_id.into(),
                crate::domain::ResourceType::File,
                Some(format!("/files/{}", file_id)),
            )
            .await;

        Ok(share)
    }

    /// Create a share for a folder with a specific user.
    pub async fn create_folder_share(
        &self,
        folder_id: FolderId,
        recipient_email: &str,
        permission: SharePermissions,
        created_by: UserId,
    ) -> Result<Share, ShareError> {
        // Verify folder exists
        let folder = self
            .folder_repo
            .get_by_id(folder_id)
            .await
            .map_err(ShareError::Database)?
            .ok_or_else(|| ShareError::NotFoundById(folder_id))?;

        // Verify creator owns the folder
        if folder.owner_id != created_by {
            return Err(ShareError::PermissionDenied {
                file_id: folder_id, // Reuse error variant (UUID is UUID)
                user_id: created_by,
            });
        }

        // Find recipient user by email
        let recipient_email_lower = recipient_email.trim().to_lowercase();
        let recipient = self
            .user_repo
            .find_by_email(&recipient_email_lower)
            .await
            .map_err(ShareError::Database)?
            .ok_or_else(|| ShareError::RecipientNotFound(recipient_email.to_string()))?;

        // Verify not sharing with self
        if recipient.id == created_by {
            return Err(ShareError::CannotShareWithSelf);
        }

        // Check if share already exists
        if let Some(existing_share) = self
            .share_repo
            .find_user_share(None, Some(folder_id), recipient.id)
            .await
            .map_err(ShareError::Database)?
        {
            if existing_share.revoked_at.is_none() {
                // Update existing share permission
                return self
                    .share_repo
                    .update_share_permission(existing_share.id, permission)
                    .await
                    .map_err(ShareError::from);
            }
        }

        // Create new share
        let share = self
            .share_repo
            .create_user_share(None, Some(folder_id), recipient.id, permission, created_by)
            .await?;

        // Create notification for recipient
        let creator = self.user_repo.get_by_id(created_by).await.ok().flatten();
        let creator_email = creator.map(|u| u.email).unwrap_or_else(|| "Someone".to_string());

        let _ = self
            .notification_service
            .create_notification(
                recipient.id,
                crate::domain::NotificationType::ShareReceived,
                "New folder shared with you".to_string(),
                format!("{} shared folder '{}' with you", creator_email, folder.name),
                folder_id.into(),
                crate::domain::ResourceType::Folder,
                Some(format!("/folders/{}", folder_id)),
            )
            .await;

        Ok(share)
    }

    /// List shares received by a user.
    pub async fn list_received_shares(
        &self,
        user_id: UserId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Share>, ShareError> {
        let shares = self
            .share_repo
            .list_received_shares(user_id, limit, offset)
            .await?;
        Ok(shares)
    }

    /// List recipients of a shared resource (Admin permission required).
    pub async fn list_recipients(
        &mut self,
        file_id: Option<FileId>,
        folder_id: Option<FolderId>,
        requesting_user: UserId,
    ) -> Result<Vec<ShareRecipient>, ShareError> {
        // Determine resource for permission check
        let resource = if let Some(fid) = file_id {
            Resource::File(fid)
        } else if let Some(foid) = folder_id {
            Resource::Folder(foid)
        } else {
            return Err(ShareError::NotFound);
        };

        // Check if requesting user has Admin permission
        let permission = Arc::get_mut(&mut self.permission_resolver)
            .ok_or_else(|| ShareError::Database(sqlx::Error::PoolTimedOut))?
            .resolve_permission(requesting_user, resource)
            .await
            .map_err(|e| ShareError::Database(sqlx::Error::Protocol(e.to_string())))?;

        if permission != Some(SharePermissions::Admin) {
            return Err(ShareError::InsufficientPermission {
                required: SharePermissions::Admin,
                actual: permission.unwrap_or(SharePermissions::View),
            });
        }

        // Get all shares for this resource
        let shares = self
            .share_repo
            .list_share_recipients(file_id, folder_id)
            .await?;

        // Convert to ShareRecipient DTOs
        let mut recipients = Vec::new();
        for share in shares {
            if let Some(recipient_user_id) = share.recipient_user_id {
                // Fetch user email
                if let Some(user) = self.user_repo.get_by_id(recipient_user_id).await.map_err(ShareError::Database)? {
                    recipients.push(ShareRecipient {
                        share_id: share.id,
                        user_id: recipient_user_id,
                        email: user.email,
                        permission: share.permissions,
                        added_at: share.created_at,
                        added_by: share.created_by,
                    });
                }
            }
        }

        Ok(recipients)
    }

    /// Update recipient permission (Admin permission required).
    pub async fn update_recipient_permission(
        &mut self,
        share_id: ShareId,
        new_permission: SharePermissions,
        requesting_user: UserId,
    ) -> Result<Share, ShareError> {
        // Get the share
        let share = self
            .share_repo
            .get_by_id(share_id)
            .await
            .map_err(ShareError::Database)?
            .ok_or(ShareError::NotFoundById(share_id))?;

        // Determine resource for permission check
        let resource = if let Some(fid) = share.file_id {
            Resource::File(fid)
        } else if let Some(foid) = share.folder_id {
            Resource::Folder(foid)
        } else {
            return Err(ShareError::NotFound);
        };

        // Check if requesting user has Admin permission
        let permission = Arc::get_mut(&mut self.permission_resolver)
            .ok_or_else(|| ShareError::Database(sqlx::Error::PoolTimedOut))?
            .resolve_permission(requesting_user, resource)
            .await
            .map_err(|e| ShareError::Database(sqlx::Error::Protocol(e.to_string())))?;

        if permission != Some(SharePermissions::Admin) {
            return Err(ShareError::InsufficientPermission {
                required: SharePermissions::Admin,
                actual: permission.unwrap_or(SharePermissions::View),
            });
        }

        // Store old permission for notification
        let old_permission = share.permissions;

        // Update permission
        let updated_share = self
            .share_repo
            .update_share_permission(share_id, new_permission)
            .await?;

        // Create notification for recipient
        if let Some(recipient_id) = updated_share.recipient_user_id {
            let resource_name = if let Some(fid) = share.file_id {
                self.file_repo
                    .get_by_id(fid)
                    .await
                    .ok()
                    .flatten()
                    .map(|f| f.name)
                    .unwrap_or_else(|| "a file".to_string())
            } else if let Some(foid) = share.folder_id {
                self.folder_repo
                    .get_by_id(foid)
                    .await
                    .ok()
                    .flatten()
                    .map(|f| f.name)
                    .unwrap_or_else(|| "a folder".to_string())
            } else {
                "a resource".to_string()
            };

            let _ = self
                .notification_service
                .create_notification(
                    recipient_id,
                    crate::domain::NotificationType::PermissionChanged,
                    "Share permission updated".to_string(),
                    format!(
                        "Your permission on '{}' changed from {:?} to {:?}",
                        resource_name, old_permission, new_permission
                    ),
                    share.resource_id(),
                    if share.is_file_share() {
                        crate::domain::ResourceType::File
                    } else {
                        crate::domain::ResourceType::Folder
                    },
                    None,
                )
                .await;
        }

        Ok(updated_share)
    }

    /// Remove a recipient from a share (Admin permission required).
    pub async fn remove_recipient(
        &mut self,
        share_id: ShareId,
        requesting_user: UserId,
    ) -> Result<(), ShareError> {
        // Get the share
        let share = self
            .share_repo
            .get_by_id(share_id)
            .await
            .map_err(ShareError::Database)?
            .ok_or(ShareError::NotFoundById(share_id))?;

        // Determine resource for permission check
        let resource = if let Some(fid) = share.file_id {
            Resource::File(fid)
        } else if let Some(foid) = share.folder_id {
            Resource::Folder(foid)
        } else {
            return Err(ShareError::NotFound);
        };

        // Check if requesting user has Admin permission
        let permission = Arc::get_mut(&mut self.permission_resolver)
            .ok_or_else(|| ShareError::Database(sqlx::Error::PoolTimedOut))?
            .resolve_permission(requesting_user, resource)
            .await
            .map_err(|e| ShareError::Database(sqlx::Error::Protocol(e.to_string())))?;

        if permission != Some(SharePermissions::Admin) {
            return Err(ShareError::InsufficientPermission {
                required: SharePermissions::Admin,
                actual: permission.unwrap_or(SharePermissions::View),
            });
        }

        // Cannot remove owner (defensive check)
        if let Some(recipient_id) = share.recipient_user_id {
            // Get resource owner
            let owner_id = if let Some(fid) = share.file_id {
                self.file_repo
                    .get_by_id(fid)
                    .await
                    .map_err(ShareError::Database)?
                    .map(|f| f.owner_id)
            } else if let Some(foid) = share.folder_id {
                self.folder_repo
                    .get_by_id(foid)
                    .await
                    .map_err(ShareError::Database)?
                    .map(|f| f.owner_id)
            } else {
                None
            };

            if let Some(owner_id) = owner_id {
                if recipient_id == owner_id {
                    return Err(ShareError::CannotRemoveOwner);
                }
            }
        }

        // Revoke share (soft delete)
        self.share_repo.revoke_share(share_id).await?;

        // Create notification for recipient
        if let Some(recipient_id) = share.recipient_user_id {
            let resource_name = if let Some(fid) = share.file_id {
                self.file_repo
                    .get_by_id(fid)
                    .await
                    .ok()
                    .flatten()
                    .map(|f| f.name)
                    .unwrap_or_else(|| "a file".to_string())
            } else if let Some(foid) = share.folder_id {
                self.folder_repo
                    .get_by_id(foid)
                    .await
                    .ok()
                    .flatten()
                    .map(|f| f.name)
                    .unwrap_or_else(|| "a folder".to_string())
            } else {
                "a resource".to_string()
            };

            let _ = self
                .notification_service
                .create_notification(
                    recipient_id,
                    crate::domain::NotificationType::ShareRevoked,
                    "Share access revoked".to_string(),
                    format!("Your access to '{}' was revoked", resource_name),
                    share.resource_id(),
                    if share.is_file_share() {
                        crate::domain::ResourceType::File
                    } else {
                        crate::domain::ResourceType::Folder
                    },
                    None,
                )
                .await;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Full tests require database setup
    // Integration tests in server/tests/integration/
}
