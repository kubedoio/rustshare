//! NotificationService for notification management operations.
//!
//! This service handles persistent notification operations, including:
//! - Creating notifications
//! - Retrieving notifications by ID
//! - Listing notifications for a user (with pagination and filtering)
//! - Counting unread notifications
//! - Marking notifications as read
//! - Deleting notifications
//!
//! All update and delete operations include ownership validation to ensure
//! users can only modify their own notifications.

use crate::domain::{Notification, NotificationId, NotificationType, ResourceType, UserId};
use crate::services::NotificationError;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct CreateNotification {
    pub user_id: UserId,
    pub notification_type: NotificationType,
    pub title: String,
    pub message: String,
    pub resource_id: Uuid,
    pub resource_type: ResourceType,
    pub action_url: Option<String>,
    pub tenant_id: Uuid,
}

/// Trait for notification repository operations needed by NotificationService.
///
/// This trait abstracts the repository to allow for testing without database dependencies.
#[allow(async_fn_in_trait)]
pub trait NotificationRepositoryOps: Send + Sync {
    /// Create a new notification.
    async fn create(&self, request: CreateNotification) -> Result<Notification, sqlx::Error>;

    /// Find a notification by ID.
    async fn find_by_id(
        &self,
        notification_id: NotificationId,
    ) -> Result<Option<Notification>, sqlx::Error>;

    /// List notifications for a user (paginated, optional unread filter).
    async fn list_for_user(
        &self,
        user_id: UserId,
        unread_only: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Notification>, sqlx::Error>;

    /// Count notifications for a user with optional unread filtering.
    async fn count_for_user(&self, user_id: UserId, unread_only: bool) -> Result<i64, sqlx::Error>;

    /// Count unread notifications for a user.
    async fn count_unread(&self, user_id: UserId) -> Result<i64, sqlx::Error>;

    /// Mark a notification as read.
    async fn mark_as_read(
        &self,
        notification_id: NotificationId,
    ) -> Result<Notification, sqlx::Error>;

    /// Delete a notification.
    async fn delete(&self, notification_id: NotificationId) -> Result<(), sqlx::Error>;
}

/// NotificationService handles persistent notification operations.
///
/// Generic over NotificationRepository implementation to support
/// different backends and testing with mock implementations.
pub struct NotificationService<R: NotificationRepositoryOps> {
    repository: R,
}

impl<R: NotificationRepositoryOps> NotificationService<R> {
    /// Create a new NotificationService instance.
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    /// Create a new notification for a user.
    ///
    /// Returns the created Notification or a NotificationError.
    pub async fn create_notification(
        &self,
        request: CreateNotification,
    ) -> Result<Notification, NotificationError> {
        self.repository
            .create(request)
            .await
            .map_err(NotificationError::Database)
    }

    /// Get a notification by ID with ownership validation.
    ///
    /// Verifies that the user owns the notification.
    /// Returns the Notification or a NotificationError.
    pub async fn get_notification(
        &self,
        notification_id: NotificationId,
        user_id: UserId,
    ) -> Result<Notification, NotificationError> {
        let notification = self
            .repository
            .find_by_id(notification_id)
            .await
            .map_err(NotificationError::Database)?
            .ok_or(NotificationError::NotFoundById(notification_id))?;

        // Verify ownership
        if notification.user_id != user_id {
            return Err(NotificationError::NotOwned {
                notification_id,
                user_id,
            });
        }

        Ok(notification)
    }

