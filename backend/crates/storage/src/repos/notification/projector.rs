//! Notification projector - creates notifications from events

use super::{NotificationRepository, NotificationRepositoryError};
use crate::metadata_v2::schemas::{EventDocument, EventType};
use rustshare_core::domain::{Notification, NotificationType, ResourceType};
use tracing::{debug, error, warn};
use uuid::Uuid;

/// Projects events into notifications
pub struct NotificationProjector<R: NotificationRepository> {
    repository: R,
}

impl<R: NotificationRepository> NotificationProjector<R> {
    /// Create a new notification projector
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    /// Project a single event into notifications
    pub async fn project(&self, event: &EventDocument) -> Result<(), NotificationRepositoryError> {
        match event.event_type {
            EventType::ShareCreated => self.project_share_created(event).await,
            EventType::ShareRevoked => self.project_share_revoked(event).await,
            EventType::FileModified => self.project_file_modified(event).await,
            _ => {
                // Event doesn't generate notifications
                Ok(())
            }
        }
    }

    /// Project multiple events
    pub async fn project_batch(
        &self,
        events: &[EventDocument],
    ) -> Result<u32, NotificationRepositoryError> {
        let mut count = 0;
        for event in events {
            if let Err(e) = self.project(event).await {
                error!("Failed to project event {}: {}", event.id, e);
            } else {
                count += 1;
            }
        }
        Ok(count)
    }

