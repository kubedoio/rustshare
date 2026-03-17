use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::UserId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub username: String,
    pub display_name: String,
    pub password_hash: String,
    pub email: String,
    pub is_admin: bool,
    pub storage_quota: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    pub fn new(username: String, display_name: String, password_hash: String, email: String, is_admin: bool, storage_quota: i64) -> Self {
        use uuid::Uuid;
        Self {
            id: Uuid::new_v4(),
            username,
            display_name,
            password_hash,
            email,
            is_admin,
            storage_quota,
            created_at: Utc::now(),
            updated_at: Utc::now(),
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
            "Alice Smith".to_string(),
            "hashed_password".to_string(),
            "alice@example.com".to_string(),
            false,
            10_737_418_240, // 10 GB
        );

        assert_eq!(user.username, "alice");
        assert_eq!(user.display_name, "Alice Smith");
        assert_eq!(user.email, "alice@example.com");
        assert!(!user.is_admin);
        assert_eq!(user.storage_quota, 10_737_418_240);
        assert!(!user.id.is_nil());
    }

    #[test]
    fn test_admin_user_creation() {
        let admin = User::new(
            "admin".to_string(),
            "Administrator".to_string(),
            "hashed_password".to_string(),
            "admin@example.com".to_string(),
            true,
            107_374_182_400, // 100 GB
        );

        assert!(admin.is_admin);
    }
}
