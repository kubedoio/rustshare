use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct File {
    pub id: Uuid,
    pub name: String,
    pub content_hash: String,
    pub size_bytes: i64,
    pub mime_type: String,
    pub parent_folder_id: Uuid,
    pub owner_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl File {
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
            content_hash: "abc123def456".to_string(),
            size_bytes: 1024,
            mime_type: "application/pdf".to_string(),
            parent_folder_id: Uuid::new_v4(),
            owner_id: Uuid::new_v4(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        assert_eq!(file.storage_key(), "blobs/abc123def456");
    }

    #[test]
    fn test_content_addressed_storage() {
        let hash = "sha256_content_hash";
        let file = File {
            id: Uuid::new_v4(),
            name: "test.txt".to_string(),
            content_hash: hash.to_string(),
            size_bytes: 100,
            mime_type: "text/plain".to_string(),
            parent_folder_id: Uuid::new_v4(),
            owner_id: Uuid::new_v4(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        assert_eq!(file.storage_key(), format!("blobs/{}", hash));
    }
}
