use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{FileId, FolderId, UserId};

/// File metadata for a user's uploaded file.
///
/// Files use content-addressed storage where the actual file content is stored
/// in object storage using the content hash as the key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct File {
    pub id: FileId,
    pub name: String,
    pub path: String,
    pub content_hash: String,
    pub size: i64,
    pub mime_type: String,
    pub parent_folder_id: Option<FolderId>,
    pub owner_id: UserId,
    pub current_version: i32,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub starred_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub tenant_id: Uuid,
}

impl File {
    /// Creates a new file with version 1.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        path: String,
        content_hash: String,
        size: i64,
        mime_type: String,
        parent_folder_id: Option<FolderId>,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            path,
            content_hash,
            size,
            mime_type,
            parent_folder_id,
            owner_id,
            current_version: 1,
            created_at: Utc::now(),
            modified_at: Utc::now(),
            starred_at: None,
            deleted_at: None,
            tenant_id,
        }
    }

    /// Returns the object storage key for this file's content.
    pub fn storage_key(&self) -> String {
        format!("blobs/{}", self.content_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_storage_key() {
        let file = File {
            id: Uuid::new_v4(),
            name: "document.pdf".to_string(),
            path: "/Documents/document.pdf".to_string(),
            content_hash: "abc123def456".to_string(),
            size: 1024,
            mime_type: "application/pdf".to_string(),
            parent_folder_id: Some(Uuid::new_v4()),
            owner_id: Uuid::new_v4(),
            current_version: 1,
            created_at: Utc::now(),
            modified_at: Utc::now(),
            starred_at: None,
            deleted_at: None,
            tenant_id: Uuid::new_v4(),
        };

        assert_eq!(file.storage_key(), "blobs/abc123def456");
    }

    #[test]
    fn test_content_addressed_storage() {
        let hash = "sha256_content_hash";
        let file = File {
            id: Uuid::new_v4(),
            name: "test.txt".to_string(),
            path: "/test.txt".to_string(),
            content_hash: hash.to_string(),
            size: 100,
            mime_type: "text/plain".to_string(),
            parent_folder_id: None,
            owner_id: Uuid::new_v4(),
            current_version: 1,
            created_at: Utc::now(),
            modified_at: Utc::now(),
            starred_at: None,
            deleted_at: None,
            tenant_id: Uuid::new_v4(),
        };

        assert_eq!(file.storage_key(), format!("blobs/{}", hash));
    }

    #[test]
    fn test_file_new_constructor() {
        let owner_id = Uuid::new_v4();
        let parent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let file = File::new(
            "document.pdf".to_string(),
            "/Documents/document.pdf".to_string(),
            "abc123def456".to_string(),
            1024,
            "application/pdf".to_string(),
            Some(parent_id),
            owner_id,
            tenant_id,
        );

        assert_eq!(file.name, "document.pdf");
        assert_eq!(file.path, "/Documents/document.pdf");
        assert_eq!(file.content_hash, "abc123def456");
        assert_eq!(file.size, 1024);
        assert_eq!(file.mime_type, "application/pdf");
        assert_eq!(file.parent_folder_id, Some(parent_id));
        assert_eq!(file.owner_id, owner_id);
        assert_eq!(file.tenant_id, tenant_id);
        assert_eq!(file.current_version, 1);
        assert!(!file.id.is_nil());
    }
}
