//! DEPRECATED: Use ShareService instead
//! 
//! This module is being phased out in favor of the unified ShareService.
//! New code should use ShareService for all share operations.
//! 
//! Migration guide:
//! - `user_share_service.create_file_share(file_id, email, perm, user)` 
//!   → `share_service.create_user_share(Resource::File(file_id), email, perm, user)`
//! - `user_share_service.create_folder_share(folder_id, email, perm, user)`
//!   → `share_service.create_user_share(Resource::Folder(folder_id), email, perm, user)`

use anyhow::Result;
use std::sync::Arc;
use tracing::{error, warn};

use crate::domain::{
    File, FileId, Folder, FolderId, Share, ShareId, SharePermissions, ShareRecipient, User, UserId,
};
use crate::events::{
    AggregateType, Event, EventBroadcaster, EventType, NotificationCreatedPayload,
};
use crate::services::{
    permission_resolver::PermissionResolverOps, CreateNotification, NotificationService,
    PermissionResolver, Resource, ShareError,
};

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
        tenant_id: uuid::Uuid,
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
    async fn get_tenant_id_for_user(&self, user_id: UserId) -> Result<Option<uuid::Uuid>, sqlx::Error>;
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

/// DEPRECATED: Use ShareService instead
#[deprecated(since = "0.2.0", note = "Use ShareService instead. See module documentation for migration guide.")]
pub struct UserShareService<SR, UR, FR, DR, P, N, E>
where
    SR: ShareOps,
    UR: UserOps,
    FR: FileOps,
    DR: FolderOps,
    P: PermissionResolverOps,
    N: crate::services::NotificationRepositoryOps,
    E: crate::services::ShareEventStoreOps,
{
    share_repo: Arc<SR>,
    user_repo: Arc<UR>,
    file_repo: Arc<FR>,
    folder_repo: Arc<DR>,
    permission_resolver: Arc<PermissionResolver<P>>,
    notification_service: Arc<NotificationService<N>>,
    event_store: Arc<E>,
    broadcaster: Arc<EventBroadcaster>,
}

pub struct UserShareServiceDeps<SR, UR, FR, DR, P, N, E>
where
    SR: ShareOps,
    UR: UserOps,
    FR: FileOps,
    DR: FolderOps,
    P: PermissionResolverOps,
    N: crate::services::NotificationRepositoryOps,
    E: crate::services::ShareEventStoreOps,
{
    pub share_repo: Arc<SR>,
    pub user_repo: Arc<UR>,
    pub file_repo: Arc<FR>,
    pub folder_repo: Arc<DR>,
    pub permission_resolver: Arc<PermissionResolver<P>>,
    pub notification_service: Arc<NotificationService<N>>,
    pub event_store: Arc<E>,
    pub broadcaster: Arc<EventBroadcaster>,
}

