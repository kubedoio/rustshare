use thiserror::Error;

use crate::domain::{FileId, FolderId, UserId};

/// Errors that can occur during file operations.
#[derive(Debug, Error)]
pub enum FileError {
    /// File with the given ID was not found.
    #[error("File not found: {0}")]
    NotFound(FileId),

    /// User lacks permission to perform the operation on this file.
    #[error("Permission denied: user {user_id} cannot access file {file_id}")]
    PermissionDenied { file_id: FileId, user_id: UserId },

    /// Version conflict during optimistic locking.
    #[error("Version conflict: expected version {expected}, but found version {actual} (modified by {current_modified_by} at {current_modified_at})")]
    VersionConflict {
        expected: i32,
        actual: i32,
        current_modified_by: String,
        current_modified_at: String,
    },

    /// The parent folder does not exist.
    #[error("Parent folder not found: {0}")]
    ParentFolderNotFound(FolderId),

    /// Target folder does not exist.
    #[error("Folder not found: {0}")]
    FolderNotFound(FolderId),

    /// User's storage quota has been exceeded.
    #[error("Quota exceeded for user {user_id}: using {used} bytes of {quota} bytes quota")]
    QuotaExceeded {
        user_id: UserId,
        used: i64,
        quota: i64,
    },

    /// File name is invalid (e.g., empty, contains illegal characters).
    #[error("Invalid file name: {0}")]
    InvalidName(String),

    /// The requested file version was not found.
    #[error("Version not found: {0}")]
    VersionNotFound(i32),

    /// Database operation failed.
    #[error("Database error: {0}")]
    Database(String),

    /// Storage operation failed.
    #[error("Storage error: {0}")]
    Storage(String),
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

    /// Attempted to move a folder into itself or one of its descendants.
    #[error("Circular reference: cannot move folder {folder_id} into {target_id}")]
    CircularReference {
        folder_id: FolderId,
        target_id: FolderId,
    },

    /// A folder with the same name already exists in the parent folder.
    #[error("Duplicate folder name: {0}")]
    DuplicateName(String),

    /// Folder name is invalid (e.g., empty, contains illegal characters).
    #[error("Invalid folder name: {0}")]
    InvalidName(String),

    /// Cannot delete the root folder.
    #[error("Cannot delete root folder: {0}")]
    CannotDeleteRoot(FolderId),

    /// Folder is not empty and cannot be deleted.
    #[error("Folder is not empty: {0}")]
    NotEmpty(FolderId),

    /// Invalid move operation.
    #[error("Invalid move for folder {folder_id}: {reason}")]
    InvalidMove { folder_id: FolderId, reason: String },

    /// Storage operation failed.
    #[error("Storage error: {0}")]
    Storage(String),

    /// Database operation failed.
    #[error("Database error: {0}")]
    Database(String),
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
    fn test_file_error_permission_denied() {
        let file_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let err = FileError::PermissionDenied { file_id, user_id };
        assert!(err.to_string().contains("Permission denied"));
    }

    #[test]
    fn test_file_error_version_conflict() {
        let err = FileError::VersionConflict {
            expected: 5,
            actual: 3,
            current_modified_by: "user@example.com".to_string(),
            current_modified_at: "2026-03-17T10:30:00Z".to_string(),
        };
        assert!(err.to_string().contains("Version conflict"));
        assert!(err.to_string().contains("expected version 5"));
        assert!(err.to_string().contains("found version 3"));
        assert!(err.to_string().contains("modified by user@example.com"));
        assert!(err.to_string().contains("at 2026-03-17T10:30:00Z"));
    }

    #[test]
    fn test_file_error_parent_folder_not_found() {
        let folder_id = Uuid::new_v4();
        let err = FileError::ParentFolderNotFound(folder_id);
        assert_eq!(
            err.to_string(),
            format!("Parent folder not found: {}", folder_id)
        );
    }

    #[test]
    fn test_file_error_quota_exceeded() {
        let user_id = Uuid::new_v4();
        let err = FileError::QuotaExceeded {
            user_id,
            used: 1024 * 1024 * 1024, // 1 GB
            quota: 500 * 1024 * 1024, // 500 MB
        };
        assert!(err.to_string().contains("Quota exceeded"));
        assert!(err.to_string().contains(&user_id.to_string()));
        assert!(err.to_string().contains("1073741824")); // 1 GB in bytes
        assert!(err.to_string().contains("524288000")); // 500 MB in bytes
    }

    #[test]
    fn test_file_error_invalid_name() {
        let name = "invalid/name";
        let err = FileError::InvalidName(name.to_string());
        assert_eq!(err.to_string(), format!("Invalid file name: {}", name));
    }

    #[test]
    fn test_file_error_version_not_found() {
        let version = 42;
        let err = FileError::VersionNotFound(version);
        assert_eq!(err.to_string(), format!("Version not found: {}", version));
    }

    #[test]
    fn test_file_error_storage() {
        let msg = "upload failed";
        let err = FileError::Storage(msg.to_string());
        assert_eq!(err.to_string(), format!("Storage error: {}", msg));
    }

    #[test]
    fn test_file_error_database() {
        let msg = "connection failed";
        let err = FileError::Database(msg.to_string());
        assert_eq!(err.to_string(), format!("Database error: {}", msg));
    }

    #[test]
    fn test_folder_error_not_found() {
        let id = Uuid::new_v4();
        let err = FolderError::NotFound(id);
        assert_eq!(err.to_string(), format!("Folder not found: {}", id));
    }

    #[test]
    fn test_folder_error_permission_denied() {
        let folder_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let err = FolderError::PermissionDenied { folder_id, user_id };
        assert!(err.to_string().contains("Permission denied"));
    }

    #[test]
    fn test_folder_error_parent_folder_not_found() {
        let folder_id = Uuid::new_v4();
        let err = FolderError::ParentFolderNotFound(folder_id);
        assert_eq!(
            err.to_string(),
            format!("Parent folder not found: {}", folder_id)
        );
    }

    #[test]
    fn test_folder_error_circular_reference() {
        let folder_id = Uuid::new_v4();
        let target_id = Uuid::new_v4();
        let err = FolderError::CircularReference {
            folder_id,
            target_id,
        };
        assert!(err.to_string().contains("Circular reference"));
    }

    #[test]
    fn test_folder_error_duplicate_name() {
        let name = "Documents".to_string();
        let err = FolderError::DuplicateName(name.clone());
        assert!(err.to_string().contains("Duplicate folder name"));
        assert!(err.to_string().contains(&name));
    }

    #[test]
    fn test_folder_error_invalid_name() {
        let name = "invalid/name";
        let err = FolderError::InvalidName(name.to_string());
        assert_eq!(err.to_string(), format!("Invalid folder name: {}", name));
    }

    #[test]
    fn test_folder_error_cannot_delete_root() {
        let folder_id = Uuid::new_v4();
        let err = FolderError::CannotDeleteRoot(folder_id);
        assert_eq!(
            err.to_string(),
            format!("Cannot delete root folder: {}", folder_id)
        );
    }

    #[test]
    fn test_folder_error_database() {
        let msg = "connection failed";
        let err = FolderError::Database(msg.to_string());
        assert_eq!(err.to_string(), format!("Database error: {}", msg));
    }
}
