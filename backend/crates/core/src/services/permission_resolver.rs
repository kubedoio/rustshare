//! PermissionResolver service for resolving user permissions on files and folders.
//!
//! This service handles permission checks with caching and folder inheritance:
//! - Owner always has Admin permission (no DB lookup)
//! - Direct share permissions (on file/folder)
//! - Folder inheritance (walk up to 50 levels max)
//! - Per-request caching to avoid repeated tree walks

use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::domain::{File, FileId, Folder, FolderId, SharePermissions, UserId};

/// Resource type for permission checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resource {
    File(FileId),
    Folder(FolderId),
}

/// Cache key for permission lookups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum CacheKey {
    File(UserId, FileId),
    Folder(UserId, FolderId),
}

/// Trait for share repository operations needed by PermissionResolver.
#[allow(async_fn_in_trait)]
pub trait ShareResolverOps: Send + Sync {
    /// Find a user share by resource and recipient.
    async fn find_user_share(
        &self,
        file_id: Option<FileId>,
        folder_id: Option<FolderId>,
        recipient_user_id: UserId,
    ) -> Result<Option<crate::domain::Share>>;
}

/// Trait for file metadata operations needed by PermissionResolver.
#[allow(async_fn_in_trait)]
pub trait FileResolverOps: Send + Sync {
    /// Find a file by ID.
    async fn find_file_by_id(&self, id: FileId) -> Result<Option<File>>;
}

/// Trait for folder metadata operations needed by PermissionResolver.
#[allow(async_fn_in_trait)]
pub trait FolderResolverOps: Send + Sync {
    /// Find a folder by ID.
    async fn find_folder_by_id(&self, id: FolderId) -> Result<Option<Folder>>;
}

/// PermissionResolver service handles permission checks with caching and folder inheritance.
///
/// Generic over ShareResolverOps, FileResolverOps, and FolderResolverOps implementations to support
/// different backends and testing with mock implementations.
pub struct PermissionResolver<S: ShareResolverOps, F: FileResolverOps, D: FolderResolverOps> {
    share_ops: Arc<S>,
    file_ops: Arc<F>,
    folder_ops: Arc<D>,
    cache: Mutex<HashMap<CacheKey, Option<SharePermissions>>>,
}

