use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Folder {
    pub id: Uuid,
    pub name: String,
    pub parent_folder_id: Option<Uuid>,
    pub owner_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Folder {
    pub fn new_root(owner_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: "Root".to_string(),
            parent_folder_id: None,
            owner_id,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    pub fn new_child(name: String, parent_folder_id: Uuid, owner_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            parent_folder_id: Some(parent_folder_id),
            owner_id,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_root_folder_creation() {
        let owner_id = Uuid::new_v4();
        let folder = Folder::new_root(owner_id);

        assert_eq!(folder.name, "Root");
        assert_eq!(folder.parent_folder_id, None);
        assert_eq!(folder.owner_id, owner_id);
    }

    #[test]
    fn test_child_folder_creation() {
        let owner_id = Uuid::new_v4();
        let parent_id = Uuid::new_v4();
        let folder = Folder::new_child("Documents".to_string(), parent_id, owner_id);

        assert_eq!(folder.name, "Documents");
        assert_eq!(folder.parent_folder_id, Some(parent_id));
        assert_eq!(folder.owner_id, owner_id);
    }
}
