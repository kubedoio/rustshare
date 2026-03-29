use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{FileId, FolderId, ShareId, UserId};

/// Permission level for a share link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
pub enum SharePermissions {
    /// Read-only access (download files, view folder contents)
    View,
    /// View + upload new versions, create files/folders
    Edit,
    /// Edit + manage recipients (add/remove, change permissions)
    Admin,
}

impl SharePermissions {
    /// Returns numeric level for comparison (View=1, Edit=2, Admin=3)
    pub fn level(&self) -> u8 {
        match self {
            Self::View => 1,
            Self::Edit => 2,
            Self::Admin => 3,
        }
    }

    /// Returns the highest permission from a list
    pub fn max(permissions: &[SharePermissions]) -> SharePermissions {
        permissions
            .iter()
            .max_by_key(|p| p.level())
            .copied()
            .unwrap_or(SharePermissions::View)
    }
}

impl PartialOrd for SharePermissions {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SharePermissions {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.level().cmp(&other.level())
    }
}

/// A share link that allows access to a file or folder.
///
/// Supports both public shares (anonymous access via token), user shares
/// (authenticated user-to-user sharing), and group shares (access via group membership).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Share {
    pub id: ShareId,
    /// File being shared (None for folder shares)
    pub file_id: Option<FileId>,
    /// Folder being shared (None for file shares)
    pub folder_id: Option<FolderId>,
    /// Token for public shares (None for user shares)
    pub share_token: Option<String>,
    pub permissions: SharePermissions,
    /// Password hash for public shares only
    pub password_hash: Option<String>,
    /// Expiration time for public shares only
    pub expires_at: Option<DateTime<Utc>>,
    /// Public folder share that allows uploads but not browsing/downloads.
    pub upload_only: bool,
    /// Access count for public shares only
    pub access_count: i32,
    /// Recipient user for user shares (None for public/group shares)
    pub recipient_user_id: Option<UserId>,
    /// Recipient group for group shares (None for public/user shares)
    pub recipient_group_id: Option<Uuid>,
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub tenant_id: Uuid,
}

impl Share {
    /// Creates a new public share link for a file (Phase 3B compatibility).
    pub fn new(
        file_id: FileId,
        share_token: String,
        created_by: UserId,
        permissions: SharePermissions,
        password_hash: Option<String>,
        expires_at: Option<DateTime<Utc>>,
        tenant_id: Uuid,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            file_id: Some(file_id),
            folder_id: None,
            share_token: Some(share_token),
            password_hash,
            expires_at,
            upload_only: false,
            created_by,
            created_at: Utc::now(),
            permissions,
            access_count: 0,
            recipient_user_id: None,
            recipient_group_id: None,
            revoked_at: None,
            tenant_id,
        }
    }

    /// Creates a new public share link for a folder.
    pub fn new_folder(
        folder_id: FolderId,
        share_token: String,
        created_by: UserId,
        permissions: SharePermissions,
        password_hash: Option<String>,
        expires_at: Option<DateTime<Utc>>,
        tenant_id: Uuid,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            file_id: None,
            folder_id: Some(folder_id),
            share_token: Some(share_token),
            password_hash,
            expires_at,
            upload_only: false,
            created_by,
            created_at: Utc::now(),
            permissions,
            access_count: 0,
            recipient_user_id: None,
            recipient_group_id: None,
            revoked_at: None,
            tenant_id,
        }
    }

    /// Checks if this is a public share (anonymous access)
    pub fn is_public_share(&self) -> bool {
        self.recipient_user_id.is_none()
    }

    /// Checks if this is a user share (authenticated user-to-user)
    pub fn is_user_share(&self) -> bool {
        self.recipient_user_id.is_some()
    }

    /// Checks if this share is for a folder
    pub fn is_folder_share(&self) -> bool {
        self.folder_id.is_some()
    }

    /// Checks if this share is for a file
    pub fn is_file_share(&self) -> bool {
        self.file_id.is_some()
    }

    /// Get the resource being shared (file or folder ID)
    ///
    /// # Safety
    /// This uses expect() which panics if both file_id and folder_id are None.
    /// The database CHECK constraint guarantees this never happens, but callers
    /// in test code should ensure one is set.
    pub fn resource_id(&self) -> uuid::Uuid {
        self.file_id
            .or(self.folder_id)
            .expect("Share must have file_id or folder_id")
    }

    /// Checks if the share link has expired (public shares only).
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() > expires_at
        } else {
            false
        }
    }

    /// Checks if the share link is password-protected (public shares only).
    pub fn is_password_protected(&self) -> bool {
        self.password_hash.is_some()
    }
}