    /// Handle share created event
    async fn project_share_created(
        &self,
        event: &EventDocument,
    ) -> Result<(), NotificationRepositoryError> {
        // Extract recipient from event payload
        let recipient_id = event
            .payload
            .get("recipient_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());

        if let Some(recipient_id) = recipient_id {
            let resource_type = event.resource_type.clone();
            let resource_name = event
                .payload
                .get("resource_name")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string();

            let (title, message, notif_type) = if resource_type == "file" {
                (
                    "File shared with you".to_string(),
                    format!(
                        "{} shared a file with you: {}",
                        event.actor_id, resource_name
                    ),
                    NotificationType::ShareReceived,
                )
            } else {
                (
                    "Folder shared with you".to_string(),
                    format!(
                        "{} shared a folder with you: {}",
                        event.actor_id, resource_name
                    ),
                    NotificationType::ShareReceived,
                )
            };

            let notification = Notification {
                id: Uuid::new_v4(),
                user_id: recipient_id,
                notification_type: notif_type,
                resource_type: resource_type.parse().unwrap_or(ResourceType::File),
                resource_id: event.resource_id,
                title,
                message,
                action_url: None,
                read: false,
                created_at: event.occurred_at,
                tenant_id: Uuid::nil(),
            };

            debug!(
                "Creating notification for user {}: {}",
                recipient_id, notification.title
            );
            self.repository.create_notification(&notification).await?;
        }

        Ok(())
    }

    /// Handle share revoked event
    async fn project_share_revoked(
        &self,
        event: &EventDocument,
    ) -> Result<(), NotificationRepositoryError> {
        let recipient_id = event
            .payload
            .get("recipient_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());

        if let Some(recipient_id) = recipient_id {
            let resource_type = event.resource_type.clone();

            let notification = Notification {
                id: Uuid::new_v4(),
                user_id: recipient_id,
                notification_type: NotificationType::ShareRevoked,
                resource_type: resource_type.parse().unwrap_or(ResourceType::File),
                resource_id: event.resource_id,
                title: "Share revoked".to_string(),
                message: format!(
                    "Your access to {} {} has been revoked",
                    resource_type, event.resource_id
                ),
                action_url: None,
                read: false,
                created_at: event.occurred_at,
                tenant_id: Uuid::nil(),
            };

            debug!("Creating revocation notification for user {}", recipient_id);
            self.repository.create_notification(&notification).await?;
        }

        Ok(())
    }

    /// Handle file modified event
    async fn project_file_modified(
        &self,
        event: &EventDocument,
    ) -> Result<(), NotificationRepositoryError> {
        // Notify users who have access to this file
        let shared_with = event.payload.get("shared_with").and_then(|v| v.as_array());

        if let Some(user_ids) = shared_with {
            let file_name = event
                .payload
                .get("file_name")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string();

            for user_id_value in user_ids {
                if let Some(user_id_str) = user_id_value.as_str() {
                    if let Ok(user_id) = Uuid::parse_str(user_id_str) {
                        // Don't notify the actor who made the change
                        if user_id != event.actor_id {
                            let notification = Notification {
                                id: Uuid::new_v4(),
                                user_id,
                                notification_type: NotificationType::PermissionChanged,
                                resource_type: ResourceType::File,
                                resource_id: event.resource_id,
                                title: "File updated".to_string(),
                                message: format!("{} was updated", file_name),
                                action_url: None,
                                read: false,
                                created_at: event.occurred_at,
                                tenant_id: Uuid::nil(),
                            };

                            if let Err(e) = self.repository.create_notification(&notification).await
                            {
                                warn!("Failed to create notification for user {}: {}", user_id, e);
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata_v2::stores::LocalFsDocumentStore;
    use crate::metadata_v2::{MetadataBackendConfig, MetadataDocumentStore};
    use crate::repos::notification::rustfs::RustFsNotificationRepository;
    use crate::repos::notification::NotificationQuery;
    use serde_json::json;
    use tempfile::TempDir;

    async fn create_test_projector() -> (
        NotificationProjector<RustFsNotificationRepository>,
        TempDir,
        Uuid,
    ) {
        let temp_dir = TempDir::new().unwrap();
        let config = MetadataBackendConfig {
            base_prefix: "test".to_string(),
            namespace: "default".to_string(),
            enable_optimistic_concurrency: true,
            fallback_to_leases: true,
        };

        let doc_store: Arc<dyn MetadataDocumentStore> = Arc::new(LocalFsDocumentStore::new(
            temp_dir.path().to_path_buf(),
            config,
        ));

        let repo = RustFsNotificationRepository::new(
            doc_store,
            "apps/rustshare".to_string(),
            "test".to_string(),
        );

        let projector = NotificationProjector::new(repo);
        let user_id = Uuid::new_v4();

        (projector, temp_dir, user_id)
    }

    use std::sync::Arc;

    #[tokio::test]
    async fn test_project_share_created() {
        let (projector, _temp, recipient_id) = create_test_projector().await;
        let actor_id = Uuid::new_v4();
        let resource_id = Uuid::new_v4();

        let event = EventDocument {
            schema_version: 1,
            id: Uuid::new_v4(),
            event_type: EventType::ShareCreated,
            actor_id,
            resource_type: "file".to_string(),
            resource_id,
            occurred_at: chrono::Utc::now(),
            correlation_id: None,
            payload: json!({
                "recipient_id": recipient_id.to_string(),
                "resource_name": "document.pdf"
            }),
            tenant_id: Uuid::new_v4(),
        };

        projector.project(&event).await.unwrap();

        // Check notification was created
        let notifications = projector
            .repository
            .get_user_notifications(recipient_id, NotificationQuery::default())
            .await
            .unwrap();

        assert_eq!(notifications.len(), 1);
        assert_eq!(
            notifications[0].notification_type,
            rustshare_core::domain::NotificationType::ShareReceived
        );
        assert_eq!(notifications[0].title, "File shared with you");
    }

    #[tokio::test]
    async fn test_project_file_modified() {
        let (projector, _temp, user1) = create_test_projector().await;
        let user2 = Uuid::new_v4();
        let actor_id = Uuid::new_v4();
        let resource_id = Uuid::new_v4();

        let event = EventDocument {
            schema_version: 1,
            id: Uuid::new_v4(),
            event_type: EventType::FileModified,
            actor_id,
            resource_type: "file".to_string(),
            resource_id,
            occurred_at: chrono::Utc::now(),
            correlation_id: None,
            payload: json!({
                "shared_with": [user1.to_string(), user2.to_string()],
                "file_name": "report.docx"
            }),
            tenant_id: Uuid::new_v4(),
        };

        projector.project(&event).await.unwrap();

        // Check notifications were created
        let notifs1 = projector
            .repository
            .get_user_notifications(user1, NotificationQuery::default())
            .await
            .unwrap();
        assert_eq!(notifs1.len(), 1);

        let notifs2 = projector
            .repository
            .get_user_notifications(user2, NotificationQuery::default())
            .await
            .unwrap();
        assert_eq!(notifs2.len(), 1);
    }
}
