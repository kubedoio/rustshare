use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{FileId, ShareId, UserId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SharePermissions {
    Read,
    ReadWrite,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Share {
    pub id: ShareId,
    pub file_id: FileId,
    pub share_token: String,
    pub permissions: SharePermissions,
    pub password_hash: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub access_count: i32,
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
}

impl Share {
    pub fn new(
        file_id: FileId,
        share_token: String,
        created_by: UserId,
        permissions: SharePermissions,
        password_hash: Option<String>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Self {
        use uuid::Uuid;
        Self {
            id: Uuid::new_v4(),
            file_id,
            share_token,
            password_hash,
            expires_at,
            created_by,
            created_at: Utc::now(),
            permissions,
            access_count: 0,
        }
    }

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
    use uuid::Uuid;

    #[test]
    fn test_share_not_expired() {
        let share = Share {
            id: Uuid::new_v4(),
            file_id: Uuid::new_v4(),
            share_token: "abc123".to_string(),
            permissions: SharePermissions::Read,
            password_hash: None,
            expires_at: Some(Utc::now() + Duration::hours(1)),
            access_count: 0,
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
            share_token: "abc123".to_string(),
            permissions: SharePermissions::Read,
            password_hash: None,
            expires_at: Some(Utc::now() - Duration::hours(1)),
            access_count: 5,
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
            share_token: "abc123".to_string(),
            permissions: SharePermissions::ReadWrite,
            password_hash: None,
            expires_at: None,
            access_count: 0,
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
            share_token: "abc123".to_string(),
            permissions: SharePermissions::Read,
            password_hash: Some("hashed_password".to_string()),
            expires_at: None,
            access_count: 0,
            created_by: Uuid::new_v4(),
            created_at: Utc::now(),
        };

        assert!(share.is_password_protected());
    }

    #[test]
    fn test_share_new_constructor() {
        let file_id = Uuid::new_v4();
        let created_by = Uuid::new_v4();

        let share = Share::new(
            file_id,
            "abc123".to_string(),
            created_by,
            SharePermissions::Read,
            Some("hashed_password".to_string()),
            Some(Utc::now() + Duration::hours(24)),
        );

        assert_eq!(share.file_id, file_id);
        assert_eq!(share.share_token, "abc123");
        assert_eq!(share.created_by, created_by);
        assert_eq!(share.permissions, SharePermissions::Read);
        assert_eq!(share.password_hash, Some("hashed_password".to_string()));
        assert_eq!(share.access_count, 0);
        assert!(!share.id.is_nil());
        assert!(share.is_password_protected());
        assert!(!share.is_expired());
    }
}