/// Represents a recipient of a share (for API responses).
///
/// Used in GET /api/shares/{id}/recipients endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareRecipient {
    pub share_id: ShareId,
    pub user_id: UserId,
    pub email: String,
    pub permission: SharePermissions,
    pub added_at: DateTime<Utc>,
    pub added_by: UserId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_share_not_expired() {
        let share = Share {
            id: Uuid::new_v4(),
            file_id: Some(Uuid::new_v4()),
            folder_id: None,
            share_token: Some("abc123".to_string()),
            permissions: SharePermissions::View,
            password_hash: None,
            expires_at: Some(Utc::now() + Duration::hours(1)),
            upload_only: false,
            access_count: 0,
            recipient_user_id: None,
            recipient_group_id: None,
            created_by: Uuid::new_v4(),
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id: Uuid::new_v4(),
        };

        assert!(!share.is_expired());
    }

    #[test]
    fn test_share_expired() {
        let share = Share {
            id: Uuid::new_v4(),
            file_id: Some(Uuid::new_v4()),
            folder_id: None,
            share_token: Some("abc123".to_string()),
            permissions: SharePermissions::View,
            password_hash: None,
            expires_at: Some(Utc::now() - Duration::hours(1)),
            upload_only: false,
            access_count: 5,
            recipient_user_id: None,
            recipient_group_id: None,
            created_by: Uuid::new_v4(),
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id: Uuid::new_v4(),
        };

        assert!(share.is_expired());
    }

    #[test]
    fn test_share_never_expires() {
        let share = Share {
            id: Uuid::new_v4(),
            file_id: Some(Uuid::new_v4()),
            folder_id: None,
            share_token: Some("abc123".to_string()),
            permissions: SharePermissions::Edit,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: None,
            recipient_group_id: None,
            created_by: Uuid::new_v4(),
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id: Uuid::new_v4(),
        };

        assert!(!share.is_expired());
    }

    #[test]
    fn test_password_protected_share() {
        let share = Share {
            id: Uuid::new_v4(),
            file_id: Some(Uuid::new_v4()),
            folder_id: None,
            share_token: Some("abc123".to_string()),
            permissions: SharePermissions::View,
            password_hash: Some("hashed_password".to_string()),
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: None,
            recipient_group_id: None,
            created_by: Uuid::new_v4(),
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id: Uuid::new_v4(),
        };

        assert!(share.is_password_protected());
    }

    #[test]
    fn test_share_new_constructor() {
        let file_id = Uuid::new_v4();
        let created_by = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let share = Share::new(
            file_id,
            "abc123".to_string(),
            created_by,
            SharePermissions::View,
            Some("hashed_password".to_string()),
            Some(Utc::now() + Duration::hours(24)),
            tenant_id,
        );

        assert_eq!(share.file_id, Some(file_id));
        assert_eq!(share.share_token, Some("abc123".to_string()));
        assert_eq!(share.created_by, created_by);
        assert_eq!(share.permissions, SharePermissions::View);
        assert_eq!(share.password_hash, Some("hashed_password".to_string()));
        assert!(!share.upload_only);
        assert_eq!(share.access_count, 0);
        assert_eq!(share.tenant_id, tenant_id);
        assert!(!share.id.is_nil());
        assert!(share.is_password_protected());
        assert!(!share.is_expired());
    }

    #[test]
    fn test_share_is_user_share() {
        let share = Share {
            id: Uuid::new_v4(),
            file_id: Some(Uuid::new_v4()),
            folder_id: None,
            share_token: None,
            permissions: SharePermissions::View,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: Some(Uuid::new_v4()),
            recipient_group_id: None,
            created_by: Uuid::new_v4(),
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id: Uuid::new_v4(),
        };

        assert!(share.is_user_share());
        assert!(!share.is_public_share());
        assert!(share.is_file_share());
        assert!(!share.is_folder_share());
    }

    #[test]
    fn test_share_is_folder_share() {
        let share = Share {
            id: Uuid::new_v4(),
            file_id: None,
            folder_id: Some(Uuid::new_v4()),
            share_token: None,
            permissions: SharePermissions::Edit,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: Some(Uuid::new_v4()),
            recipient_group_id: None,
            created_by: Uuid::new_v4(),
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id: Uuid::new_v4(),
        };

        assert!(share.is_folder_share());
        assert!(!share.is_file_share());
    }

    #[test]
    fn test_share_resource_id() {
        let file_id = Uuid::new_v4();
        let share = Share {
            id: Uuid::new_v4(),
            file_id: Some(file_id),
            folder_id: None,
            share_token: None,
            permissions: SharePermissions::View,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: Some(Uuid::new_v4()),
            recipient_group_id: None,
            created_by: Uuid::new_v4(),
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id: Uuid::new_v4(),
        };

        assert_eq!(share.resource_id(), file_id);
    }

    #[test]
    fn test_permission_ordering() {
        assert!(SharePermissions::View < SharePermissions::Edit);
        assert!(SharePermissions::Edit < SharePermissions::Admin);
        assert!(SharePermissions::View < SharePermissions::Admin);
    }

    #[test]
    fn test_permission_max() {
        let perms = vec![
            SharePermissions::View,
            SharePermissions::Admin,
            SharePermissions::Edit,
        ];
        assert_eq!(SharePermissions::max(&perms), SharePermissions::Admin);

        let perms = vec![SharePermissions::View, SharePermissions::Edit];
        assert_eq!(SharePermissions::max(&perms), SharePermissions::Edit);
    }
}
