//! RustFS-backed user repository implementation

use super::*;
use crate::metadata_v2::{
    schemas::UserDocument,
    MetadataDocumentStore, MetadataDocumentStoreExt, PutOptions,
};
use crate::repos::PathBuilder;
use async_trait::async_trait;
use std::sync::Arc;
use tracing::debug;
use uuid::Uuid;

/// RustFS-backed user repository
pub struct RustFsUserRepository {
    doc_store: Arc<dyn MetadataDocumentStore>,
    path_builder: PathBuilder,
}

/// Email index entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct EmailIndexEntry {
    email: String,
    user_id: Uuid,
}

/// Username index entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct UsernameIndexEntry {
    username: String,
    user_id: Uuid,
}

/// User list index
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct UserListIndex {
    user_ids: Vec<Uuid>,
    version: u64,
}

impl RustFsUserRepository {
    /// Create a new RustFS user repository
    pub fn new(doc_store: Arc<dyn MetadataDocumentStore>, base_prefix: String, namespace: String) -> Self {
        Self {
            doc_store,
            path_builder: PathBuilder::new(base_prefix, namespace),
        }
    }
    
    /// Update indexes after user creation
    async fn update_indexes_for_create(&self, user: &UserDocument) -> Result<(), UserRepositoryError> {
        // Update email index
        let email_entry = EmailIndexEntry {
            email: user.email.clone(),
            user_id: user.id,
        };
        let email_path = self.path_builder.email_index_path(&user.email);
        self.doc_store
            .put(&email_path, &email_entry, PutOptions::default())
            .await
            .map_err(|e| UserRepositoryError::Storage(e.to_string()))?;
        
        // Update username index
        let username_entry = UsernameIndexEntry {
            username: user.username.clone(),
            user_id: user.id,
        };
        let username_path = self.path_builder.username_index_path(&user.username);
        self.doc_store
            .put(&username_path, &username_entry, PutOptions::default())
            .await
            .map_err(|e| UserRepositoryError::Storage(e.to_string()))?;
        
        // Update user list index
        self.add_to_user_list(user.id).await?;
        
        Ok(())
    }
    
    /// Add user to list index
    async fn add_to_user_list(&self, user_id: Uuid) -> Result<(), UserRepositoryError> {
        let list_path = self.path_builder.user_list_path();
        
        // Get current list or create new
        let mut list: UserListIndex = self
            .doc_store
            .get(&list_path)
            .await
            .map_err(|e| UserRepositoryError::Storage(e.to_string()))?
            .map(|(l, _)| l)
            .unwrap_or_default();
        
        // Add user if not present
        if !list.user_ids.contains(&user_id) {
            list.user_ids.push(user_id);
            list.version += 1;
            
            self.doc_store
                .put(&list_path, &list, PutOptions::default())
                .await
                .map_err(|e| UserRepositoryError::Storage(e.to_string()))?;
        }
        
        Ok(())
    }
    
    /// Remove user from list index
    async fn remove_from_user_list(&self, user_id: Uuid) -> Result<(), UserRepositoryError> {
        let list_path = self.path_builder.user_list_path();
        
        // Get current list
        let mut list: UserListIndex = self
            .doc_store
            .get(&list_path)
            .await
            .map_err(|e| UserRepositoryError::Storage(e.to_string()))?
            .map(|(l, _)| l)
            .unwrap_or_default();
        
        // Remove user if present
        if let Some(pos) = list.user_ids.iter().position(|&id| id == user_id) {
            list.user_ids.remove(pos);
            list.version += 1;
            
            self.doc_store
                .put(&list_path, &list, PutOptions::default())
                .await
                .map_err(|e| UserRepositoryError::Storage(e.to_string()))?;
        }
        
        Ok(())
    }
    
    /// Update indexes after user update (if email/username changed)
    async fn update_indexes_for_update(
        &self,
        old_user: &UserDocument,
        new_user: &UserDocument,
    ) -> Result<(), UserRepositoryError> {
        // Update email index if changed
        if old_user.email.to_lowercase() != new_user.email.to_lowercase() {
            // Delete old index - best effort, may not exist
            let old_email_path = self.path_builder.email_index_path(&old_user.email);
            if let Err(e) = self.doc_store.delete(&old_email_path).await {
                tracing::debug!(path = %old_email_path, error = %e, "failed to delete old email index");
            }
            
            // Create new index
            let email_entry = EmailIndexEntry {
                email: new_user.email.clone(),
                user_id: new_user.id,
            };
            let new_email_path = self.path_builder.email_index_path(&new_user.email);
            self.doc_store
                .put(&new_email_path, &email_entry, PutOptions::default())
                .await
                .map_err(|e| UserRepositoryError::Storage(e.to_string()))?;
        }
        
        // Update username index if changed
        if old_user.username.to_lowercase() != new_user.username.to_lowercase() {
            // Delete old index - best effort, may not exist
            let old_username_path = self.path_builder.username_index_path(&old_user.username);
            if let Err(e) = self.doc_store.delete(&old_username_path).await {
                tracing::debug!(path = %old_username_path, error = %e, "failed to delete old username index");
            }
            
            // Create new index
            let username_entry = UsernameIndexEntry {
                username: new_user.username.clone(),
                user_id: new_user.id,
            };
            let new_username_path = self.path_builder.username_index_path(&new_user.username);
            self.doc_store
                .put(&new_username_path, &username_entry, PutOptions::default())
                .await
                .map_err(|e| UserRepositoryError::Storage(e.to_string()))?;
        }
        
        Ok(())
    }
}