impl<SR, UR, FR, DR, P, N, E> UserShareService<SR, UR, FR, DR, P, N, E>
where
    SR: ShareOps,
    UR: UserOps,
    FR: FileOps,
    DR: FolderOps,
    P: PermissionResolverOps,
    N: crate::services::NotificationRepositoryOps,
    E: crate::services::ShareEventStoreOps,
{
    fn shared_resource_action_url(
        resource_type: crate::domain::ResourceType,
        resource_id: uuid::Uuid,
    ) -> String {
        let resource_path = match resource_type {
            crate::domain::ResourceType::File => "file",
            crate::domain::ResourceType::Folder => "folder",
            crate::domain::ResourceType::Share => "file",
        };

        format!("/shared-with-me/{resource_path}/{resource_id}")
    }

    /// DEPRECATED: Use ShareService::new instead
    #[deprecated(since = "0.2.0", note = "Use ShareService::new instead")]
    pub fn new(deps: UserShareServiceDeps<SR, UR, FR, DR, P, N, E>) -> Self {
        Self {
            share_repo: deps.share_repo,
            user_repo: deps.user_repo,
            file_repo: deps.file_repo,
            folder_repo: deps.folder_repo,
            permission_resolver: deps.permission_resolver,
            notification_service: deps.notification_service,
            event_store: deps.event_store,
            broadcaster: deps.broadcaster,
        }
    }

    async fn emit_notification_created_event(&self, notification: &crate::domain::Notification) {
        let notification_type = serde_json::to_string(&notification.notification_type)
            .unwrap_or_else(|_| "\"notification\"".to_string())
            .trim_matches('"')
            .to_string();

        let payload = NotificationCreatedPayload {
            notification_id: notification.id,
            user_id: notification.user_id,
            title: notification.title.clone(),
            message: notification.message.clone(),
            notification_type: notification_type.clone(),
            resource_id: notification.resource_id,
            resource_type: serde_json::to_string(&notification.resource_type)
                .unwrap_or_else(|_| "\"resource\"".to_string())
                .trim_matches('"')
                .to_string(),
            action_url: notification.action_url.clone(),
            timestamp: notification.created_at,
        };

        let event = Event::new(
            EventType::NotificationCreated,
            notification.user_id,
            AggregateType::User,
            serde_json::to_value(&payload).unwrap_or(serde_json::json!({
                "notification_id": notification.id,
                "user_id": notification.user_id,
                "title": notification.title.clone(),
                "message": notification.message.clone(),
                "notification_type": notification_type,
                "resource_id": notification.resource_id,
                "resource_type": serde_json::to_string(&notification.resource_type)
                    .unwrap_or_else(|_| "\"resource\"".to_string())
                    .trim_matches('"')
                    .to_string(),
                "action_url": notification.action_url.clone(),
                "timestamp": notification.created_at,
            })),
            notification.user_id,
        );

        if let Err(error) = self.event_store.append(&event, &self.broadcaster).await {
            warn!(
                notification_id = %notification.id,
                user_id = %notification.user_id,
                "failed to append notification event: {error}"
            );
        }
    }

    /// DEPRECATED: Use ShareService::create_user_share with Resource::File instead
    #[deprecated(since = "0.2.0", note = "Use ShareService::create_user_share with Resource::File instead")]
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

        // Get creator's tenant ID
        let creator_tenant_id = self
            .user_repo
            .get_tenant_id_for_user(created_by)
            .await
            .map_err(ShareError::Database)?;

        // Find recipient user by email
        let recipient_email_lower = recipient_email.trim().to_lowercase();
        let recipient = self
            .user_repo
            .find_by_email(&recipient_email_lower)
            .await
            .map_err(ShareError::Database)?
            .ok_or_else(|| ShareError::RecipientNotFound(recipient_email.to_string()))?;

        // Verify recipient is in the same tenant as the creator
        if Some(recipient.tenant_id) != creator_tenant_id {
            return Err(ShareError::RecipientNotFound(recipient_email.to_string()));
        }

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
                    .map_err(|error| {
                        error!(
                            share_id = %existing_share.id,
                            file_id = %file_id,
                            recipient_user_id = %recipient.id,
                            "failed to update existing file share: {error}"
                        );
                        error
                    })
                    .map_err(ShareError::from);
            }
        }

        // Create new share
        let share = self
            .share_repo
            .create_user_share(
                Some(file_id),
                None,
                recipient.id,
                permission,
                created_by,
                creator_tenant_id.unwrap_or_else(|| uuid::Uuid::nil()),
            )
            .await
            .map_err(|error| {
                error!(
                    file_id = %file_id,
                    recipient_user_id = %recipient.id,
                    created_by = %created_by,
                    "failed to create file share: {error}"
                );
                error
            })?;

        // Create notification for recipient (ignore errors - notifications are best-effort)
        let creator = self.user_repo.get_by_id(created_by).await.ok().flatten();
        let creator_email = creator
            .map(|u| u.email)
            .unwrap_or_else(|| "Someone".to_string());

        if let Ok(notification) = self
            .notification_service
            .create_notification(CreateNotification {
                user_id: recipient.id,
                notification_type: crate::domain::NotificationType::ShareReceived,
                title: "New file shared with you".to_string(),
                message: format!("{} shared '{}' with you", creator_email, file.name),
                resource_id: file_id,
                resource_type: crate::domain::ResourceType::File,
                action_url: Some(Self::shared_resource_action_url(
                    crate::domain::ResourceType::File,
                    file_id,
                )),
                tenant_id: share.tenant_id,
            })
            .await
        {
            self.emit_notification_created_event(&notification).await;
        }

        Ok(share)
    }

    /// DEPRECATED: Use ShareService::create_user_share with Resource::Folder instead
    #[deprecated(since = "0.2.0", note = "Use ShareService::create_user_share with Resource::Folder instead")]
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
            .ok_or_else(|| ShareError::FolderNotFound(folder_id))?;

        // Verify creator owns the folder
        if folder.owner_id != created_by {
            return Err(ShareError::PermissionDenied {
                file_id: folder_id, // Reuse error variant (UUID is UUID)
                user_id: created_by,
            });
        }

        // Get creator's tenant ID
        let creator_tenant_id = self
            .user_repo
            .get_tenant_id_for_user(created_by)
            .await
            .map_err(ShareError::Database)?;

        // Find recipient user by email
        let recipient_email_lower = recipient_email.trim().to_lowercase();
        let recipient = self
            .user_repo
            .find_by_email(&recipient_email_lower)
            .await
            .map_err(ShareError::Database)?
            .ok_or_else(|| ShareError::RecipientNotFound(recipient_email.to_string()))?;

        // Verify recipient is in the same tenant as the creator
        if Some(recipient.tenant_id) != creator_tenant_id {
            return Err(ShareError::RecipientNotFound(recipient_email.to_string()));
        }

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
                    .map_err(|error| {
                        error!(
                            share_id = %existing_share.id,
                            folder_id = %folder_id,
                            recipient_user_id = %recipient.id,
                            "failed to update existing folder share: {error}"
                        );
                        error
                    })
                    .map_err(ShareError::from);
            }
        }

        // Create new share
        let share = self
            .share_repo
            .create_user_share(
                None,
                Some(folder_id),
                recipient.id,
                permission,
                created_by,
                creator_tenant_id.unwrap_or_else(|| uuid::Uuid::nil()),
            )
            .await
            .map_err(|error| {
                error!(
                    folder_id = %folder_id,
                    recipient_user_id = %recipient.id,
                    created_by = %created_by,
                    "failed to create folder share: {error}"
                );
                error
            })?;

        // Create notification for recipient
        let creator = self.user_repo.get_by_id(created_by).await.ok().flatten();
        let creator_email = creator
            .map(|u| u.email)
            .unwrap_or_else(|| "Someone".to_string());

        if let Ok(notification) = self
            .notification_service
            .create_notification(CreateNotification {
                user_id: recipient.id,
                notification_type: crate::domain::NotificationType::ShareReceived,
                title: "New folder shared with you".to_string(),
                message: format!("{} shared folder '{}' with you", creator_email, folder.name),
                resource_id: folder_id,
                resource_type: crate::domain::ResourceType::Folder,
                action_url: Some(Self::shared_resource_action_url(
                    crate::domain::ResourceType::Folder,
                    folder_id,
                )),
                tenant_id: share.tenant_id,
            })
            .await
        {
            self.emit_notification_created_event(&notification).await;
        }

        Ok(share)
    }

    /// DEPRECATED: Use ShareService::list_received_shares instead
    #[deprecated(since = "0.2.0", note = "Use ShareService::list_received_shares instead")]
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

    /// DEPRECATED: Use ShareService::list_recipients instead
    #[deprecated(since = "0.2.0", note = "Use ShareService::list_recipients instead")]
    /// List recipients of a shared resource (Admin permission required).
    pub async fn list_recipients(
        &self,
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
            return Err(ShareError::InvalidState("Share has neither file_id nor folder_id".to_string()));
        };

        // Check if requesting user has Admin permission
        // Owners implicitly have Admin permission via ownership check in permission_resolver
        let permission = match self.permission_resolver.resolve_permission(requesting_user, resource).await {
            Ok(Some(perm)) => perm,
            Ok(None) => {
                return Err(ShareError::InsufficientPermission {
                    required: SharePermissions::Admin,
                    actual: SharePermissions::View,
                });
            }
            Err(e) => {
                return Err(ShareError::Database(sqlx::Error::Protocol(e.to_string())));
            }
        };

        if permission != SharePermissions::Admin {
            return Err(ShareError::InsufficientPermission {
                required: SharePermissions::Admin,
                actual: permission,
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
                if let Some(user) = self
                    .user_repo
                    .get_by_id(recipient_user_id)
                    .await
                    .map_err(ShareError::Database)?
                {
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

    /// DEPRECATED: Use ShareService::update_recipient_permission instead
    #[deprecated(since = "0.2.0", note = "Use ShareService::update_recipient_permission instead")]
    /// Update recipient permission (Admin permission required).
    pub async fn update_recipient_permission(
        &self,
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
            .ok_or(ShareError::ShareNotFound(share_id))?;

        // Determine resource for permission check
        let resource = if let Some(fid) = share.file_id {
            Resource::File(fid)
        } else if let Some(foid) = share.folder_id {
            Resource::Folder(foid)
        } else {
            return Err(ShareError::InvalidState("Share has neither file_id nor folder_id".to_string()));
        };

        // Check if requesting user has Admin permission
        let permission = self
            .permission_resolver
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

            if let Ok(notification) = self
                .notification_service
                .create_notification(CreateNotification {
                    user_id: recipient_id,
                    notification_type: crate::domain::NotificationType::PermissionChanged,
                    title: "Share permission updated".to_string(),
                    message: format!(
                        "Your permission on '{}' changed from {:?} to {:?}",
                        resource_name, old_permission, new_permission
                    ),
                    resource_id: share.resource_id().unwrap_or(share.id),
                    resource_type: if share.is_file_share() {
                        crate::domain::ResourceType::File
                    } else {
                        crate::domain::ResourceType::Folder
                    },
                    action_url: Some(Self::shared_resource_action_url(
                        if share.is_file_share() {
                            crate::domain::ResourceType::File
                        } else {
                            crate::domain::ResourceType::Folder
                        },
                        share.resource_id().unwrap_or(share.id),
                    )),
                    tenant_id: updated_share.tenant_id,
                })
                .await
            {
                self.emit_notification_created_event(&notification).await;
            }
        }

        Ok(updated_share)
    }

    /// DEPRECATED: Use ShareService::remove_recipient instead
    #[deprecated(since = "0.2.0", note = "Use ShareService::remove_recipient instead")]
    /// Remove a recipient from a share (Admin permission required).
    pub async fn remove_recipient(
        &self,
        share_id: ShareId,
        requesting_user: UserId,
    ) -> Result<(), ShareError> {
        // Get the share
        let share = self
            .share_repo
            .get_by_id(share_id)
            .await
            .map_err(ShareError::Database)?
            .ok_or(ShareError::ShareNotFound(share_id))?;

        // Determine resource for permission check
        let resource = if let Some(fid) = share.file_id {
            Resource::File(fid)
        } else if let Some(foid) = share.folder_id {
            Resource::Folder(foid)
        } else {
            return Err(ShareError::InvalidState("Share has neither file_id nor folder_id".to_string()));
        };

        // Check if requesting user has Admin permission
        let permission = self
            .permission_resolver
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

            if let Ok(notification) = self
                .notification_service
                .create_notification(CreateNotification {
                    user_id: recipient_id,
                    notification_type: crate::domain::NotificationType::ShareRevoked,
                    title: "Share access revoked".to_string(),
                    message: format!("Your access to '{}' was revoked", resource_name),
                    resource_id: share.resource_id().unwrap_or(share.id),
                    resource_type: if share.is_file_share() {
                        crate::domain::ResourceType::File
                    } else {
                        crate::domain::ResourceType::Folder
                    },
                    action_url: Some("/shared-with-me".to_string()),
                    tenant_id: share.tenant_id,
                })
                .await
            {
                self.emit_notification_created_event(&notification).await;
            }
        }

        Ok(())
    }
}
