use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::UserId;

pub type NotificationId = Uuid;

/// Type of notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
#[serde(rename_all = "snake_case")]
pub enum NotificationType {
    ShareReceived,
    PermissionChanged,
    ShareRevoked,
}

/// Type of resource referenced by notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    File,
    Folder,
    Share,
}

/// In-app notification for a user.
///
/// Notifications are persistent and stored in the database. They complement
/// real-time WebSocket notifications for offline users.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Notification {
    pub id: NotificationId,
    pub user_id: UserId,
    pub notification_type: NotificationType,
    pub title: String,
    pub message: String,
    /// Polymorphic reference to resource (file/folder/share)
    pub resource_id: Uuid,
    pub resource_type: ResourceType,
    /// Optional deep link to the resource
    pub action_url: Option<String>,
    pub read: bool,
    pub created_at: DateTime<Utc>,
}

impl Notification {
    /// Create a new notification.
    pub fn new(
        user_id: UserId,
        notification_type: NotificationType,
        title: String,
        message: String,
        resource_id: Uuid,
        resource_type: ResourceType,
        action_url: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            notification_type,
            title,
            message,
            resource_id,
            resource_type,
            action_url,
            read: false,
            created_at: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_creation() {
        let user_id = Uuid::new_v4();
        let resource_id = Uuid::new_v4();

        let notification = Notification {
            id: Uuid::new_v4(),
            user_id,
            notification_type: NotificationType::ShareReceived,
            title: "File shared".to_string(),
            message: "Alice shared file.pdf with you".to_string(),
            resource_id,
            resource_type: ResourceType::File,
            action_url: Some("/files/123".to_string()),
            read: false,
            created_at: Utc::now(),
        };

        assert_eq!(notification.user_id, user_id);
        assert!(!notification.read);
        assert_eq!(notification.notification_type, NotificationType::ShareReceived);
    }

    #[test]
    fn test_notification_type_serialization() {
        let json = serde_json::to_string(&NotificationType::ShareReceived).unwrap();
        assert_eq!(json, r#""share_received""#);

        let json = serde_json::to_string(&NotificationType::PermissionChanged).unwrap();
        assert_eq!(json, r#""permission_changed""#);
    }

    #[test]
    fn test_resource_type_serialization() {
        let json = serde_json::to_string(&ResourceType::File).unwrap();
        assert_eq!(json, r#""file""#);

        let json = serde_json::to_string(&ResourceType::Folder).unwrap();
        assert_eq!(json, r#""folder""#);
    }
}

