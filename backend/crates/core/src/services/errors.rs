use thiserror::Error;

use crate::domain::{FileId, FolderId, UserId};

/// Errors that can occur during file operations.
#[derive(Debug, Error)]
pub enum FileError {
    /// File with the given ID was not found.
    #[error("File not found: {0}")]
    NotFound(FileId),

    /// Version conflict during optimistic locking.
    #[error("Version conflict: expected version {expected}, but found version {actual}")]
    VersionConflict { expected: i32, actual: i32 },

    /// User lacks permission to perform the operation on this file.
    #[error("Permission denied: user {user_id} cannot access file {file_id}")]
    PermissionDenied { file_id: FileId, user_id: UserId },

    /// The parent folder does not exist.
    #[error("Parent folder not found: {0}")]
    ParentFolderNotFound(FolderId),

    /// A file with the same name already exists in the target folder.
    #[error("File already exists: {name} in folder {folder_id}")]
    FileAlreadyExists { name: String, folder_id: FolderId },

    /// The file content could not be stored.
    #[error("Storage error: {0}")]
    StorageError(String),

    /// The file content could not be retrieved.
    #[error("Download error: {0}")]
    DownloadError(String),

    /// Database operation failed.
    #[error("Database error: {0}")]
    DatabaseError(String),

    /// Invalid input provided.
    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

/// Errors that can occur during folder operations.
#[derive(Debug, Error)]
pub enum FolderError {
    /// Folder with the given ID was not found.
    #[error("Folder not found: {0}")]
    NotFound(FolderId),

    /// User lacks permission to perform the operation on this folder.
    #[error("Permission denied: user {user_id} cannot access folder {folder_id}")]
    PermissionDenied {
        folder_id: FolderId,
        user_id: UserId,
    },

    /// The parent folder does not exist.
    #[error("Parent folder not found: {0}")]
    ParentFolderNotFound(FolderId),

    /// A folder with the same name already exists in the target folder.
    #[error("Folder already exists: {name} in folder {parent_id}")]
    FolderAlreadyExists { name: String, parent_id: FolderId },

    /// Attempted to move a folder into itself or one of its descendants.
    #[error("Circular reference: cannot move folder {folder_id} into {target_id}")]
    CircularReference {
        folder_id: FolderId,
        target_id: FolderId,
    },

    /// Cannot delete a folder that contains files or subfolders.
    #[error("Folder not empty: {0}")]
    FolderNotEmpty(FolderId),

    /// Database operation failed.
    #[error("Database error: {0}")]
    DatabaseError(String),

    /// Invalid input provided.
    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_file_error_not_found() {
        let id = Uuid::new_v4();
        let err = FileError::NotFound(id);
        assert_eq!(err.to_string(), format!("File not found: {}", id));
    }

    #[test]
    fn test_version_conflict_error() {
        let err = FileError::VersionConflict {
            expected: 5,
            actual: 3,
        };
        assert!(err.to_string().contains("Version conflict"));
    }

    #[test]
    fn test_file_error_permission_denied() {
        let file_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let err = FileError::PermissionDenied { file_id, user_id };
        assert!(err.to_string().contains("Permission denied"));
    }

    #[test]
    fn test_folder_error_not_found() {
        let id = Uuid::new_v4();
        let err = FolderError::NotFound(id);
        assert_eq!(err.to_string(), format!("Folder not found: {}", id));
    }

    #[test]
    fn test_folder_circular_reference() {
        let folder_id = Uuid::new_v4();
        let target_id = Uuid::new_v4();
        let err = FolderError::CircularReference {
            folder_id,
            target_id,
        };
        assert!(err.to_string().contains("Circular reference"));
    }

    #[test]
    fn test_folder_error_permission_denied() {
        let folder_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let err = FolderError::PermissionDenied { folder_id, user_id };
        assert!(err.to_string().contains("Permission denied"));
    }
}
