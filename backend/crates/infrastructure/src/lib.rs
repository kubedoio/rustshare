pub mod repositories;

// Implement service layer traits for repository types
use anyhow::Result;
use rustshare_core::services::{
    ShareResolverOps, FileResolverOps, FolderResolverOps, NotificationRepositoryOps,
};
use rustshare_core::domain::{File, Folder, Notification, NotificationId, NotificationType, ResourceType, Share, UserId, FileId, FolderId};

use crate::repositories::{ShareRepository, FileRepository, FolderRepository, NotificationRepository};

// ShareRepository implements ShareResolverOps
impl ShareResolverOps for ShareRepository {
    async fn find_user_share(
        &self,
        file_id: Option<FileId>,
        folder_id: Option<FolderId>,
        recipient_user_id: UserId,
    ) -> Result<Option<Share>> {
        self.find_user_share(file_id, folder_id, recipient_user_id)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }
}

// FileRepository implements FileResolverOps
impl FileResolverOps for FileRepository {
    async fn find_file_by_id(&self, id: FileId) -> Result<Option<File>> {
        self.get_by_id(id)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }
}

// FolderRepository implements FolderResolverOps
impl FolderResolverOps for FolderRepository {
    async fn find_folder_by_id(&self, id: FolderId) -> Result<Option<Folder>> {
        self.get_by_id(id)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }
}

// NotificationRepository implements NotificationRepositoryOps
impl NotificationRepositoryOps for NotificationRepository {
    async fn create(
        &self,
        user_id: UserId,
        notification_type: NotificationType,
        title: String,
        message: String,
        resource_id: uuid::Uuid,
        resource_type: ResourceType,
        action_url: Option<String>,
    ) -> Result<Notification, sqlx::Error> {
        self.create(user_id, notification_type, title, message, resource_id, resource_type, action_url).await
    }

    async fn find_by_id(&self, notification_id: NotificationId) -> Result<Option<Notification>, sqlx::Error> {
        self.find_by_id(notification_id).await
    }

    async fn list_for_user(
        &self,
        user_id: UserId,
        unread_only: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Notification>, sqlx::Error> {
        self.list_for_user(user_id, unread_only, limit, offset).await
    }

    async fn count_unread(&self, user_id: UserId) -> Result<i64, sqlx::Error> {
        self.count_unread(user_id).await
    }

    async fn mark_as_read(&self, notification_id: NotificationId) -> Result<Notification, sqlx::Error> {
        self.mark_as_read(notification_id).await
    }

    async fn delete(&self, notification_id: NotificationId) -> Result<(), sqlx::Error> {
        self.delete(notification_id).await
    }
}
