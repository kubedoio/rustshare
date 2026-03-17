use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Share {
    pub id: Uuid,
    pub file_id: Uuid,
    pub token: String,
    pub password_hash: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
}

impl Share {
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() > expires_at
        } else {
            false
        }
    }

    pub fn is_password_protected(&self) -> bool {
        self.password_hash.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_share_not_expired() {
        let share = Share {
            id: Uuid::new_v4(),
            file_id: Uuid::new_v4(),
            token: "abc123".to_string(),
            password_hash: None,
            expires_at: Some(Utc::now() + Duration::hours(1)),
            created_by: Uuid::new_v4(),
            created_at: Utc::now(),
        };

        assert!(!share.is_expired());
    }

    #[test]
    fn test_share_expired() {
        let share = Share {
            id: Uuid::new_v4(),
            file_id: Uuid::new_v4(),
            token: "abc123".to_string(),
            password_hash: None,
            expires_at: Some(Utc::now() - Duration::hours(1)),
            created_by: Uuid::new_v4(),
            created_at: Utc::now(),
        };

        assert!(share.is_expired());
    }

    #[test]
    fn test_share_never_expires() {
        let share = Share {
            id: Uuid::new_v4(),
            file_id: Uuid::new_v4(),
            token: "abc123".to_string(),
            password_hash: None,
            expires_at: None,
            created_by: Uuid::new_v4(),
            created_at: Utc::now(),
        };

        assert!(!share.is_expired());
    }

    #[test]
    fn test_password_protected_share() {
        let share = Share {
            id: Uuid::new_v4(),
            file_id: Uuid::new_v4(),
            token: "abc123".to_string(),
            password_hash: Some("hashed_password".to_string()),
            expires_at: None,
            created_by: Uuid::new_v4(),
            created_at: Utc::now(),
        };

        assert!(share.is_password_protected());
    }
}