    /// List notifications for a user with pagination and optional filtering.
    ///
    /// Returns a vector of notifications sorted by created_at descending.
    ///
    /// # Arguments
    /// * `user_id` - The user to list notifications for
    /// * `unread_only` - If true, only return unread notifications
    /// * `limit` - Maximum number of notifications to return
    /// * `offset` - Number of notifications to skip (for pagination)
    pub async fn list_notifications(
        &self,
        user_id: UserId,
        unread_only: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Notification>, NotificationError> {
        self.repository
            .list_for_user(user_id, unread_only, limit, offset)
            .await
            .map_err(NotificationError::Database)
    }

    /// Count unread notifications for a user.
    ///
    /// Returns the count or a NotificationError.
    pub async fn count_unread(&self, user_id: UserId) -> Result<i64, NotificationError> {
        self.repository
            .count_unread(user_id)
            .await
            .map_err(NotificationError::Database)
    }

    /// Count notifications for a user with optional unread filtering.
    pub async fn count_notifications(
        &self,
        user_id: UserId,
        unread_only: bool,
    ) -> Result<i64, NotificationError> {
        self.repository
            .count_for_user(user_id, unread_only)
            .await
            .map_err(NotificationError::Database)
    }

    /// Mark a notification as read with ownership validation.
    ///
    /// Verifies that the user owns the notification before marking it as read.
    /// Returns the updated Notification or a NotificationError.
    pub async fn mark_as_read(
        &self,
        notification_id: NotificationId,
        user_id: UserId,
    ) -> Result<Notification, NotificationError> {
        // First verify ownership
        let notification = self
            .repository
            .find_by_id(notification_id)
            .await
            .map_err(NotificationError::Database)?
            .ok_or(NotificationError::NotFoundById(notification_id))?;

        if notification.user_id != user_id {
            return Err(NotificationError::NotOwned {
                notification_id,
                user_id,
            });
        }

        // Mark as read
        self.repository
            .mark_as_read(notification_id)
            .await
            .map_err(NotificationError::Database)
    }

    /// Delete a notification with ownership validation.
    ///
    /// Verifies that the user owns the notification before deleting it.
    /// Returns unit or a NotificationError.
    pub async fn delete_notification(
        &self,
        notification_id: NotificationId,
        user_id: UserId,
    ) -> Result<(), NotificationError> {
        // First verify ownership
        let notification = self
            .repository
            .find_by_id(notification_id)
            .await
            .map_err(NotificationError::Database)?
            .ok_or(NotificationError::NotFoundById(notification_id))?;

        if notification.user_id != user_id {
            return Err(NotificationError::NotOwned {
                notification_id,
                user_id,
            });
        }

        // Delete
        self.repository
            .delete(notification_id)
            .await
            .map_err(NotificationError::Database)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::sync::Mutex;

    struct MockNotificationRepository {
        notifications: Mutex<Vec<Notification>>,
    }

    impl MockNotificationRepository {
        fn new() -> Self {
            Self {
                notifications: Mutex::new(Vec::new()),
            }
        }
    }

    impl NotificationRepositoryOps for MockNotificationRepository {
        async fn create(&self, request: CreateNotification) -> Result<Notification, sqlx::Error> {
            let notification = Notification {
                id: Uuid::new_v4(),
                user_id: request.user_id,
                notification_type: request.notification_type,
                title: request.title,
                message: request.message,
                resource_id: request.resource_id,
                resource_type: request.resource_type,
                action_url: request.action_url,
                read: false,
                created_at: Utc::now(),
                tenant_id: request.tenant_id,
            };
            self.notifications
                .lock()
                .unwrap()
                .push(notification.clone());
            Ok(notification)
        }

        async fn find_by_id(
            &self,
            notification_id: NotificationId,
        ) -> Result<Option<Notification>, sqlx::Error> {
            Ok(self
                .notifications
                .lock()
                .unwrap()
                .iter()
                .find(|n| n.id == notification_id)
                .cloned())
        }

        async fn list_for_user(
            &self,
            user_id: UserId,
            unread_only: bool,
            limit: i64,
            offset: i64,
        ) -> Result<Vec<Notification>, sqlx::Error> {
            let notifications = self.notifications.lock().unwrap();
            let mut filtered: Vec<_> = notifications
                .iter()
                .filter(|n| n.user_id == user_id && (!unread_only || !n.read))
                .cloned()
                .collect();

            // Sort by created_at descending
            filtered.sort_by(|a, b| b.created_at.cmp(&a.created_at));

            // Apply pagination
            let start = offset as usize;
            let result = filtered
                .into_iter()
                .skip(start)
                .take(limit as usize)
                .collect();

            Ok(result)
        }

        async fn count_for_user(
            &self,
            user_id: UserId,
            unread_only: bool,
        ) -> Result<i64, sqlx::Error> {
            let count = self
                .notifications
                .lock()
                .unwrap()
                .iter()
                .filter(|n| n.user_id == user_id && (!unread_only || !n.read))
                .count();

            Ok(count as i64)
        }

        async fn count_unread(&self, user_id: UserId) -> Result<i64, sqlx::Error> {
            let count = self
                .notifications
                .lock()
                .unwrap()
                .iter()
                .filter(|n| n.user_id == user_id && !n.read)
                .count();
            Ok(count as i64)
        }

        async fn mark_as_read(
            &self,
            notification_id: NotificationId,
        ) -> Result<Notification, sqlx::Error> {
            let mut notifications = self.notifications.lock().unwrap();
            if let Some(notification) = notifications.iter_mut().find(|n| n.id == notification_id) {
                notification.read = true;
                Ok(notification.clone())
            } else {
                Err(sqlx::Error::RowNotFound)
            }
        }

        async fn delete(&self, notification_id: NotificationId) -> Result<(), sqlx::Error> {
            let mut notifications = self.notifications.lock().unwrap();
            if let Some(pos) = notifications.iter().position(|n| n.id == notification_id) {
                notifications.remove(pos);
                Ok(())
            } else {
                Err(sqlx::Error::RowNotFound)
            }
        }
    }

    fn setup_service() -> NotificationService<MockNotificationRepository> {
        let repository = MockNotificationRepository::new();
        NotificationService::new(repository)
    }

    fn notification_request(
        user_id: UserId,
        notification_type: NotificationType,
        title: &str,
        message: &str,
        resource_id: Uuid,
        resource_type: ResourceType,
        action_url: Option<&str>,
    ) -> CreateNotification {
        CreateNotification {
            user_id,
            notification_type,
            title: title.to_string(),
            message: message.to_string(),
            resource_id,
            resource_type,
            action_url: action_url.map(str::to_string),
            tenant_id: Uuid::new_v4(),
        }
    }

    #[tokio::test]
    async fn test_create_notification() {
        let service = setup_service();
        let user_id = Uuid::new_v4();
        let resource_id = Uuid::new_v4();

        let notification = service
            .create_notification(notification_request(
                user_id,
                NotificationType::ShareReceived,
                "File shared",
                "Alice shared file.pdf with you",
                resource_id,
                ResourceType::File,
                Some("/files/123"),
            ))
            .await
            .unwrap();

        assert_eq!(notification.user_id, user_id);
        assert_eq!(
            notification.notification_type,
            NotificationType::ShareReceived
        );
        assert_eq!(notification.title, "File shared");
        assert!(!notification.read);
    }

    #[tokio::test]
    async fn test_get_notification_success() {
        let service = setup_service();
        let user_id = Uuid::new_v4();
        let resource_id = Uuid::new_v4();

        let created = service
            .create_notification(notification_request(
                user_id,
                NotificationType::ShareReceived,
                "Test",
                "Message",
                resource_id,
                ResourceType::File,
                None,
            ))
            .await
            .unwrap();

        let retrieved = service.get_notification(created.id, user_id).await.unwrap();

        assert_eq!(retrieved.id, created.id);
        assert_eq!(retrieved.user_id, user_id);
    }

    #[tokio::test]
    async fn test_get_notification_not_owned() {
        let service = setup_service();
        let owner_id = Uuid::new_v4();
        let other_user_id = Uuid::new_v4();
        let resource_id = Uuid::new_v4();

        let notification = service
            .create_notification(notification_request(
                owner_id,
                NotificationType::ShareReceived,
                "Test",
                "Message",
                resource_id,
                ResourceType::File,
                None,
            ))
            .await
            .unwrap();

        let result = service
            .get_notification(notification.id, other_user_id)
            .await;

        assert!(matches!(result, Err(NotificationError::NotOwned { .. })));
    }

    #[tokio::test]
    async fn test_get_notification_not_found() {
        let service = setup_service();
        let user_id = Uuid::new_v4();
        let nonexistent_id = Uuid::new_v4();

        let result = service.get_notification(nonexistent_id, user_id).await;

        assert!(matches!(result, Err(NotificationError::NotFoundById(_))));
    }

    #[tokio::test]
    async fn test_list_notifications() {
        let service = setup_service();
        let user_id = Uuid::new_v4();
        let other_user_id = Uuid::new_v4();
        let resource_id = Uuid::new_v4();

        // Create notifications for user
        service
            .create_notification(notification_request(
                user_id,
                NotificationType::ShareReceived,
                "First",
                "Message 1",
                resource_id,
                ResourceType::File,
                None,
            ))
            .await
            .unwrap();

        service
            .create_notification(notification_request(
                user_id,
                NotificationType::PermissionChanged,
                "Second",
                "Message 2",
                resource_id,
                ResourceType::Folder,
                None,
            ))
            .await
            .unwrap();

        // Create notification for different user
        service
            .create_notification(notification_request(
                other_user_id,
                NotificationType::ShareReceived,
                "Other",
                "Message 3",
                resource_id,
                ResourceType::File,
                None,
            ))
            .await
            .unwrap();

        // List notifications for user
        let notifications = service
            .list_notifications(user_id, false, 10, 0)
            .await
            .unwrap();

        assert_eq!(notifications.len(), 2);
        assert!(notifications.iter().all(|n| n.user_id == user_id));
    }

    #[tokio::test]
    async fn test_list_notifications_unread_only() {
        let service = setup_service();
        let user_id = Uuid::new_v4();
        let resource_id = Uuid::new_v4();

        // Create notifications
        let n1 = service
            .create_notification(notification_request(
                user_id,
                NotificationType::ShareReceived,
                "First",
                "Message 1",
                resource_id,
                ResourceType::File,
                None,
            ))
            .await
            .unwrap();

        service
            .create_notification(notification_request(
                user_id,
                NotificationType::PermissionChanged,
                "Second",
                "Message 2",
                resource_id,
                ResourceType::Folder,
                None,
            ))
            .await
            .unwrap();

        // Mark first as read
        service.mark_as_read(n1.id, user_id).await.unwrap();

        // List unread only
        let unread = service
            .list_notifications(user_id, true, 10, 0)
            .await
            .unwrap();

        assert_eq!(unread.len(), 1);
        assert_eq!(unread[0].title, "Second");
    }

    #[tokio::test]
    async fn test_list_notifications_pagination() {
        let service = setup_service();
        let user_id = Uuid::new_v4();
        let resource_id = Uuid::new_v4();

        // Create 5 notifications
        for i in 0..5 {
            service
                .create_notification(CreateNotification {
                    user_id,
                    notification_type: NotificationType::ShareReceived,
                    title: format!("Notification {}", i),
                    message: format!("Message {}", i),
                    resource_id,
                    resource_type: ResourceType::File,
                    action_url: None,
                    tenant_id: Uuid::new_v4(),
                })
                .await
                .unwrap();
        }

        // Get first page (2 items)
        let page1 = service
            .list_notifications(user_id, false, 2, 0)
            .await
            .unwrap();
        assert_eq!(page1.len(), 2);

        // Get second page (2 items)
        let page2 = service
            .list_notifications(user_id, false, 2, 2)
            .await
            .unwrap();
        assert_eq!(page2.len(), 2);

        // Get third page (1 item remaining)
        let page3 = service
            .list_notifications(user_id, false, 2, 4)
            .await
            .unwrap();
        assert_eq!(page3.len(), 1);
    }

    #[tokio::test]
    async fn test_count_unread() {
        let service = setup_service();
        let user_id = Uuid::new_v4();
        let resource_id = Uuid::new_v4();

        // Initially no unread
        let count = service.count_unread(user_id).await.unwrap();
        assert_eq!(count, 0);

        // Create notifications
        let n1 = service
            .create_notification(notification_request(
                user_id,
                NotificationType::ShareReceived,
                "First",
                "Message 1",
                resource_id,
                ResourceType::File,
                None,
            ))
            .await
            .unwrap();

        service
            .create_notification(notification_request(
                user_id,
                NotificationType::PermissionChanged,
                "Second",
                "Message 2",
                resource_id,
                ResourceType::Folder,
                None,
            ))
            .await
            .unwrap();

        // Count should be 2
        let count = service.count_unread(user_id).await.unwrap();
        assert_eq!(count, 2);

        // Mark one as read
        service.mark_as_read(n1.id, user_id).await.unwrap();

        // Count should be 1
        let count = service.count_unread(user_id).await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_count_notifications_total_and_unread() {
        let service = setup_service();
        let user_id = Uuid::new_v4();
        let resource_id = Uuid::new_v4();

        let first = service
            .create_notification(notification_request(
                user_id,
                NotificationType::ShareReceived,
                "First",
                "Message 1",
                resource_id,
                ResourceType::File,
                None,
            ))
            .await
            .unwrap();

        service
            .create_notification(notification_request(
                user_id,
                NotificationType::PermissionChanged,
                "Second",
                "Message 2",
                resource_id,
                ResourceType::Folder,
                None,
            ))
            .await
            .unwrap();

        service.mark_as_read(first.id, user_id).await.unwrap();

        let total = service.count_notifications(user_id, false).await.unwrap();
        let unread = service.count_notifications(user_id, true).await.unwrap();

        assert_eq!(total, 2);
        assert_eq!(unread, 1);
    }

    #[tokio::test]
    async fn test_mark_as_read_success() {
        let service = setup_service();
        let user_id = Uuid::new_v4();
        let resource_id = Uuid::new_v4();

        let notification = service
            .create_notification(notification_request(
                user_id,
                NotificationType::ShareReceived,
                "Test",
                "Message",
                resource_id,
                ResourceType::File,
                None,
            ))
            .await
            .unwrap();

        assert!(!notification.read);

        let updated = service
            .mark_as_read(notification.id, user_id)
            .await
            .unwrap();

        assert!(updated.read);
    }

    #[tokio::test]
    async fn test_mark_as_read_not_owned() {
        let service = setup_service();
        let owner_id = Uuid::new_v4();
        let other_user_id = Uuid::new_v4();
        let resource_id = Uuid::new_v4();

        let notification = service
            .create_notification(notification_request(
                owner_id,
                NotificationType::ShareReceived,
                "Test",
                "Message",
                resource_id,
                ResourceType::File,
                None,
            ))
            .await
            .unwrap();

        let result = service.mark_as_read(notification.id, other_user_id).await;

        assert!(matches!(result, Err(NotificationError::NotOwned { .. })));
    }

    #[tokio::test]
    async fn test_delete_notification_success() {
        let service = setup_service();
        let user_id = Uuid::new_v4();
        let resource_id = Uuid::new_v4();

        let notification = service
            .create_notification(notification_request(
                user_id,
                NotificationType::ShareReceived,
                "Test",
                "Message",
                resource_id,
                ResourceType::File,
                None,
            ))
            .await
            .unwrap();

        // Delete should succeed
        service
            .delete_notification(notification.id, user_id)
            .await
            .unwrap();

        // Notification should no longer exist
        let result = service.get_notification(notification.id, user_id).await;
        assert!(matches!(result, Err(NotificationError::NotFoundById(_))));
    }

    #[tokio::test]
    async fn test_delete_notification_not_owned() {
        let service = setup_service();
        let owner_id = Uuid::new_v4();
        let other_user_id = Uuid::new_v4();
        let resource_id = Uuid::new_v4();

        let notification = service
            .create_notification(notification_request(
                owner_id,
                NotificationType::ShareReceived,
                "Test",
                "Message",
                resource_id,
                ResourceType::File,
                None,
            ))
            .await
            .unwrap();

        let result = service
            .delete_notification(notification.id, other_user_id)
            .await;

        assert!(matches!(result, Err(NotificationError::NotOwned { .. })));
    }

    #[tokio::test]
    async fn test_delete_notification_not_found() {
        let service = setup_service();
        let user_id = Uuid::new_v4();
        let nonexistent_id = Uuid::new_v4();

        let result = service.delete_notification(nonexistent_id, user_id).await;

        assert!(matches!(result, Err(NotificationError::NotFoundById(_))));
    }
}
