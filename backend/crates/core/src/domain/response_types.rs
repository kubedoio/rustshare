use serde::{Deserialize, Serialize};

use super::{File, Folder};

/// A flat representation of folder contents.
///
/// Contains the files and immediate subfolders in a directory,
/// without recursing into subdirectories.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FolderContents {
    pub files: Vec<File>,
    pub folders: Vec<Folder>,
}

impl FolderContents {
    /// Creates a new FolderContents with empty files and folders.
    pub fn new() -> Self {
        Self {
            files: vec![],
            folders: vec![],
        }
    }

    /// Creates a new FolderContents with the given files and folders.
    pub fn with_contents(files: Vec<File>, folders: Vec<Folder>) -> Self {
        Self { files, folders }
    }
}

impl Default for FolderContents {
    fn default() -> Self {
        Self::new()
    }
}

/// A recursive representation of folder structure.
///
/// Contains a folder and all its subfolders recursively, along with files
/// at each level. This provides a complete tree view of the folder hierarchy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FolderTree {
    pub folder: Folder,
    pub subfolders: Vec<FolderTree>,
    pub files: Vec<File>,
}

impl FolderTree {
    /// Creates a new FolderTree with the given folder and empty files/subfolders.
    pub fn new(folder: Folder) -> Self {
        Self {
            folder,
            subfolders: vec![],
            files: vec![],
        }
    }

