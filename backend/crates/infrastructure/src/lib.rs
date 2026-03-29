pub mod repositories;

// Implement service layer traits for repository types
use anyhow::Result;
use rustshare_core::domain::{
    File, FileId, Folder, FolderId, Notification, NotificationId, Share, UserId,
};
use rustshare_core::services::{
    CreateNotification, NotificationRepositoryOps,
};

use crate::repositories::{
    NotificationRepository,
};

// NotificationRepository implements NotificationRepositoryOps
impl NotificationRepositoryOps for NotificationRepository {
    async fn create(&self, request: CreateNotification) -> Result<Notification, sqlx::Error> {
        self.create(request).await
    }

    async fn find_by_id(
        &self,
        notification_id: NotificationId,
    ) -> Result<Option<Notification>, sqlx::Error> {
        self.find_by_id(notification_id).await
    }

    async fn list_for_user(
        &self,
        user_id: UserId,
        unread_only: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Notification>, sqlx::Error> {
        self.list_for_user(user_id, unread_only, limit, offset)
            .await
    }

    async fn count_for_user(&self, user_id: UserId, unread_only: bool) -> Result<i64, sqlx::Error> {
        self.count_for_user(user_id, unread_only).await
    }

    async fn count_unread(&self, user_id: UserId) -> Result<i64, sqlx::Error> {
        self.count_unread(user_id).await
    }

    async fn mark_as_read(
        &self,
        notification_id: NotificationId,
    ) -> Result<Notification, sqlx::Error> {
        self.mark_as_read(notification_id).await
    }

    async fn delete(&self, notification_id: NotificationId) -> Result<(), sqlx::Error> {
        self.delete(notification_id).await
    }
}