#[async_trait]
impl UserRepository for RustFsUserRepository {
    async fn create_user(&self, user: &User) -> Result<(), UserRepositoryError> {
        let doc = super::conversions::user_to_doc(user);
        let user_path = self.path_builder.user_path(user.id);
        
        // Check if user already exists by email
        if let Some(existing) = self.get_user_by_email(&user.email).await? {
            if existing.id != user.id {
                return Err(UserRepositoryError::DuplicateEmail(user.email.clone()));
            }
        }
        
        // Check if user already exists by username
        if let Some(existing) = self.get_user_by_username(&user.username).await? {
            if existing.id != user.id {
                return Err(UserRepositoryError::DuplicateUsername(user.username.clone()));
            }
        }
        
        // Create user with if-none-match to prevent overwrites
        let opts = PutOptions {
            if_none_match: Some("*".to_string()),
            ..Default::default()
        };
        
        let bytes = serde_json::to_vec(&doc)
            .map_err(|e| UserRepositoryError::Storage(format!("failed to serialize user: {e}")))?;
        
        match self.doc_store.put_raw(&user_path, &bytes, opts).await {
            Ok(_) => {
                // Update indexes
                self.update_indexes_for_create(&doc).await?;
                Ok(())
            }
            Err(e) => {
                // Check if this is a precondition/conflict error (document already exists)
                // Both S3 (PreconditionFailed, 409) and LocalFs ("Precondition failed") return similar messages
                let err_str = e.to_string();
                let is_precondition_failed = err_str.contains("Precondition")
                    || err_str.contains("412")  // HTTP 412 Precondition Failed
                    || err_str.contains("409")  // HTTP 409 Conflict
                    || err_str.contains("AlreadyExists");
                
                if is_precondition_failed {
                    Err(UserRepositoryError::Conflict)
                } else {
                    Err(UserRepositoryError::Storage(err_str))
                }
            }
        }
    }
    
    async fn get_user_by_id(&self, id: Uuid) -> Result<Option<User>, UserRepositoryError> {
        let user_path = self.path_builder.user_path(id);
        
        match self.doc_store.get::<UserDocument>(&user_path).await {
            Ok(Some((doc, _))) => Ok(Some(super::conversions::doc_to_user(doc))),
            Ok(None) => Ok(None),
            Err(e) => Err(UserRepositoryError::Storage(e.to_string())),
        }
    }
    
    async fn get_user_by_email(&self, email: &str) -> Result<Option<User>, UserRepositoryError> {
        let email_path = self.path_builder.email_index_path(email);
        
        // Get email index entry
        let entry: Option<(EmailIndexEntry, _)> = self
            .doc_store
            .get(&email_path)
            .await
            .map_err(|e| UserRepositoryError::Storage(e.to_string()))?;
        
        if let Some((entry, _)) = entry {
            // Verify email matches (case-insensitive)
            if entry.email.to_lowercase() == email.to_lowercase() {
                return self.get_user_by_id(entry.user_id).await;
            }
        }
        
        Ok(None)
    }
    
    async fn get_user_by_username(&self, username: &str) -> Result<Option<User>, UserRepositoryError> {
        let username_path = self.path_builder.username_index_path(username);
        
        // Get username index entry
        let entry: Option<(UsernameIndexEntry, _)> = self
            .doc_store
            .get(&username_path)
            .await
            .map_err(|e| UserRepositoryError::Storage(e.to_string()))?;
        
        if let Some((entry, _)) = entry {
            // Verify username matches (case-insensitive)
            if entry.username.to_lowercase() == username.to_lowercase() {
                return self.get_user_by_id(entry.user_id).await;
            }
        }
        
        Ok(None)
    }
    
