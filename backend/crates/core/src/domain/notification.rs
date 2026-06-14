use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::UserId;

pub type NotificationId = Uuid;

/// Type of notification.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, utoipa::ToSchema,
)]
#[sqlx(type_name = "TEXT")]
#[serde(rename_all = "snake_case")]
pub enum NotificationType {
    ShareReceived,
    PermissionChanged,
    ShareRevoked,
}

impl std::fmt::Display for NotificationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotificationType::ShareReceived => write!(f, "share_received"),
            NotificationType::PermissionChanged => write!(f, "permission_changed"),
            NotificationType::ShareRevoked => write!(f, "share_revoked"),
        }
    }
}

impl std::str::FromStr for NotificationType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = s.trim().to_lowercase().replace(['_', '-'], "");

        match normalized.as_str() {
            "sharereceived" => Ok(NotificationType::ShareReceived),
            "permissionchanged" => Ok(NotificationType::PermissionChanged),
            "sharerevoked" => Ok(NotificationType::ShareRevoked),
            _ => Err(format!("Invalid notification type: {}", s)),
        }
    }
}

/// Type of resource referenced by notification.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, utoipa::ToSchema,
)]
#[sqlx(type_name = "TEXT")]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    File,
    Folder,
    Share,
}

impl std::fmt::Display for ResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceType::File => write!(f, "file"),
            ResourceType::Folder => write!(f, "folder"),
            ResourceType::Share => write!(f, "share"),
        }
    }
}

impl std::str::FromStr for ResourceType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "file" => Ok(ResourceType::File),
            "folder" => Ok(ResourceType::Folder),
            "share" => Ok(ResourceType::Share),
            _ => Err(format!("Invalid resource type: {}", s)),
        }
    }
}

/// In-app notification for a user.
///
/// Notifications are persistent and stored in the database. They complement
/// real-time WebSocket notifications for offline users.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Notification {
    #[schema(value_type = Uuid)]
    pub id: NotificationId,
    #[schema(value_type = Uuid)]
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
    pub tenant_id: Uuid,
}

impl Notification {
    /// Create a new notification.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        user_id: UserId,
        notification_type: NotificationType,
        title: String,
        message: String,
        resource_id: Uuid,
        resource_type: ResourceType,
        action_url: Option<String>,
        tenant_id: Uuid,
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
            tenant_id,
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
            tenant_id: Uuid::nil(),
        };

        assert_eq!(notification.user_id, user_id);
        assert!(!notification.read);
        assert_eq!(
            notification.notification_type,
            NotificationType::ShareReceived
        );
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

    #[test]
    fn test_notification_type_parsing() {
        assert_eq!(
            "share_received".parse::<NotificationType>().unwrap(),
            NotificationType::ShareReceived
        );
        assert_eq!(
            "ShareReceived".parse::<NotificationType>().unwrap(),
            NotificationType::ShareReceived
        );
        assert_eq!(
            "permission_changed".parse::<NotificationType>().unwrap(),
            NotificationType::PermissionChanged
        );
        assert_eq!(
            "PermissionChanged".parse::<NotificationType>().unwrap(),
            NotificationType::PermissionChanged
        );
        assert!("invalid".parse::<NotificationType>().is_err());
    }

    #[test]
    fn test_resource_type_parsing() {
        assert_eq!("file".parse::<ResourceType>().unwrap(), ResourceType::File);
        assert_eq!("File".parse::<ResourceType>().unwrap(), ResourceType::File);
        assert_eq!(
            "folder".parse::<ResourceType>().unwrap(),
            ResourceType::Folder
        );
        assert!("invalid".parse::<ResourceType>().is_err());
    }
}
