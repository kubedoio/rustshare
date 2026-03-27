//! V2 Path Builder
//!
//! Generates S3 keys for all document types in the V2 User Storage Domain.
//! 
//! Bucket Layout:
//! ```
//! owned/
//!   files/{file_id}.json
//!   folders/{folder_id}.json
//!   file_versions/{file_id}/{version_id}.json
//!   shares/
//!     outbound/{share_id}.json
//!   tombstones/
//!     files/{file_id}.json
//!     folders/{folder_id}.json
//! received/
//!   shares/{share_id}.json
//! indexes/
//!   favourites.json
//!   shared_with_me.json
//!   folder_children/{folder_id}.json
//!   roots.json
//! ```

use crate::UserId;
use uuid::Uuid;

/// Path builder for user bucket storage
#[derive(Debug, Clone)]
pub struct UserBucketPaths {
    user_id: UserId,
}

impl UserBucketPaths {
    /// Create a new path builder for the given user
    pub fn new(user_id: UserId) -> Self {
        Self { user_id }
    }

    /// Get the user ID
    pub fn user_id(&self) -> UserId {
        self.user_id
    }

    // =========================================================================
    // OWNED RESOURCES
    // =========================================================================

    /// Path to a file document: owned/files/{file_id}.json
    pub fn file(&self, file_id: Uuid) -> String {
        format!("owned/files/{}.json", file_id)
    }

    /// Path to a folder document: owned/folders/{folder_id}.json
    pub fn folder(&self, folder_id: Uuid) -> String {
        format!("owned/folders/{}.json", folder_id)
    }

    /// Path to a file version document: owned/file_versions/{file_id}/{version_id}.json
    pub fn file_version(&self, file_id: Uuid, version_id: Uuid) -> String {
        format!("owned/file_versions/{}/{}.json", file_id, version_id)
    }

    /// Prefix for all file versions of a file: owned/file_versions/{file_id}/
    pub fn file_versions_prefix(&self, file_id: Uuid) -> String {
        format!("owned/file_versions/{}/", file_id)
    }

    // =========================================================================
    // SHARES
    // =========================================================================

    /// Path to an outbound share document: owned/shares/outbound/{share_id}.json
    pub fn outbound_share(&self, share_id: Uuid) -> String {
        format!("owned/shares/outbound/{}.json", share_id)
    }

    /// Prefix for all outbound shares: owned/shares/outbound/
    pub fn outbound_shares_prefix(&self) -> String {
        "owned/shares/outbound/".to_string()
    }

    /// Path to a received share document: received/shares/{share_id}.json
    pub fn received_share(&self, share_id: Uuid) -> String {
        format!("received/shares/{}.json", share_id)
    }

    /// Prefix for all received shares: received/shares/
    pub fn received_shares_prefix(&self) -> String {
        "received/shares/".to_string()
    }

    // =========================================================================
    // TOMBSTONES (Soft Delete Support)
    // =========================================================================

    /// Path to a file tombstone: owned/tombstones/files/{file_id}.json
    pub fn file_tombstone(&self, file_id: Uuid) -> String {
        format!("owned/tombstones/files/{}.json", file_id)
    }

    /// Path to a folder tombstone: owned/tombstones/folders/{folder_id}.json
    pub fn folder_tombstone(&self, folder_id: Uuid) -> String {
        format!("owned/tombstones/folders/{}.json", folder_id)
    }

    /// Prefix for all tombstones: owned/tombstones/
    pub fn tombstones_prefix(&self) -> String {
        "owned/tombstones/".to_string()
    }

    // =========================================================================
    // INDEXES (Derived State - Rebuildable)
    // =========================================================================

    /// Path to favourites index: indexes/favourites.json
    pub fn favourites_index(&self) -> String {
        "indexes/favourites.json".to_string()
    }

    /// Path to shared with me index: indexes/shared_with_me.json
    pub fn shared_with_me_index(&self) -> String {
        "indexes/shared_with_me.json".to_string()
    }

    /// Path to folder children index: indexes/folder_children/{folder_id}.json
    pub fn folder_children_index(&self, folder_id: Uuid) -> String {
        format!("indexes/folder_children/{}.json", folder_id)
    }

    /// Prefix for all folder children indexes: indexes/folder_children/
    pub fn folder_children_prefix(&self) -> String {
        "indexes/folder_children/".to_string()
    }

    /// Path to user roots index: indexes/roots.json
    pub fn roots_index(&self) -> String {
        "indexes/roots.json".to_string()
    }
}

/// Path builder for cross-bucket operations
#[derive(Debug, Clone)]
pub struct CrossBucketPaths;

impl CrossBucketPaths {
    /// Parse a file path to extract the file ID
    /// Expects format: owned/files/{file_id}.json
    pub fn parse_file_id(path: &str) -> Option<Uuid> {
        let prefix = "owned/files/";
        let suffix = ".json";
        
        if let Some(start) = path.find(prefix) {
            let id_start = start + prefix.len();
            if let Some(end) = path[id_start..].find(suffix) {
                let id_str = &path[id_start..id_start + end];
                Uuid::parse_str(id_str).ok()
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Parse a folder path to extract the folder ID
    /// Expects format: owned/folders/{folder_id}.json
    pub fn parse_folder_id(path: &str) -> Option<Uuid> {
        let prefix = "owned/folders/";
        let suffix = ".json";
        
        if let Some(start) = path.find(prefix) {
            let id_start = start + prefix.len();
            if let Some(end) = path[id_start..].find(suffix) {
                let id_str = &path[id_start..id_start + end];
                Uuid::parse_str(id_str).ok()
            } else {
                None
            }
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_path() {
        let paths = UserBucketPaths::new(Uuid::new_v4());
        let file_id = Uuid::new_v4();
        assert_eq!(
            paths.file(file_id),
            format!("owned/files/{}.json", file_id)
        );
    }

    #[test]
    fn test_folder_path() {
        let paths = UserBucketPaths::new(Uuid::new_v4());
        let folder_id = Uuid::new_v4();
        assert_eq!(
            paths.folder(folder_id),
            format!("owned/folders/{}.json", folder_id)
        );
    }

    #[test]
    fn test_file_version_path() {
        let paths = UserBucketPaths::new(Uuid::new_v4());
        let file_id = Uuid::new_v4();
        let version_id = Uuid::new_v4();
        assert_eq!(
            paths.file_version(file_id, version_id),
            format!("owned/file_versions/{}/{}.json", file_id, version_id)
        );
    }

    #[test]
    fn test_parse_file_id() {
        let file_id = Uuid::new_v4();
        let path = format!("owned/files/{}.json", file_id);
        assert_eq!(CrossBucketPaths::parse_file_id(&path), Some(file_id));
    }

    #[test]
    fn test_parse_folder_id() {
        let folder_id = Uuid::new_v4();
        let path = format!("owned/folders/{}.json", folder_id);
        assert_eq!(CrossBucketPaths::parse_folder_id(&path), Some(folder_id));
    }
}