    async fn update_user(&self, user: &User) -> Result<(), UserRepositoryError> {
        let user_path = self.path_builder.user_path(user.id);
        
        // Get existing user for index update
        let existing = self
            .get_user_by_id(user.id)
            .await?
            .ok_or(UserRepositoryError::NotFound(user.id))?;
        
        let old_doc = super::conversions::user_to_doc(&existing);
        let mut new_doc = super::conversions::user_to_doc(user);
        new_doc.version = old_doc.version + 1;
        new_doc.updated_at = chrono::Utc::now();
        
        // Update user
        self.doc_store
            .put(&user_path, &new_doc, PutOptions::default())
            .await
            .map_err(|e| UserRepositoryError::Storage(e.to_string()))?;
        
        // Update indexes
        self.update_indexes_for_update(&old_doc, &new_doc).await?;
        
        Ok(())
    }
    
    async fn delete_user(&self, id: Uuid) -> Result<(), UserRepositoryError> {
        let user_path = self.path_builder.user_path(id);
        
        // Get existing user for index cleanup
        let existing = self
            .get_user_by_id(id)
            .await?
            .ok_or(UserRepositoryError::NotFound(id))?;
        
        // Delete user document
        self.doc_store
            .delete(&user_path)
            .await
            .map_err(|e| UserRepositoryError::Storage(e.to_string()))?;
        
        // Delete email index - best effort
        let email_path = self.path_builder.email_index_path(&existing.email);
        if let Err(e) = self.doc_store.delete(&email_path).await {
            tracing::debug!(path = %email_path, error = %e, "failed to delete email index");
        }
        
        // Delete username index - best effort
        let username_path = self.path_builder.username_index_path(&existing.username);
        if let Err(e) = self.doc_store.delete(&username_path).await {
            tracing::debug!(path = %username_path, error = %e, "failed to delete username index");
        }
        
        // Update user list
        self.remove_from_user_list(id).await?;
        
        Ok(())
    }
    
    async fn list_users(
        &self,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<User>, UserRepositoryError> {
        let list_path = self.path_builder.user_list_path();
        
        // Get user list index
        let list: UserListIndex = self
            .doc_store
            .get(&list_path)
            .await
            .map_err(|e| UserRepositoryError::Storage(e.to_string()))?
            .map(|(l, _)| l)
            .unwrap_or_default();
        
        // Apply pagination
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(usize::MAX);
        
        let user_ids: Vec<Uuid> = list
            .user_ids
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect();
        
        // Fetch each user
        let mut users = Vec::new();
        for user_id in user_ids {
            if let Some(user) = self.get_user_by_id(user_id).await? {
                users.push(user);
            }
        }
        
        Ok(users)
    }
    
    async fn has_users(&self) -> Result<bool, UserRepositoryError> {
        let count = self.count_users().await?;
        Ok(count > 0)
    }
    
    async fn count_users(&self) -> Result<usize, UserRepositoryError> {
        let list_path = self.path_builder.user_list_path();
        
        let list: UserListIndex = self
            .doc_store
            .get(&list_path)
            .await
            .map_err(|e| UserRepositoryError::Storage(e.to_string()))?
            .map(|(l, _)| l)
            .unwrap_or_default();
        
        Ok(list.user_ids.len())
    }
}

/// User's group membership list
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct UserGroupsList {
    group_ids: Vec<Uuid>,
    version: u64,
}

/// Group's member list
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct GroupMembersList {
    user_ids: Vec<Uuid>,
    version: u64,
}

#[async_trait]
impl GroupRepo for RustFsUserRepository {
    async fn is_member(&self, user_id: Uuid, group_id: Uuid) -> Result<bool, UserRepositoryError> {
        let user_groups_path = self.path_builder.user_groups_path(user_id);
        
        // Get user's groups list
        let user_groups: UserGroupsList = self
            .doc_store
            .get(&user_groups_path)
            .await
            .map_err(|e| UserRepositoryError::Storage(e.to_string()))?
            .map(|(l, _)| l)
            .unwrap_or_default();
        
        Ok(user_groups.group_ids.contains(&group_id))
    }
    
    async fn get_members(&self, group_id: Uuid) -> Result<Vec<Uuid>, UserRepositoryError> {
        let group_members_path = self.path_builder.group_members_path(group_id);
        
        // Get group's member list
        let members: GroupMembersList = self
            .doc_store
            .get(&group_members_path)
            .await
            .map_err(|e| UserRepositoryError::Storage(e.to_string()))?
            .map(|(l, _)| l)
            .unwrap_or_default();
        
        Ok(members.user_ids)
    }
    
