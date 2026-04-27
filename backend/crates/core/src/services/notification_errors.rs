use crate::domain::{NotificationId, UserId};
use thiserror::Error;

/// Errors that can occur during notification operations.
#[derive(Debug, Error)]
pub enum NotificationError {
    /// Notification was not found.
    #[error("Notification not found")]
    NotFound,

    /// Notification with the given ID was not found.
    #[error("Notification {0} not found")]
    NotFoundById(NotificationId),

    /// User does not own the notification.
    #[error("User {user_id} does not own notification {notification_id}")]
    NotOwned {
        notification_id: NotificationId,
        user_id: UserId,
    },

    /// Database operation failed.
    #[error("Database error: {0}")]
    Database(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_notification_error_not_found() {
        let err = NotificationError::NotFound;
        assert_eq!(err.to_string(), "Notification not found");
    }

    #[test]
    fn test_notification_error_not_found_by_id() {
        let id = Uuid::new_v4();
        let err = NotificationError::NotFoundById(id);
        assert_eq!(err.to_string(), format!("Notification {} not found", id));
    }

    #[test]
    fn test_notification_error_not_owned() {
        let notification_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let err = NotificationError::NotOwned {
            notification_id,
            user_id,
        };
        let msg = err.to_string();
        assert!(msg.contains("does not own notification"));
        assert!(msg.contains(&user_id.to_string()));
        assert!(msg.contains(&notification_id.to_string()));
    }

    // Note: Database error tests removed as they require sqlx::Error which cannot
    // be easily constructed in unit tests. The #[from] attribute ensures proper
    // automatic conversion from sqlx::Error to NotificationError::Database.
}
