pub mod repositories;

// Implement service layer traits for repository types
use anyhow::Result;
use rustshare_core::domain::{Notification, NotificationId, UserId};
use rustshare_core::services::{CreateNotification, NotificationError, NotificationRepositoryOps};

use crate::repositories::NotificationRepository;

// NotificationRepository implements NotificationRepositoryOps
impl NotificationRepositoryOps for NotificationRepository {
    async fn create(&self, request: CreateNotification) -> Result<Notification, NotificationError> {
        self.create(request).await
    }

    async fn find_by_id(
        &self,
        notification_id: NotificationId,
        tenant_id: uuid::Uuid,
    ) -> Result<Option<Notification>, NotificationError> {
        self.find_by_id(notification_id, tenant_id).await
    }

    async fn list_for_user(
        &self,
        user_id: UserId,
        tenant_id: uuid::Uuid,
        unread_only: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Notification>, NotificationError> {
        self.list_for_user(user_id, tenant_id, unread_only, limit, offset)
            .await
    }

    async fn count_for_user(
        &self,
        user_id: UserId,
        tenant_id: uuid::Uuid,
        unread_only: bool,
    ) -> Result<i64, NotificationError> {
        self.count_for_user(user_id, tenant_id, unread_only).await
    }

    async fn count_unread(
        &self,
        user_id: UserId,
        tenant_id: uuid::Uuid,
    ) -> Result<i64, NotificationError> {
        self.count_unread(user_id, tenant_id).await
    }

    async fn mark_as_read(
        &self,
        notification_id: NotificationId,
        tenant_id: uuid::Uuid,
    ) -> Result<Notification, NotificationError> {
        self.mark_as_read(notification_id, tenant_id).await
    }

    async fn delete(
        &self,
        notification_id: NotificationId,
        tenant_id: uuid::Uuid,
    ) -> Result<(), NotificationError> {
        self.delete(notification_id, tenant_id).await
    }
}
