//! RustFS-backed notification repository implementation

use super::*;
use crate::metadata_v2::{
    schemas::{
        NotificationDocument, NotificationRef, UserNotificationIndex,
        CURRENT_SCHEMA_VERSION,
    },
    MetadataDocumentStore, MetadataDocumentStoreExt, PutOptions,
};
use crate::repos::PathBuilder;
use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

/// RustFS-backed notification repository
pub struct RustFsNotificationRepository {
    doc_store: Arc<dyn MetadataDocumentStore>,
    path_builder: PathBuilder,
}

impl RustFsNotificationRepository {
    /// Create a new RustFS notification repository
    pub fn new(doc_store: Arc<dyn MetadataDocumentStore>, base_prefix: String, namespace: String) -> Self {
        Self {
            doc_store,
            path_builder: PathBuilder::new(base_prefix, namespace),
        }
    }
    
    /// Get or create user notification index
    async fn get_or_create_index(&self, user_id: Uuid) -> Result<UserNotificationIndex, NotificationRepositoryError> {
        let index_path = self.path_builder.user_index_path(user_id);
        
        match self.doc_store.get::<UserNotificationIndex>(&index_path).await {
            Ok(Some((index, _))) => Ok(index),
            Ok(None) => Ok(UserNotificationIndex::new(user_id)),
            Err(e) => Err(NotificationRepositoryError::Storage(e.to_string())),
        }
    }
    
    /// Save user notification index
    async fn save_index(&self, index: &UserNotificationIndex) -> Result<(), NotificationRepositoryError> {
        let index_path = self.path_builder.user_index_path(index.user_id);
        
        self.doc_store
            .put(&index_path, index, PutOptions::default())
            .await
            .map_err(|e| NotificationRepositoryError::Storage(e.to_string()))?;
        
        Ok(())
    }
    
    /// Update index when notification is created
    async fn add_to_index(&self, doc: &NotificationDocument) -> Result<(), NotificationRepositoryError> {
        let mut index = self.get_or_create_index(doc.user_id).await?;
        
        let notif_ref = NotificationRef {
            notification_id: doc.id,
            notification_type: doc.notification_type,
            resource_type: doc.resource_type.clone(),
            resource_id: doc.resource_id,
            read: doc.read,
            created_at: doc.created_at,
        };
        
        index.add_notification(&notif_ref);
        self.save_index(&index).await
    }
    
    /// Update index when notification is marked read
    async fn mark_read_in_index(&self, user_id: Uuid, notification_id: Uuid) -> Result<(), NotificationRepositoryError> {
        let mut index = self.get_or_create_index(user_id).await?;
        index.mark_read(notification_id);
        self.save_index(&index).await
    }
    
    /// Update index when notification is deleted
    async fn remove_from_index(&self, user_id: Uuid, notification_id: Uuid) -> Result<(), NotificationRepositoryError> {
        let mut index = self.get_or_create_index(user_id).await?;
        index.remove_notification(notification_id);
        self.save_index(&index).await
    }
}

#[async_trait]
impl NotificationRepository for RustFsNotificationRepository {
    async fn create_notification(&self, notification: &Notification) -> Result<(), NotificationRepositoryError> {
        let doc = super::conversions::notification_to_doc(notification);
        let path = self.path_builder.notification_path(doc.user_id, doc.id);
        
        // Store notification
        self.doc_store
            .put(&path, &doc, PutOptions::default())
            .await
            .map_err(|e| NotificationRepositoryError::Storage(e.to_string()))?;
        
        // Update index
        self.add_to_index(&doc).await?;
        
        Ok(())
    }
    
    async fn get_notification(&self, user_id: Uuid, id: Uuid) -> Result<Option<Notification>, NotificationRepositoryError> {
        let path = self.path_builder.notification_path(user_id, id);
        
        match self.doc_store.get::<NotificationDocument>(&path).await {
            Ok(Some((doc, _))) => Ok(Some(super::conversions::doc_to_notification(doc))),
            Ok(None) => Ok(None),
            Err(e) => Err(NotificationRepositoryError::Storage(e.to_string())),
        }
    }
    
    async fn get_user_notifications(
        &self,
        user_id: Uuid,
        query: NotificationQuery,
    ) -> Result<Vec<Notification>, NotificationRepositoryError> {
        let index = self.get_or_create_index(user_id).await?;
        
        // Get notification references from index
        let refs: Vec<NotificationRef> = index
            .notifications
            .into_iter()
            .filter(|n| query.read.map_or(true, |r| n.read == r))
            .skip(query.offset.unwrap_or(0))
            .take(query.limit.unwrap_or(usize::MAX))
            .collect();
        
        // Fetch full documents in parallel
        let paths: Vec<String> = refs
            .iter()
            .map(|r| self.path_builder.notification_path(user_id, r.notification_id))
            .collect();
        let path_refs: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
        
        let mut notifications = Vec::new();
        if !path_refs.is_empty() {
            match self.doc_store.get_multi::<NotificationDocument>(&path_refs).await {
                Ok(results) => {
                    for (_, doc, _) in results {
                        notifications.push(super::conversions::doc_to_notification(doc));
                    }
                }
                Err(e) => return Err(NotificationRepositoryError::Storage(e.to_string())),
            }
        }
        
        Ok(notifications)
    }
    
