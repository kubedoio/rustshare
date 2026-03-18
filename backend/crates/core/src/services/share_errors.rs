use std::fmt;
use uuid::Uuid;

#[derive(Debug)]
pub enum ShareError {
    NotFound,
    NotFoundById(Uuid),
    FileNotFound(Uuid),
    PermissionDenied { file_id: Uuid, user_id: Uuid },
    Revoked,
    Expired,
    PasswordRequired,
    InvalidPassword,
    Database(sqlx::Error),
    PasswordHash(String),
    Jwt(String),
}

impl fmt::Display for ShareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShareError::NotFound => write!(f, "Share not found"),
            ShareError::NotFoundById(id) => write!(f, "Share {} not found", id),
            ShareError::FileNotFound(id) => write!(f, "File {} not found", id),
            ShareError::PermissionDenied { file_id, user_id } => {
                write!(f, "User {} does not have permission to manage shares for file {}", user_id, file_id)
            }
            ShareError::Revoked => write!(f, "Share has been revoked"),
            ShareError::Expired => write!(f, "Share has expired"),
            ShareError::PasswordRequired => write!(f, "Password required for this share"),
            ShareError::InvalidPassword => write!(f, "Invalid password"),
            ShareError::Database(err) => write!(f, "Database error: {}", err),
            ShareError::PasswordHash(msg) => write!(f, "Password hashing error: {}", msg),
            ShareError::Jwt(msg) => write!(f, "JWT error: {}", msg),
        }
    }
}

impl std::error::Error for ShareError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_share_error_display() {
        let err = ShareError::NotFound;
        assert_eq!(err.to_string(), "Share not found");

        let err = ShareError::Revoked;
        assert_eq!(err.to_string(), "Share has been revoked");

        let err = ShareError::PasswordRequired;
        assert_eq!(err.to_string(), "Password required for this share");
    }
}