    /// Creates a new FolderTree with the given folder, files, and subfolders.
    pub fn with_contents(folder: Folder, files: Vec<File>, subfolders: Vec<FolderTree>) -> Self {
        Self {
            folder,
            subfolders,
            files,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    #[test]
    fn test_folder_contents_structure() {
        let contents = FolderContents {
            files: vec![],
            folders: vec![],
        };
        assert_eq!(contents.files.len(), 0);
        assert_eq!(contents.folders.len(), 0);
    }

    #[test]
    fn test_folder_contents_new() {
        let contents = FolderContents::new();
        assert_eq!(contents.files.len(), 0);
        assert_eq!(contents.folders.len(), 0);
    }

    #[test]
    fn test_folder_contents_default() {
        let contents = FolderContents::default();
        assert_eq!(contents.files.len(), 0);
        assert_eq!(contents.folders.len(), 0);
    }

    #[test]
    fn test_folder_contents_with_files() {
        let owner_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let file = File {
            id: Uuid::new_v4(),
            name: "test.txt".to_string(),
            path: "/test.txt".to_string(),
            content_hash: "hash123".to_string(),
            size: 100,
            mime_type: "text/plain".to_string(),
            parent_folder_id: None,
            owner_id,
            current_version: 1,
            created_at: Utc::now(),
            modified_at: Utc::now(),
            starred_at: None,
            deleted_at: None,
            tenant_id,
        };

        let contents = FolderContents::with_contents(vec![file.clone()], vec![]);
        assert_eq!(contents.files.len(), 1);
        assert_eq!(contents.files[0], file);
        assert_eq!(contents.folders.len(), 0);
    }

    #[test]
    fn test_folder_contents_with_folders() {
        let owner_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let folder = Folder::new_root(owner_id, tenant_id);

        let contents = FolderContents::with_contents(vec![], vec![folder.clone()]);
        assert_eq!(contents.files.len(), 0);
        assert_eq!(contents.folders.len(), 1);
        assert_eq!(contents.folders[0], folder);
    }

    #[test]
    fn test_folder_contents_with_mixed_contents() {
        let owner_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let file = File {
            id: Uuid::new_v4(),
            name: "test.txt".to_string(),
            path: "/test.txt".to_string(),
            content_hash: "hash123".to_string(),
            size: 100,
            mime_type: "text/plain".to_string(),
            parent_folder_id: None,
            owner_id,
            current_version: 1,
            created_at: Utc::now(),
            modified_at: Utc::now(),
            starred_at: None,
            deleted_at: None,
            tenant_id,
        };

        let folder = Folder::new_root(owner_id, tenant_id);

        let contents = FolderContents::with_contents(vec![file.clone()], vec![folder.clone()]);
        assert_eq!(contents.files.len(), 1);
        assert_eq!(contents.folders.len(), 1);
        assert_eq!(contents.files[0], file);
        assert_eq!(contents.folders[0], folder);
    }

    #[test]
    fn test_folder_tree_structure() {
        let owner_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let folder = Folder::new_root(owner_id, tenant_id);
        let tree = FolderTree {
            folder: folder.clone(),
            subfolders: vec![],
            files: vec![],
        };

        assert_eq!(tree.folder, folder);
        assert_eq!(tree.files.len(), 0);
        assert_eq!(tree.subfolders.len(), 0);
    }

    #[test]
    fn test_folder_tree_new() {
        let owner_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let folder = Folder::new_root(owner_id, tenant_id);
        let tree = FolderTree::new(folder.clone());

        assert_eq!(tree.folder, folder);
        assert_eq!(tree.files.len(), 0);
        assert_eq!(tree.subfolders.len(), 0);
    }

    #[test]
    fn test_folder_tree_with_files() {
        let owner_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let folder = Folder::new_root(owner_id, tenant_id);
        let file = File {
            id: Uuid::new_v4(),
            name: "document.pdf".to_string(),
            path: "/document.pdf".to_string(),
            content_hash: "hash456".to_string(),
            size: 2048,
            mime_type: "application/pdf".to_string(),
            parent_folder_id: Some(folder.id),
            owner_id,
            current_version: 1,
            created_at: Utc::now(),
            modified_at: Utc::now(),
            starred_at: None,
            deleted_at: None,
            tenant_id,
        };

        let tree = FolderTree::with_contents(folder.clone(), vec![file.clone()], vec![]);

        assert_eq!(tree.folder, folder);
        assert_eq!(tree.files.len(), 1);
        assert_eq!(tree.files[0], file);
        assert_eq!(tree.subfolders.len(), 0);
    }

    #[test]
    fn test_folder_tree_with_subfolders() {
        let owner_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let root_folder = Folder::new_root(owner_id, tenant_id);
        let subfolder = Folder::new_child(
            "Documents".to_string(),
            "/Documents".to_string(),
            root_folder.id,
            owner_id,
            tenant_id,
        );
        let subtree = FolderTree::new(subfolder.clone());

        let tree = FolderTree::with_contents(root_folder.clone(), vec![], vec![subtree.clone()]);

        assert_eq!(tree.folder, root_folder);
        assert_eq!(tree.files.len(), 0);
        assert_eq!(tree.subfolders.len(), 1);
        assert_eq!(tree.subfolders[0].folder, subfolder);
    }

    #[test]
    fn test_folder_tree_recursive_structure() {
        let owner_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let root_folder = Folder::new_root(owner_id, tenant_id);
        let file = File {
            id: Uuid::new_v4(),
            name: "readme.md".to_string(),
            path: "/readme.md".to_string(),
            content_hash: "hash789".to_string(),
            size: 512,
            mime_type: "text/markdown".to_string(),
            parent_folder_id: Some(root_folder.id),
            owner_id,
            current_version: 1,
            created_at: Utc::now(),
            modified_at: Utc::now(),
            starred_at: None,
            deleted_at: None,
            tenant_id,
        };

        let subfolder1 = Folder::new_child(
            "Documents".to_string(),
            "/Documents".to_string(),
            root_folder.id,
            owner_id,
            tenant_id,
        );

        let subfolder2 = Folder::new_child(
            "Projects".to_string(),
            "/Projects".to_string(),
            root_folder.id,
            owner_id,
            tenant_id,
        );

        let subtree1 = FolderTree::new(subfolder1.clone());
        let subtree2 = FolderTree::new(subfolder2.clone());

        let tree = FolderTree::with_contents(
            root_folder.clone(),
            vec![file.clone()],
            vec![subtree1.clone(), subtree2.clone()],
        );

        assert_eq!(tree.folder, root_folder);
        assert_eq!(tree.files.len(), 1);
        assert_eq!(tree.files[0], file);
        assert_eq!(tree.subfolders.len(), 2);
        assert_eq!(tree.subfolders[0].folder, subfolder1);
        assert_eq!(tree.subfolders[1].folder, subfolder2);
    }
}