    async fn count_unread(&self, user_id: Uuid) -> Result<u32, NotificationRepositoryError> {
        let index = self.get_or_create_index(user_id).await?;
        Ok(index.unread_count)
    }
    
    async fn mark_read(&self, user_id: Uuid, id: Uuid) -> Result<(), NotificationRepositoryError> {
        let path = self.path_builder.notification_path(user_id, id);
        
        match self.doc_store.get::<NotificationDocument>(&path).await {
            Ok(Some((mut doc, meta))) => {
                doc.read = true;
                doc.read_at = Some(chrono::Utc::now());
                
                self.doc_store
                    .put(&path, &doc, PutOptions {
                        if_match: Some(meta.etag),
                        ..Default::default()
                    })
                    .await
                    .map_err(|e| NotificationRepositoryError::Storage(e.to_string()))?;
                
                self.mark_read_in_index(user_id, id).await?;
                Ok(())
            }
            Ok(None) => Err(NotificationRepositoryError::NotFound(id)),
            Err(e) => Err(NotificationRepositoryError::Storage(e.to_string())),
        }
    }
    
    async fn mark_all_read(&self, user_id: Uuid) -> Result<u32, NotificationRepositoryError> {
        let mut index = self.get_or_create_index(user_id).await?;
        let count = index.unread_count;
        
        if count > 0 {
            // Mark all as read in index
            for notif_ref in &mut index.notifications {
                if !notif_ref.read {
                    notif_ref.read = true;
                    
                    // Update the document
                    let path = self.path_builder.notification_path(user_id, notif_ref.notification_id);
                    if let Ok(Some((mut doc, meta))) = self.doc_store.get::<NotificationDocument>(&path).await {
                        doc.read = true;
                        doc.read_at = Some(chrono::Utc::now());
                        let _ = self.doc_store.put(&path, &doc, PutOptions {
                            if_match: Some(meta.etag),
                            ..Default::default()
                        }).await;
                    }
                }
            }
            index.unread_count = 0;
            index.version += 1;
            index.updated_at = chrono::Utc::now();
            
            self.save_index(&index).await?;
        }
        
        Ok(count)
    }
    
    async fn delete_notification(&self, user_id: Uuid, id: Uuid) -> Result<(), NotificationRepositoryError> {
        let path = self.path_builder.notification_path(user_id, id);
        
        // Check if the notification exists first
        match self.doc_store.get::<NotificationDocument>(&path).await {
            Ok(Some(_)) => {
                self.doc_store
                    .delete(&path)
                    .await
                    .map_err(|e| NotificationRepositoryError::Storage(e.to_string()))?;
                
                self.remove_from_index(user_id, id).await?;
                Ok(())
            }
            Ok(None) => Err(NotificationRepositoryError::NotFound(id)),
            Err(e) => Err(NotificationRepositoryError::Storage(e.to_string())),
        }
    }
    
