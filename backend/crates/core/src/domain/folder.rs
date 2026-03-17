use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{FolderId, UserId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Folder {
    pub id: FolderId,
    pub name: String,
    pub path: String,
    pub parent_folder_id: Option<FolderId>,
    pub owner_id: UserId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Folder {
    pub fn new_root(owner_id: UserId) -> Self {
        use uuid::Uuid;
        Self {
            id: Uuid::new_v4(),
            name: "Root".to_string(),
            path: "/".to_string(),
            parent_folder_id: None,
            owner_id,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    pub fn new_child(name: String, path: String, parent_folder_id: FolderId, owner_id: UserId) -> Self {
        use uuid::Uuid;
        Self {
            id: Uuid::new_v4(),
            name,
            path,
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
    use uuid::Uuid;

    #[test]
    fn test_root_folder_creation() {
        let owner_id = Uuid::new_v4();
        let folder = Folder::new_root(owner_id);

        assert_eq!(folder.name, "Root");
        assert_eq!(folder.path, "/");
        assert_eq!(folder.parent_folder_id, None);
        assert_eq!(folder.owner_id, owner_id);
    }

    #[test]
    fn test_child_folder_creation() {
        let owner_id = Uuid::new_v4();
        let parent_id = Uuid::new_v4();
        let folder = Folder::new_child(
            "Documents".to_string(),
            "/Documents".to_string(),
            parent_id,
            owner_id,
        );

        assert_eq!(folder.name, "Documents");
        assert_eq!(folder.path, "/Documents");
        assert_eq!(folder.parent_folder_id, Some(parent_id));
        assert_eq!(folder.owner_id, owner_id);
    }
}
