//! User repository for zero-PostgreSQL user management

use async_trait::async_trait;
use rustshare_core::domain::User;
use thiserror::Error;
use uuid::Uuid;

pub mod rustfs;

pub use rustfs::RustFsUserRepository;

/// Errors that can occur in user repository operations
#[derive(Debug, Error)]
pub enum UserRepositoryError {
    #[error("User not found: {0}")]
    NotFound(Uuid),
    
    #[error("User with email {0} already exists")]
    DuplicateEmail(String),
    
    #[error("User with username {0} already exists")]
    DuplicateUsername(String),
    
    #[error("Storage error: {0}")]
    Storage(String),
    
    #[error("Concurrency conflict")]
    Conflict,
}

/// User repository trait
#[async_trait]
pub trait UserRepository: Send + Sync {
    /// Create a new user
    async fn create_user(&self, user: &User) -> Result<(), UserRepositoryError>;
    
    /// Get user by ID
    async fn get_user_by_id(&self, id: Uuid) -> Result<Option<User>, UserRepositoryError>;
    
    /// Get user by email
    async fn get_user_by_email(&self, email: &str) -> Result<Option<User>, UserRepositoryError>;
    
    /// Get user by username
    async fn get_user_by_username(&self, username: &str) -> Result<Option<User>, UserRepositoryError>;
    
    /// Update a user
    async fn update_user(&self, user: &User) -> Result<(), UserRepositoryError>;
    
    /// Delete a user
    async fn delete_user(&self, id: Uuid) -> Result<(), UserRepositoryError>;
    
    /// List all users (with optional pagination)
    async fn list_users(
        &self,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<User>, UserRepositoryError>;
    
    /// Check if any users exist
    async fn has_users(&self) -> Result<bool, UserRepositoryError>;
    
    /// Count total users
    async fn count_users(&self) -> Result<usize, UserRepositoryError>;
}

/// Converts between domain User and UserDocument
pub mod conversions {
    use super::*;
    use crate::metadata_v2::schemas::UserDocument;
    
    /// Convert UserDocument to domain User
    pub fn doc_to_user(doc: UserDocument) -> User {
        User {
            id: doc.id,
            username: doc.username,
            display_name: doc.display_name,
            email: doc.email,
            password_hash: doc.password_hash,
            is_admin: doc.is_admin,
            disabled_at: doc.disabled_at,
            storage_quota: doc.storage_quota_bytes,
            theme: doc.theme.parse().unwrap_or(rustshare_core::domain::Theme::System),
            created_at: doc.created_at,
            updated_at: doc.updated_at,
            name: None,
            surname: None,
            avatar_path: None,
            email_sharing_enabled: true,
            tenant_id: doc.tenant_id,
        }
    }
    
    /// Convert domain User to UserDocument
    pub fn user_to_doc(user: &User) -> UserDocument {
        UserDocument {
            schema_version: crate::metadata_v2::schemas::CURRENT_SCHEMA_VERSION,
            id: user.id,
            username: user.username.clone(),
            display_name: user.display_name.clone(),
            email: user.email.clone(),
            password_hash: user.password_hash.clone(),
            is_admin: user.is_admin,
            disabled: user.disabled_at.is_some(),
            disabled_at: user.disabled_at,
            disabled_reason: None, // Not stored in domain User
            storage_quota_bytes: user.storage_quota,
            theme: user.theme.to_string(),
            email_verified_at: None, // Not in domain User
            created_at: user.created_at,
            updated_at: user.updated_at,
            tenant_id: user.tenant_id,
            version: 1, // Will be managed by repository
        }
    }
}