    async fn delete_all_for_user(&self, user_id: Uuid) -> Result<u32, NotificationRepositoryError> {
        let index = self.get_or_create_index(user_id).await?;
        let count = index.notifications.len() as u32;
        
        if count > 0 {
            // Delete all notification documents
            for notif_ref in &index.notifications {
                let path = self.path_builder.notification_path(user_id, notif_ref.notification_id);
                let _ = self.doc_store.delete(&path).await;
            }
            
            // Clear index
            let new_index = UserNotificationIndex::new(user_id);
            self.save_index(&new_index).await?;
        }
        
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata_v2::stores::LocalFsDocumentStore;
    use crate::metadata_v2::MetadataBackendConfig;
    use rustshare_core::domain::NotificationType;
    use tempfile::TempDir;

    async fn create_test_repository() -> (RustFsNotificationRepository, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = MetadataBackendConfig {
            base_prefix: "test".to_string(),
            namespace: "default".to_string(),
            enable_optimistic_concurrency: true,
            fallback_to_leases: true,
        };
        
        let doc_store: Arc<dyn MetadataDocumentStore> = Arc::new(
            LocalFsDocumentStore::new(temp_dir.path().to_path_buf(), config)
        );
        
        let repo = RustFsNotificationRepository::new(
            doc_store,
            "apps/rustshare".to_string(),
            "test".to_string(),
        );
        
        (repo, temp_dir)
    }

    use rustshare_core::domain::ResourceType;

    fn create_test_notification(user_id: Uuid, id: Uuid, title: &str) -> Notification {
        Notification {
            id,
            user_id,
            notification_type: NotificationType::ShareReceived,
            resource_type: ResourceType::File,
            resource_id: Uuid::new_v4(),
            title: title.to_string(),
            message: "Test message".to_string(),
            action_url: None,
            read: false,
            created_at: chrono::Utc::now(),
            tenant_id: Uuid::nil(),
        }
    }

    #[tokio::test]
    async fn test_create_and_list_notifications() {
        let (repo, _temp) = create_test_repository().await;
        let user_id = Uuid::new_v4();
        
        // Create notifications
        for i in 0..5 {
            let notif = create_test_notification(user_id, Uuid::new_v4(), &format!("Notif {}", i));
            repo.create_notification(&notif).await.unwrap();
        }
        
        // List all
        let query = NotificationQuery::default();
        let notifs = repo.get_user_notifications(user_id, query).await.unwrap();
        assert_eq!(notifs.len(), 5);
        
        // Count unread
        let unread = repo.count_unread(user_id).await.unwrap();
        assert_eq!(unread, 5);
    }

    #[tokio::test]
    async fn test_mark_all_read() {
        let (repo, _temp) = create_test_repository().await;
        let user_id = Uuid::new_v4();
        
        // Create notifications
        for i in 0..3 {
            let notif = create_test_notification(user_id, Uuid::new_v4(), &format!("Notif {}", i));
            repo.create_notification(&notif).await.unwrap();
        }
        
        // Mark all read
        let marked = repo.mark_all_read(user_id).await.unwrap();
        assert_eq!(marked, 3);
        
        // Count unread
        let unread = repo.count_unread(user_id).await.unwrap();
        assert_eq!(unread, 0);
        
        // List unread only
        let query = NotificationQuery {
            read: Some(false),
            ..Default::default()
        };
        let notifs = repo.get_user_notifications(user_id, query).await.unwrap();
        assert_eq!(notifs.len(), 0);
    }

    #[tokio::test]
    async fn test_delete_all() {
        let (repo, _temp) = create_test_repository().await;
        let user_id = Uuid::new_v4();
        
        // Create notifications
        for i in 0..3 {
            let notif = create_test_notification(user_id, Uuid::new_v4(), &format!("Notif {}", i));
            repo.create_notification(&notif).await.unwrap();
        }
        
        // Delete all
        let deleted = repo.delete_all_for_user(user_id).await.unwrap();
        assert_eq!(deleted, 3);
        
        // List should be empty
        let query = NotificationQuery::default();
        let notifs = repo.get_user_notifications(user_id, query).await.unwrap();
        assert_eq!(notifs.len(), 0);
    }

    #[tokio::test]
    async fn test_get_notification() {
        let (repo, _temp) = create_test_repository().await;
        let user_id = Uuid::new_v4();
        let notif_id = Uuid::new_v4();
        
        let notif = create_test_notification(user_id, notif_id, "Test get");
        repo.create_notification(&notif).await.unwrap();
        
        // Get existing notification
        let retrieved = repo.get_notification(user_id, notif_id).await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, notif_id);
        assert_eq!(retrieved.user_id, user_id);
        assert_eq!(retrieved.title, "Test get");
        
        // Get non-existent notification
        let missing = repo.get_notification(user_id, Uuid::new_v4()).await.unwrap();
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn test_mark_read() {
        let (repo, _temp) = create_test_repository().await;
        let user_id = Uuid::new_v4();
        let notif_id = Uuid::new_v4();
        
        let notif = create_test_notification(user_id, notif_id, "Test read");
        repo.create_notification(&notif).await.unwrap();
        
        // Mark as read
        repo.mark_read(user_id, notif_id).await.unwrap();
        
        // Verify via get_notification
        let retrieved = repo.get_notification(user_id, notif_id).await.unwrap().unwrap();
        assert!(retrieved.read);
        
        // Verify via index (unread count)
        let unread = repo.count_unread(user_id).await.unwrap();
        assert_eq!(unread, 0);
        
        // Mark non-existent notification should fail
        let result = repo.mark_read(user_id, Uuid::new_v4()).await;
        assert!(matches!(result, Err(NotificationRepositoryError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_delete_notification() {
        let (repo, _temp) = create_test_repository().await;
        let user_id = Uuid::new_v4();
        let notif_id = Uuid::new_v4();
        
        let notif = create_test_notification(user_id, notif_id, "Test delete");
        repo.create_notification(&notif).await.unwrap();
        
        // Delete notification
        repo.delete_notification(user_id, notif_id).await.unwrap();
        
        // Verify deleted
        let retrieved = repo.get_notification(user_id, notif_id).await.unwrap();
        assert!(retrieved.is_none());
        
        // Verify index updated
        let query = NotificationQuery::default();
        let notifs = repo.get_user_notifications(user_id, query).await.unwrap();
        assert_eq!(notifs.len(), 0);
        
        // Delete non-existent notification should fail
        let result = repo.delete_notification(user_id, Uuid::new_v4()).await;
        assert!(matches!(result, Err(NotificationRepositoryError::NotFound(_))));
    }
}
