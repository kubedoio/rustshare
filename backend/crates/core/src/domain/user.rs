use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub password_hash: String,
    pub email: String,
    pub is_admin: bool,
    pub created_at: DateTime<Utc>,
}

impl User {
    pub fn new(username: String, password_hash: String, email: String, is_admin: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            username,
            password_hash,
            email,
            is_admin,
            created_at: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_creation() {
        let user = User::new(
            "alice".to_string(),
            "hashed_password".to_string(),
            "alice@example.com".to_string(),
            false,
        );

        assert_eq!(user.username, "alice");
        assert_eq!(user.email, "alice@example.com");
        assert!(!user.is_admin);
        assert!(!user.id.is_nil());
    }

    #[test]
    fn test_admin_user_creation() {
        let admin = User::new(
            "admin".to_string(),
            "hashed_password".to_string(),
            "admin@example.com".to_string(),
            true,
        );

        assert!(admin.is_admin);
    }
}