    async fn get_user_groups(&self, user_id: Uuid) -> Result<Vec<Uuid>, UserRepositoryError> {
        let user_groups_path = self.path_builder.user_groups_path(user_id);
        
        // Get user's groups list
        let user_groups: UserGroupsList = self
            .doc_store
            .get(&user_groups_path)
            .await
            .map_err(|e| UserRepositoryError::Storage(e.to_string()))?
            .map(|(l, _)| l)
            .unwrap_or_default();
        
        Ok(user_groups.group_ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata_v2::stores::LocalFsDocumentStore;
    use crate::metadata_v2::MetadataBackendConfig;
    use tempfile::TempDir;

    async fn create_test_repository() -> (RustFsUserRepository, TempDir) {
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
        
        let repo = RustFsUserRepository::new(
            doc_store,
            "apps/rustshare".to_string(),
            "test".to_string(),
        );
        
        (repo, temp_dir)
    }

    fn create_test_user(id: Uuid, username: &str, email: &str) -> User {
        User {
            id,
            username: username.to_string(),
            display_name: "Test User".to_string(),
            email: email.to_string(),
            password_hash: "hash".to_string(),
            is_admin: false,
            disabled_at: None,
            storage_quota: 1000000,
            theme: rustshare_core::domain::Theme::System,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            name: None,
            surname: None,
            avatar_path: None,
            email_sharing_enabled: true,
            tenant_id: Uuid::nil(),
        }
    }

    #[tokio::test]
    async fn test_create_and_get_user() {
        let (repo, _temp) = create_test_repository().await;
        
        let user = create_test_user(Uuid::new_v4(), "testuser", "test@example.com");
        
        // Create user
        repo.create_user(&user).await.unwrap();
        
        // Get by ID
        let found = repo.get_user_by_id(user.id).await.unwrap().unwrap();
        assert_eq!(found.username, "testuser");
        assert_eq!(found.email, "test@example.com");
        
        // Get by email
        let found = repo.get_user_by_email("test@example.com").await.unwrap().unwrap();
        assert_eq!(found.id, user.id);
        
        // Get by username
        let found = repo.get_user_by_username("testuser").await.unwrap().unwrap();
        assert_eq!(found.id, user.id);
    }

    #[tokio::test]
    async fn test_duplicate_email() {
        let (repo, _temp) = create_test_repository().await;
        
        let user1 = create_test_user(Uuid::new_v4(), "user1", "test@example.com");
        let user2 = create_test_user(Uuid::new_v4(), "user2", "test@example.com");
        
        repo.create_user(&user1).await.unwrap();
        
        let result = repo.create_user(&user2).await;
        assert!(matches!(result, Err(UserRepositoryError::DuplicateEmail(_))));
    }

    #[tokio::test]
    async fn test_update_user() {
        let (repo, _temp) = create_test_repository().await;
        
        let mut user = create_test_user(Uuid::new_v4(), "testuser", "test@example.com");
        repo.create_user(&user).await.unwrap();
        
        // Update
        user.display_name = "Updated Name".to_string();
        repo.update_user(&user).await.unwrap();
        
        // Verify
        let found = repo.get_user_by_id(user.id).await.unwrap().unwrap();
        assert_eq!(found.display_name, "Updated Name");
    }

    #[tokio::test]
    async fn test_delete_user() {
        let (repo, _temp) = create_test_repository().await;
        
        let user = create_test_user(Uuid::new_v4(), "testuser", "test@example.com");
        repo.create_user(&user).await.unwrap();
        
        // Delete
        repo.delete_user(user.id).await.unwrap();
        
        // Verify deleted
        let found = repo.get_user_by_id(user.id).await.unwrap();
        assert!(found.is_none());
        
        // Index should also be cleaned up
        let found = repo.get_user_by_email("test@example.com").await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_list_users() {
        let (repo, _temp) = create_test_repository().await;
        
        // Create multiple users
        for i in 0..5 {
            let user = create_test_user(
                Uuid::new_v4(),
                &format!("user{}", i),
                &format!("user{}@example.com", i),
            );
            repo.create_user(&user).await.unwrap();
        }
        
        // List all
        let users = repo.list_users(None, None).await.unwrap();
        assert_eq!(users.len(), 5);
        
        // List with limit
        let users = repo.list_users(Some(2), None).await.unwrap();
        assert_eq!(users.len(), 2);
        
        // List with offset
        let users = repo.list_users(Some(2), Some(2)).await.unwrap();
        assert_eq!(users.len(), 2);
    }

    #[tokio::test]
    async fn test_has_users_and_count() {
        let (repo, _temp) = create_test_repository().await;
        
        assert!(!repo.has_users().await.unwrap());
        assert_eq!(repo.count_users().await.unwrap(), 0);
        
        let user = create_test_user(Uuid::new_v4(), "testuser", "test@example.com");
        repo.create_user(&user).await.unwrap();
        
        assert!(repo.has_users().await.unwrap());
        assert_eq!(repo.count_users().await.unwrap(), 1);
    }
}