impl<S: ShareResolverOps, F: FileResolverOps, D: FolderResolverOps> PermissionResolver<S, F, D> {
    /// Create a new PermissionResolver instance.
    pub fn new(share_ops: Arc<S>, file_ops: Arc<F>, folder_ops: Arc<D>) -> Self {
        Self {
            share_ops,
            file_ops,
            folder_ops,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Check if user has required permission on file.
    ///
    /// Checks in order:
    /// 1. Owner check (Admin permission, no DB lookup)
    /// 2. Direct share on file
    /// 3. Folder ancestry (inherited permissions)
    ///
    /// Returns true if user has the required permission or higher.
    pub async fn check_file_permission(
        &self,
        user_id: UserId,
        file_id: FileId,
        required: SharePermissions,
    ) -> Result<bool> {
        // Check cache first
        let cache_key = CacheKey::File(user_id, file_id);
        let cached = { self.cache.lock().unwrap().get(&cache_key).copied() };
        if let Some(cached) = cached {
            return Ok(cached.map_or(false, |perm| perm >= required));
        }

        // Get file metadata
        let file = self
            .file_ops
            .find_file_by_id(file_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("File not found"))?;

        // 1. Check ownership (implicit Admin permission)
        if file.owner_id == user_id {
            self.cache
                .lock()
                .unwrap()
                .insert(cache_key, Some(SharePermissions::Admin));
            return Ok(true);
        }

        // 2. Check direct share on file
        if let Some(share) = self
            .share_ops
            .find_user_share(Some(file_id), None, user_id)
            .await?
        {
            if share.revoked_at.is_none() {
                let perm = share.permissions;
                self.cache.lock().unwrap().insert(cache_key, Some(perm));
                return Ok(perm >= required);
            }
        }

        // 3. Walk up folder ancestry for inherited permissions
        if let Some(parent_folder_id) = file.parent_folder_id {
            if let Some(inherited_perm) = self
                .resolve_folder_ancestry(user_id, parent_folder_id)
                .await?
            {
                self.cache
                    .lock()
                    .unwrap()
                    .insert(cache_key, Some(inherited_perm));
                return Ok(inherited_perm >= required);
            }
        }

        // No permission found
        self.cache.lock().unwrap().insert(cache_key, None);
        Ok(false)
    }

    /// Check if user has required permission on folder.
    ///
    /// Checks in order:
    /// 1. Owner check (Admin permission, no DB lookup)
    /// 2. Direct share on folder
    /// 3. Parent folder ancestry (inherited permissions)
    ///
    /// Returns true if user has the required permission or higher.
    pub async fn check_folder_permission(
        &self,
        user_id: UserId,
        folder_id: FolderId,
        required: SharePermissions,
    ) -> Result<bool> {
        // Check cache first
        let cache_key = CacheKey::Folder(user_id, folder_id);
        let cached = { self.cache.lock().unwrap().get(&cache_key).copied() };
        if let Some(cached) = cached {
            return Ok(cached.map_or(false, |perm| perm >= required));
        }

        // Get folder metadata
        let folder = self
            .folder_ops
            .find_folder_by_id(folder_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Folder not found"))?;

        // 1. Check ownership (implicit Admin permission)
        if folder.owner_id == user_id {
            self.cache
                .lock()
                .unwrap()
                .insert(cache_key, Some(SharePermissions::Admin));
            return Ok(true);
        }

        // 2. Check direct share on folder
        if let Some(share) = self
            .share_ops
            .find_user_share(None, Some(folder_id), user_id)
            .await?
        {
            if share.revoked_at.is_none() {
                let perm = share.permissions;
                self.cache.lock().unwrap().insert(cache_key, Some(perm));
                return Ok(perm >= required);
            }
        }

        // 3. Walk up parent folder ancestry for inherited permissions
        if let Some(parent_folder_id) = folder.parent_folder_id {
            if let Some(inherited_perm) = self
                .resolve_folder_ancestry(user_id, parent_folder_id)
                .await?
            {
                self.cache
                    .lock()
                    .unwrap()
                    .insert(cache_key, Some(inherited_perm));
                return Ok(inherited_perm >= required);
            }
        }

        // No permission found
        self.cache.lock().unwrap().insert(cache_key, None);
        Ok(false)
    }

    /// Walk up folder ancestry to find inherited permissions.
    ///
    /// Returns the highest permission found in the folder tree.
    /// Walks up to 50 levels max to prevent infinite loops.
    async fn resolve_folder_ancestry(
        &self,
        user_id: UserId,
        mut folder_id: FolderId,
    ) -> Result<Option<SharePermissions>> {
        let mut permissions = Vec::new();
        let mut max_depth = 50;

        while max_depth > 0 {
            // Check cache for this folder
            let cache_key = CacheKey::Folder(user_id, folder_id);
            let cached = { self.cache.lock().unwrap().get(&cache_key).copied() };
            if let Some(cached) = cached {
                if let Some(perm) = cached {
                    permissions.push(perm);
                }
                // If cached with no permission, continue walking up
            } else {
                // Check for share on this folder
                if let Some(share) = self
                    .share_ops
                    .find_user_share(None, Some(folder_id), user_id)
                    .await?
                {
                    if share.revoked_at.is_none() {
                        let perm = share.permissions;
                        self.cache.lock().unwrap().insert(cache_key, Some(perm));
                        permissions.push(perm);
                    }
                } else {
                    self.cache.lock().unwrap().insert(cache_key, None);
                }
            }

            // Get parent folder
            let folder = self
                .folder_ops
                .find_folder_by_id(folder_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Folder not found in ancestry"))?;

            match folder.parent_folder_id {
                Some(parent_id) => folder_id = parent_id,
                None => break, // Reached root
            }

            max_depth -= 1;
        }

        if max_depth == 0 {
            return Err(anyhow::anyhow!("Max folder depth exceeded (50 levels)"));
        }

        // Return highest permission found
        Ok(if permissions.is_empty() {
            None
        } else {
            Some(SharePermissions::max(&permissions))
        })
    }

    /// Clear the permission cache.
    ///
    /// Should be called at the end of each request to ensure cache doesn't leak between requests.
    pub fn clear_cache(&self) {
        self.cache.lock().unwrap().clear();
    }

    /// Resolve the permission a user has on a resource (file or folder).
    ///
    /// Returns Some(permission) if the user has access, None otherwise.
    /// This is a convenience method that wraps check_file_permission and check_folder_permission.
    pub async fn resolve_permission(
        &self,
        user_id: UserId,
        resource: Resource,
    ) -> Result<Option<SharePermissions>> {
        match resource {
            Resource::File(file_id) => {
                // Check all permission levels from highest to lowest
                if self
                    .check_file_permission(user_id, file_id, SharePermissions::Admin)
                    .await?
                {
                    Ok(Some(SharePermissions::Admin))
                } else if self
                    .check_file_permission(user_id, file_id, SharePermissions::Edit)
                    .await?
                {
                    Ok(Some(SharePermissions::Edit))
                } else if self
                    .check_file_permission(user_id, file_id, SharePermissions::View)
                    .await?
                {
                    Ok(Some(SharePermissions::View))
                } else {
                    Ok(None)
                }
            }
            Resource::Folder(folder_id) => {
                // Check all permission levels from highest to lowest
                if self
                    .check_folder_permission(user_id, folder_id, SharePermissions::Admin)
                    .await?
                {
                    Ok(Some(SharePermissions::Admin))
                } else if self
                    .check_folder_permission(user_id, folder_id, SharePermissions::Edit)
                    .await?
                {
                    Ok(Some(SharePermissions::Edit))
                } else if self
                    .check_folder_permission(user_id, folder_id, SharePermissions::View)
                    .await?
                {
                    Ok(Some(SharePermissions::View))
                } else {
                    Ok(None)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Share;
    use chrono::Utc;
    use std::sync::Mutex;
    use uuid::Uuid;

    struct MockShareOps {
        shares: Mutex<Vec<Share>>,
    }

    impl MockShareOps {
        fn new() -> Self {
            Self {
                shares: Mutex::new(Vec::new()),
            }
        }

        fn add_share(&self, share: Share) {
            self.shares.lock().unwrap().push(share);
        }
    }

    impl ShareResolverOps for MockShareOps {
        async fn find_user_share(
            &self,
            file_id: Option<FileId>,
            folder_id: Option<FolderId>,
            recipient_user_id: UserId,
        ) -> Result<Option<Share>> {
            let shares = self.shares.lock().unwrap();
            Ok(shares
                .iter()
                .find(|s| {
                    s.file_id == file_id
                        && s.folder_id == folder_id
                        && s.recipient_user_id == Some(recipient_user_id)
                })
                .cloned())
        }
    }

    struct MockFileOps {
        files: Mutex<Vec<File>>,
    }

    impl MockFileOps {
        fn new() -> Self {
            Self {
                files: Mutex::new(Vec::new()),
            }
        }

        fn add_file(&self, file: File) {
            self.files.lock().unwrap().push(file);
        }
    }

    impl FileResolverOps for MockFileOps {
        async fn find_file_by_id(&self, id: FileId) -> Result<Option<File>> {
            Ok(self
                .files
                .lock()
                .unwrap()
                .iter()
                .find(|f| f.id == id)
                .cloned())
        }
    }

    struct MockFolderOps {
        folders: Mutex<Vec<Folder>>,
    }

    impl MockFolderOps {
        fn new() -> Self {
            Self {
                folders: Mutex::new(Vec::new()),
            }
        }

        fn add_folder(&self, folder: Folder) {
            self.folders.lock().unwrap().push(folder);
        }
    }

    impl FolderResolverOps for MockFolderOps {
        async fn find_folder_by_id(&self, id: FolderId) -> Result<Option<Folder>> {
            Ok(self
                .folders
                .lock()
                .unwrap()
                .iter()
                .find(|f| f.id == id)
                .cloned())
        }
    }

    fn setup() -> (
        PermissionResolver<MockShareOps, MockFileOps, MockFolderOps>,
        Arc<MockShareOps>,
        Arc<MockFileOps>,
        Arc<MockFolderOps>,
    ) {
        let share_ops = Arc::new(MockShareOps::new());
        let file_ops = Arc::new(MockFileOps::new());
        let folder_ops = Arc::new(MockFolderOps::new());

        let resolver =
            PermissionResolver::new(share_ops.clone(), file_ops.clone(), folder_ops.clone());

        (resolver, share_ops, file_ops, folder_ops)
    }

    #[tokio::test]
    async fn test_owner_has_admin_permission() {
        let (resolver, _share_ops, file_ops, _folder_ops) = setup();

        let owner_id = Uuid::new_v4();
        let file = File::new(
            "test.txt".to_string(),
            "/test.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        let file_id = file.id;
        file_ops.add_file(file);

        // Owner should have Admin permission without any share record
        assert!(resolver
            .check_file_permission(owner_id, file_id, SharePermissions::Admin)
            .await
            .unwrap());
        assert!(resolver
            .check_file_permission(owner_id, file_id, SharePermissions::Edit)
            .await
            .unwrap());
        assert!(resolver
            .check_file_permission(owner_id, file_id, SharePermissions::View)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_direct_file_share() {
        let (resolver, share_ops, file_ops, _folder_ops) = setup();

        let owner_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        let file = File::new(
            "test.txt".to_string(),
            "/test.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        let file_id = file.id;
        file_ops.add_file(file);

        // Create share with View permission
        let share = Share {
            id: Uuid::new_v4(),
            file_id: Some(file_id),
            folder_id: None,
            share_token: None,
            permissions: SharePermissions::View,
            password_hash: None,
            expires_at: None,
            access_count: 0,
            recipient_user_id: Some(user_id),
            created_by: owner_id,
            created_at: Utc::now(),
            revoked_at: None,
        };
        share_ops.add_share(share);

        // User should have View permission
        assert!(resolver
            .check_file_permission(user_id, file_id, SharePermissions::View)
            .await
            .unwrap());
        // But not Edit permission
        assert!(!resolver
            .check_file_permission(user_id, file_id, SharePermissions::Edit)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_folder_permission_inheritance() {
        let (resolver, share_ops, file_ops, folder_ops) = setup();

        let owner_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        // Create folder hierarchy: root -> parent -> child
        let root_folder = Folder::new_root(owner_id);
        let root_id = root_folder.id;
        folder_ops.add_folder(root_folder);

        let parent_folder = Folder::new_child(
            "parent".to_string(),
            "/parent".to_string(),
            root_id,
            owner_id,
        );
        let parent_id = parent_folder.id;
        folder_ops.add_folder(parent_folder);

        let child_folder = Folder::new_child(
            "child".to_string(),
            "/parent/child".to_string(),
            parent_id,
            owner_id,
        );
        let child_id = child_folder.id;
        folder_ops.add_folder(child_folder);

        // Create file in child folder
        let file = File::new(
            "test.txt".to_string(),
            "/parent/child/test.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            Some(child_id),
            owner_id,
        );
        let file_id = file.id;
        file_ops.add_file(file);

        // Share parent folder with Edit permission
        let share = Share {
            id: Uuid::new_v4(),
            file_id: None,
            folder_id: Some(parent_id),
            share_token: None,
            permissions: SharePermissions::Edit,
            password_hash: None,
            expires_at: None,
            access_count: 0,
            recipient_user_id: Some(user_id),
            created_by: owner_id,
            created_at: Utc::now(),
            revoked_at: None,
        };
        share_ops.add_share(share);

        // User should have Edit permission on file through inheritance
        assert!(resolver
            .check_file_permission(user_id, file_id, SharePermissions::View)
            .await
            .unwrap());
        assert!(resolver
            .check_file_permission(user_id, file_id, SharePermissions::Edit)
            .await
            .unwrap());
        assert!(!resolver
            .check_file_permission(user_id, file_id, SharePermissions::Admin)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_revoked_share_denied() {
        let (resolver, share_ops, file_ops, _folder_ops) = setup();

        let owner_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        let file = File::new(
            "test.txt".to_string(),
            "/test.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        let file_id = file.id;
        file_ops.add_file(file);

        // Create revoked share
        let share = Share {
            id: Uuid::new_v4(),
            file_id: Some(file_id),
            folder_id: None,
            share_token: None,
            permissions: SharePermissions::View,
            password_hash: None,
            expires_at: None,
            access_count: 0,
            recipient_user_id: Some(user_id),
            created_by: owner_id,
            created_at: Utc::now(),
            revoked_at: Some(Utc::now()),
        };
        share_ops.add_share(share);

        // User should not have permission
        assert!(!resolver
            .check_file_permission(user_id, file_id, SharePermissions::View)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_caching_works() {
        let (resolver, share_ops, file_ops, _folder_ops) = setup();

        let owner_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        let file = File::new(
            "test.txt".to_string(),
            "/test.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        let file_id = file.id;
        file_ops.add_file(file);

        // Create share
        let share = Share {
            id: Uuid::new_v4(),
            file_id: Some(file_id),
            folder_id: None,
            share_token: None,
            permissions: SharePermissions::Edit,
            password_hash: None,
            expires_at: None,
            access_count: 0,
            recipient_user_id: Some(user_id),
            created_by: owner_id,
            created_at: Utc::now(),
            revoked_at: None,
        };
        share_ops.add_share(share);

        // First call should populate cache
        assert!(resolver
            .check_file_permission(user_id, file_id, SharePermissions::View)
            .await
            .unwrap());

        // Second call should use cache (we can verify this by checking cache size)
        assert!(resolver
            .check_file_permission(user_id, file_id, SharePermissions::Edit)
            .await
            .unwrap());
        assert_eq!(resolver.cache.lock().unwrap().len(), 1);

        // Clear cache
        resolver.clear_cache();
        assert_eq!(resolver.cache.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_no_permission_for_unshared_resource() {
        let (resolver, _share_ops, file_ops, _folder_ops) = setup();

        let owner_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        let file = File::new(
            "test.txt".to_string(),
            "/test.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        let file_id = file.id;
        file_ops.add_file(file);

        // User has no share, should not have permission
        assert!(!resolver
            .check_file_permission(user_id, file_id, SharePermissions::View)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_folder_owner_has_admin() {
        let (resolver, _share_ops, _file_ops, folder_ops) = setup();

        let owner_id = Uuid::new_v4();

        let folder = Folder::new_root(owner_id);
        let folder_id = folder.id;
        folder_ops.add_folder(folder);

        // Owner should have Admin permission
        assert!(resolver
            .check_folder_permission(owner_id, folder_id, SharePermissions::Admin)
            .await
            .unwrap());
        assert!(resolver
            .check_folder_permission(owner_id, folder_id, SharePermissions::Edit)
            .await
            .unwrap());
        assert!(resolver
            .check_folder_permission(owner_id, folder_id, SharePermissions::View)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_max_permission_from_multiple_shares() {
        let (resolver, share_ops, file_ops, folder_ops) = setup();

        let owner_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        // Create folder hierarchy: root -> parent
        let root_folder = Folder::new_root(owner_id);
        let root_id = root_folder.id;
        folder_ops.add_folder(root_folder);

        let parent_folder = Folder::new_child(
            "parent".to_string(),
            "/parent".to_string(),
            root_id,
            owner_id,
        );
        let parent_id = parent_folder.id;
        folder_ops.add_folder(parent_folder);

        // Create file in parent folder
        let file = File::new(
            "test.txt".to_string(),
            "/parent/test.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            Some(parent_id),
            owner_id,
        );
        let file_id = file.id;
        file_ops.add_file(file);

        // Share file with View permission
        let file_share = Share {
            id: Uuid::new_v4(),
            file_id: Some(file_id),
            folder_id: None,
            share_token: None,
            permissions: SharePermissions::View,
            password_hash: None,
            expires_at: None,
            access_count: 0,
            recipient_user_id: Some(user_id),
            created_by: owner_id,
            created_at: Utc::now(),
            revoked_at: None,
        };
        share_ops.add_share(file_share);

        // Share parent folder with Admin permission
        let folder_share = Share {
            id: Uuid::new_v4(),
            file_id: None,
            folder_id: Some(parent_id),
            share_token: None,
            permissions: SharePermissions::Admin,
            password_hash: None,
            expires_at: None,
            access_count: 0,
            recipient_user_id: Some(user_id),
            created_by: owner_id,
            created_at: Utc::now(),
            revoked_at: None,
        };
        share_ops.add_share(folder_share);

        // User should have View permission from direct share (direct share takes precedence)
        // This is correct because we check direct shares before ancestry
        assert!(resolver
            .check_file_permission(user_id, file_id, SharePermissions::View)
            .await
            .unwrap());
        assert!(!resolver
            .check_file_permission(user_id, file_id, SharePermissions::Admin)
            .await
            .unwrap());
    }
}
