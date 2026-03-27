use crate::domain::{SharePermissions, UserId};
use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur during share operations.
#[derive(Debug, Error)]
pub enum ShareError {
    /// Share was not found.
    #[error("Share not found")]
    NotFound,

    /// Share with the given ID was not found.
    #[error("Share {0} not found")]
    NotFoundById(Uuid),

    /// Share with the given ID was not found (V2).
    #[error("Share not found: {0}")]
    ShareNotFound(Uuid),

    /// File with the given ID was not found.
    #[error("File {0} not found")]
    FileNotFound(Uuid),

    /// Resource being shared was not found.
    #[error("Resource not found: {resource_id} (type: {resource_type})")]
    ResourceNotFound { resource_id: Uuid, resource_type: String },

    /// User lacks permission to manage shares for this file.
    #[error("User {user_id} does not have permission to manage shares for file {file_id}")]
    PermissionDenied { file_id: Uuid, user_id: Uuid },

    /// Share has been revoked.
    #[error("Share has been revoked")]
    Revoked,

    /// Share has expired.
    #[error("Share has expired")]
    Expired,

    /// Share has expired (V2 variant with ID).
    #[error("Share expired: {0}")]
    ShareExpired(Uuid),

    /// Password is required for this share.
    #[error("Password required for this share")]
    PasswordRequired,

    /// Invalid password provided for this share.
    #[error("Invalid password")]
    InvalidPassword,

    /// Invalid share operation.
    #[error("Invalid share: {reason}")]
    InvalidShare { reason: String },

    /// Database operation failed.
    #[error("Database error: {0}")]
    Database(String),

    /// Storage operation failed.
    #[error("Storage error: {0}")]
    Storage(String),

    /// Password hashing operation failed.
    #[error("Password hashing error: {0}")]
    PasswordHash(String),

    /// JWT operation failed.
    #[error("JWT error: {0}")]
    Jwt(String),

    /// Recipient user email was not found.
    #[error("Recipient {0} not found")]
    RecipientNotFound(String),

    /// User lacks required permission for this operation.
    #[error("Insufficient permission: required {required:?}, but have {actual:?}")]
    InsufficientPermission {
        required: SharePermissions,
        actual: SharePermissions,
    },

    /// Cannot share a file with oneself.
    #[error("Cannot share a file with yourself")]
    CannotShareWithSelf,

    /// Share already exists for this recipient.
    #[error("Share already exists for user {0}")]
    ShareAlreadyExists(UserId),

    /// Cannot remove the owner from a share.
    #[error("Cannot remove the owner from a share")]
    CannotRemoveOwner,
}

impl From<String> for ShareError {
    fn from(s: String) -> Self {
        ShareError::Database(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_share_error_not_found() {
        let err = ShareError::NotFound;
        assert_eq!(err.to_string(), "Share not found");
    }

    #[test]
    fn test_share_error_not_found_by_id() {
        let id = Uuid::new_v4();
        let err = ShareError::NotFoundById(id);
        assert_eq!(err.to_string(), format!("Share {} not found", id));
    }

    #[test]
    fn test_share_error_file_not_found() {
        let id = Uuid::new_v4();
        let err = ShareError::FileNotFound(id);
        assert_eq!(err.to_string(), format!("File {} not found", id));
    }

    #[test]
    fn test_share_error_permission_denied() {
        let file_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let err = ShareError::PermissionDenied { file_id, user_id };
        let msg = err.to_string();
        assert!(msg.contains("does not have permission to manage shares for file"));
        assert!(msg.contains(&file_id.to_string()));
        assert!(msg.contains(&user_id.to_string()));
    }

    #[test]
    fn test_share_error_revoked() {
        let err = ShareError::Revoked;
        assert_eq!(err.to_string(), "Share has been revoked");
    }

    #[test]
    fn test_share_error_expired() {
        let err = ShareError::Expired;
        assert_eq!(err.to_string(), "Share has expired");
    }

    #[test]
    fn test_share_error_password_required() {
        let err = ShareError::PasswordRequired;
        assert_eq!(err.to_string(), "Password required for this share");
    }

    #[test]
    fn test_share_error_invalid_password() {
        let err = ShareError::InvalidPassword;
        assert_eq!(err.to_string(), "Invalid password");
    }

    #[test]
    fn test_share_error_password_hash() {
        let msg = "bcrypt error";
        let err = ShareError::PasswordHash(msg.to_string());
        assert_eq!(err.to_string(), format!("Password hashing error: {}", msg));
    }

    #[test]
    fn test_share_error_jwt() {
        let msg = "token validation failed";
        let err = ShareError::Jwt(msg.to_string());
        assert_eq!(err.to_string(), format!("JWT error: {}", msg));
    }

    #[test]
    fn test_share_error_recipient_not_found() {
        let email = "nonexistent@example.com";
        let err = ShareError::RecipientNotFound(email.to_string());
        assert_eq!(err.to_string(), format!("Recipient {} not found", email));
    }

    #[test]
    fn test_share_error_insufficient_permission() {
        let err = ShareError::InsufficientPermission {
            required: SharePermissions::Admin,
            actual: SharePermissions::View,
        };
        let msg = err.to_string();
        assert!(msg.contains("Insufficient permission"));
        assert!(msg.contains("required"));
        assert!(msg.contains("have"));
    }

    #[test]
    fn test_share_error_cannot_share_with_self() {
        let err = ShareError::CannotShareWithSelf;
        assert_eq!(err.to_string(), "Cannot share a file with yourself");
    }

    #[test]
    fn test_share_error_share_already_exists() {
        let user_id = Uuid::new_v4();
        let err = ShareError::ShareAlreadyExists(user_id);
        let msg = err.to_string();
        assert!(msg.contains("Share already exists for user"));
        assert!(msg.contains(&user_id.to_string()));
    }

    #[test]
    fn test_share_error_cannot_remove_owner() {
        let err = ShareError::CannotRemoveOwner;
        assert_eq!(err.to_string(), "Cannot remove the owner from a share");
    }

    #[test]
    fn test_share_error_database() {
        let msg = "connection failed";
        let err = ShareError::Database(msg.to_string());
        assert_eq!(err.to_string(), format!("Database error: {}", msg));
    }
}
