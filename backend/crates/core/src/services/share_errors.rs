use crate::domain::{SharePermissions, UserId};
use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur during share operations.
#[derive(Debug, Error)]
pub enum ShareError {
    /// Share with the given ID was not found.
    #[error("Share {0} not found")]
    ShareNotFound(Uuid),

    /// Share with the given token was not found.
    #[error("Share with token {0} not found")]
    ShareNotFoundByToken(String),

    /// File with the given ID was not found.
    #[error("File {0} not found")]
    FileNotFound(Uuid),

    /// Folder with the given ID was not found.
    #[error("Folder {0} not found")]
    FolderNotFound(Uuid),

    /// User lacks permission to manage shares for this file.
    #[error("User {user_id} does not have permission to manage shares for file {file_id}")]
    PermissionDenied { file_id: Uuid, user_id: Uuid },

    /// Share has been revoked.
    #[error("Share has been revoked")]
    Revoked,

    /// Share has expired.
    #[error("Share has expired")]
    Expired,

    /// Password is required for this share.
    #[error("Password required for this share")]
    PasswordRequired,

    /// Invalid password provided for this share.
    #[error("Invalid password")]
    InvalidPassword,

    /// Database operation failed.
    #[error("Database error: {0}")]
    Database(String),

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

    /// Share is in an invalid state (invariant violated).
    #[error("Share in invalid state: {0}")]
    InvalidState(String),

    /// Cross-tenant sharing attempted
    #[error("Cross-tenant sharing is not allowed")]
    CrossTenantSharingNotAllowed,

    /// Group not found
    #[error("Group {0} not found")]
    GroupNotFound(Uuid),

    /// User not member of group
    #[error("User is not a member of group {0}")]
    NotGroupMember(Uuid),

    /// Group share already exists
    #[error("Group already has access to this resource")]
    GroupShareAlreadyExists,

    /// Recipient visibility config invalid
    #[error("Invalid recipient visibility: {0}")]
    InvalidRecipientVisibility(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_share_error_share_not_found() {
        let id = Uuid::new_v4();
        let err = ShareError::ShareNotFound(id);
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

    // Note: Database error tests removed as they require sqlx::Error which cannot
    // be easily constructed in unit tests. The #[from] attribute ensures proper
    // automatic conversion from sqlx::Error to ShareError::Database.
}
