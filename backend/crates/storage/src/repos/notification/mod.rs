//! Notification repository for zero-PostgreSQL notification management

use async_trait::async_trait;
use rustshare_core::domain::Notification;
use thiserror::Error;
use uuid::Uuid;

pub mod projector;
pub mod rustfs;

pub use projector::NotificationProjector;
pub use rustfs::RustFsNotificationRepository;

/// Errors that can occur in notification repository operations
#[derive(Debug, Error)]
pub enum NotificationRepositoryError {
    #[error("Notification not found: {0}")]
    NotFound(Uuid),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Concurrency conflict")]
    Conflict,
}

/// Query options for listing notifications
#[derive(Debug, Clone, Default)]
pub struct NotificationQuery {
    /// Filter by read status
    pub read: Option<bool>,
    /// Limit results
    pub limit: Option<usize>,
    /// Offset for pagination
    pub offset: Option<usize>,
}

/// Notification repository trait
#[async_trait]
pub trait NotificationRepository: Send + Sync {
    /// Create a new notification
    async fn create_notification(
        &self,
        notification: &Notification,
    ) -> Result<(), NotificationRepositoryError>;

    /// Get notification by ID
    async fn get_notification(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Notification>, NotificationRepositoryError>;

    /// Get notifications for a user
    async fn get_user_notifications(
        &self,
        user_id: Uuid,
        query: NotificationQuery,
    ) -> Result<Vec<Notification>, NotificationRepositoryError>;

    /// Count unread notifications for a user
    async fn count_unread(&self, user_id: Uuid) -> Result<u32, NotificationRepositoryError>;

    /// Mark a notification as read
    async fn mark_read(&self, user_id: Uuid, id: Uuid) -> Result<(), NotificationRepositoryError>;

    /// Mark all notifications as read for a user
    async fn mark_all_read(&self, user_id: Uuid) -> Result<u32, NotificationRepositoryError>;

    /// Delete a notification
    async fn delete_notification(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> Result<(), NotificationRepositoryError>;

    /// Delete all notifications for a user
    async fn delete_all_for_user(&self, user_id: Uuid) -> Result<u32, NotificationRepositoryError>;
}

/// Converts between domain Notification and NotificationDocument
pub mod conversions {
    use super::*;
    use crate::metadata_v2::schemas::{NotificationDocument, NotificationType};
    use rustshare_core::domain::{
        NotificationType as CoreNotificationType, ResourceType as CoreResourceType,
    };

    /// Convert NotificationType from core domain
    fn from_core_type(ty: CoreNotificationType) -> NotificationType {
        match ty {
            CoreNotificationType::ShareReceived => NotificationType::FileShared,
            CoreNotificationType::PermissionChanged => NotificationType::AccessGranted,
            CoreNotificationType::ShareRevoked => NotificationType::ShareRevoked,
        }
    }

    /// Convert NotificationType to core domain
    fn to_core_type(ty: NotificationType) -> CoreNotificationType {
        match ty {
            NotificationType::FileShared | NotificationType::FolderShared => {
                CoreNotificationType::ShareReceived
            }
            NotificationType::ShareRevoked => CoreNotificationType::ShareRevoked,
            NotificationType::FileModified => CoreNotificationType::PermissionChanged,
            NotificationType::AccessRequested | NotificationType::AccessGranted => {
                CoreNotificationType::PermissionChanged
            }
        }
    }

    /// Convert ResourceType to core domain
    fn to_core_resource_type(ty: &str) -> CoreResourceType {
        ty.parse().unwrap_or(CoreResourceType::File)
    }

    /// Convert NotificationDocument to domain Notification
    pub fn doc_to_notification(doc: NotificationDocument) -> Notification {
        Notification {
            id: doc.id,
            user_id: doc.user_id,
            notification_type: to_core_type(doc.notification_type),
            resource_type: to_core_resource_type(&doc.resource_type),
            resource_id: doc.resource_id,
            title: doc.title.clone(),
            message: doc.message.clone(),
            action_url: None, // Not stored in document
            read: doc.read,
            created_at: doc.created_at,
            tenant_id: doc.tenant_id,
        }
    }

    /// Convert domain Notification to NotificationDocument
    pub fn notification_to_doc(notification: &Notification) -> NotificationDocument {
        NotificationDocument {
            schema_version: crate::metadata_v2::schemas::CURRENT_SCHEMA_VERSION,
            id: notification.id,
            user_id: notification.user_id,
            event_id: Uuid::nil(), // Not tracked in domain Notification
            resource_type: notification.resource_type.to_string(),
            resource_id: notification.resource_id,
            notification_type: from_core_type(notification.notification_type),
            title: notification.title.clone(),
            message: notification.message.clone(),
            read: notification.read,
            read_at: None, // Will be set when marked read
            created_at: notification.created_at,
            tenant_id: notification.tenant_id,
        }
    }
}
