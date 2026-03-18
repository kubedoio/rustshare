use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{FolderId, UserId};

/// A folder that organizes files in a hierarchical structure.
///
/// Folders form a tree structure where each folder can have a parent folder.
/// The root folder has no parent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
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
    /// Creates a new root folder for a user.
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

    /// Creates a new subfolder under a parent folder.
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
