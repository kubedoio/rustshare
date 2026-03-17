use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileVersion {
    pub id: Uuid,
    pub file_id: Uuid,
    pub version_number: i32,
    pub content_hash: String,
    pub size_bytes: i64,
    pub created_at: DateTime<Utc>,
    pub created_by: Uuid,
}

impl FileVersion {
    pub fn storage_key(&self) -> String {
        format!("blobs/{}", self.content_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_version_storage_key() {
        let version = FileVersion {
            id: Uuid::new_v4(),
            file_id: Uuid::new_v4(),
            version_number: 1,
            content_hash: "def789ghi012".to_string(),
            size_bytes: 2048,
            created_at: Utc::now(),
            created_by: Uuid::new_v4(),
        };

        assert_eq!(version.storage_key(), "blobs/def789ghi012");
    }
}
