use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    pub tenant_id: Uuid,
    /// Ancestor folder IDs (parent, grandparent, etc.) for efficient permission resolution.
    /// Stored as Option for backward compatibility with folders created before this field existed.
    pub ancestor_ids: Option<Vec<FolderId>>,
}

impl Folder {
    /// Creates a new root folder for a user with a specific name.
    ///
    /// The name can be any type that converts into a String,
    /// such as `&str` or `String`.
    pub fn new_root_with_name(name: impl Into<String>, owner_id: UserId, tenant_id: Uuid) -> Self {
        let name = name.into();
        let path = format!("/{}", name);
        Self {
            id: Uuid::new_v4(),
            name,
            path,
            parent_folder_id: None,
            owner_id,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            tenant_id,
            ancestor_ids: Some(Vec::new()), // Root has no ancestors
        }
    }

    /// Creates a new root folder for a user with default "Root" name.
    /// Deprecated: Use new_root_with_name for user-created root folders.
    pub fn new_root(owner_id: UserId, tenant_id: Uuid) -> Self {
        Self::new_root_with_name("Root".to_string(), owner_id, tenant_id)
    }

    /// Creates a new subfolder under a parent folder.
    /// Note: ancestor_ids should be computed from parent's ancestor_ids + parent.id
    pub fn new_child(
        name: String,
        path: String,
        parent_folder_id: FolderId,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            path,
            parent_folder_id: Some(parent_folder_id),
            owner_id,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            tenant_id,
            ancestor_ids: None, // Must be set by caller using parent's ancestor_ids
        }
    }

    /// Creates a new subfolder with proper ancestor_ids computed.
    pub fn new_child_with_ancestors(
        name: String,
        path: String,
        parent_folder_id: FolderId,
        parent_ancestor_ids: Option<&[FolderId]>,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Self {
        // Build ancestor_ids: parent's ancestors + parent_id
        let mut ancestor_ids = parent_ancestor_ids
            .map(|ids| ids.to_vec())
            .unwrap_or_default();
        ancestor_ids.push(parent_folder_id);

        Self {
            id: Uuid::new_v4(),
            name,
            path,
            parent_folder_id: Some(parent_folder_id),
            owner_id,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            tenant_id,
            ancestor_ids: Some(ancestor_ids),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_root_folder_creation() {
        let owner_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let folder = Folder::new_root(owner_id, tenant_id);

        assert_eq!(folder.name, "Root");
        assert_eq!(folder.path, "/Root");
        assert_eq!(folder.parent_folder_id, None);
        assert_eq!(folder.owner_id, owner_id);
        assert_eq!(folder.tenant_id, tenant_id);
    }

    #[test]
    fn test_root_folder_with_name() {
        let owner_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let folder = Folder::new_root_with_name("Projects".to_string(), owner_id, tenant_id);

        assert_eq!(folder.name, "Projects");
        assert_eq!(folder.path, "/Projects");
        assert_eq!(folder.parent_folder_id, None);
        assert_eq!(folder.owner_id, owner_id);
        assert_eq!(folder.tenant_id, tenant_id);
    }

    #[test]
    fn test_child_folder_creation() {
        let owner_id = Uuid::new_v4();
        let parent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let folder = Folder::new_child(
            "Documents".to_string(),
            "/Documents".to_string(),
            parent_id,
            owner_id,
            tenant_id,
        );

        assert_eq!(folder.name, "Documents");
        assert_eq!(folder.path, "/Documents");
        assert_eq!(folder.parent_folder_id, Some(parent_id));
        assert_eq!(folder.owner_id, owner_id);
        assert_eq!(folder.tenant_id, tenant_id);
    }
}
