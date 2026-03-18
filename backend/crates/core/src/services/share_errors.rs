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

    /// File with the given ID was not found.
    #[error("File {0} not found")]
    FileNotFound(Uuid),

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
    Database(#[from] sqlx::Error),

    /// Password hashing operation failed.
    #[error("Password hashing error: {0}")]
    PasswordHash(String),

    /// JWT operation failed.
    #[error("JWT error: {0}")]
    Jwt(String),
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

    // Note: Database error tests removed as they require sqlx::Error which cannot
    // be easily constructed in unit tests. The #[from] attribute ensures proper
    // automatic conversion from sqlx::Error to ShareError::Database.
}
