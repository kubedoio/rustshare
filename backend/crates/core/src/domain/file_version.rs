use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{FileId, UserId, VersionId};

/// A version snapshot of a file's content.
///
/// Each time a file is modified, a new version is created with the updated content hash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileVersion {
    pub id: VersionId,
    pub file_id: FileId,
    pub version_number: i32,
    pub content_hash: String,
    pub size: i64,
    pub change_description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub created_by: UserId,
}

impl FileVersion {
    /// Creates a new file version snapshot.
    pub fn new(
        file_id: FileId,
        version_number: i32,
        content_hash: String,
        size: i64,
        created_by: UserId,
        change_description: Option<String>,
    ) -> Self {
        use uuid::Uuid;
        Self {
            id: Uuid::new_v4(),
            file_id,
            version_number,
            content_hash,
            size,
            created_at: Utc::now(),
            created_by,
            change_description,
        }
    }

    /// Returns the object storage key for this version's content.
    pub fn storage_key(&self) -> String {
        format!("blobs/{}", self.content_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_file_version_storage_key() {
        let version = FileVersion {
            id: Uuid::new_v4(),
            file_id: Uuid::new_v4(),
            version_number: 1,
            content_hash: "def789ghi012".to_string(),
            size: 2048,
            change_description: Some("Initial version".to_string()),
            created_at: Utc::now(),
            created_by: Uuid::new_v4(),
        };

        assert_eq!(version.storage_key(), "blobs/def789ghi012");
    }

    #[test]
    fn test_file_version_new_constructor() {
        let file_id = Uuid::new_v4();
        let created_by = Uuid::new_v4();

        let version = FileVersion::new(
            file_id,
            1,
            "def789ghi012".to_string(),
            2048,
            created_by,
            Some("Initial version".to_string()),
        );

        assert_eq!(version.file_id, file_id);
        assert_eq!(version.version_number, 1);
        assert_eq!(version.content_hash, "def789ghi012");
        assert_eq!(version.size, 2048);
        assert_eq!(version.created_by, created_by);
        assert_eq!(version.change_description, Some("Initial version".to_string()));
        assert!(!version.id.is_nil());
    }
}
