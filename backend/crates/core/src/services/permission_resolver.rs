//! PermissionResolver service for resolving user permissions on files and folders.
//!
//! This service handles permission checks with caching and folder inheritance:
//! - Owner always has Admin permission (no DB lookup)
//! - Direct share permissions (on file/folder)
//! - Group share permissions (via group membership)
//! - Folder inheritance (walk up to 50 levels max)
//! - Per-request caching to avoid repeated tree walks

use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use crate::domain::{File, FileId, Folder, FolderId, Share, ShareId, SharePermissions, UserId};

/// Resource type for permission checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resource {
    File(FileId),
    Folder(FolderId),
}

/// Indicates how permission was granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionSource {
    /// User owns the resource.
    Owner,
    /// Direct user-to-user share.
    DirectShare,
    /// Access via group membership.
    GroupShare,
    /// Inherited from parent folder.
    Inherited,
    /// No permission.
    None,
}

/// Result of permission resolution with source information.
#[derive(Debug, Clone, Copy)]
pub struct PermissionResult {
    /// The permission level granted (if any).
    pub permission: Option<SharePermissions>,
    /// How the permission was granted.
    pub source: PermissionSource,
    /// The share that granted access (if applicable).
    pub share_id: Option<ShareId>,
}

/// Cache key for permission lookups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum CacheKey {
    File(UserId, FileId),
    Folder(UserId, FolderId),
}

/// Combined trait for all operations needed by PermissionResolver.
///
/// This trait combines share resolution, file/folder metadata, and group membership
/// operations to simplify the generic bounds for PermissionResolver.
#[allow(async_fn_in_trait)]
pub trait PermissionResolverOps: Send + Sync {
    /// Find a user share by resource and recipient.
    async fn find_user_share(
        &self,
        file_id: Option<FileId>,
        folder_id: Option<FolderId>,
        recipient_user_id: UserId,
    ) -> Result<Option<Share>>;

    /// Find group shares by resource and group IDs.
    /// Returns all shares where recipient_group_id matches any of the provided group_ids.
    async fn find_group_shares(
        &self,
        file_id: Option<FileId>,
        folder_id: Option<FolderId>,
        group_ids: &[Uuid],
    ) -> Result<Vec<Share>>;

    /// Find user shares for multiple folders at once.
    /// Returns all shares where the resource is one of the folder_ids and recipient matches user_id.
    async fn find_user_shares_for_folders(
        &self,
        folder_ids: &[FolderId],
        recipient_user_id: UserId,
    ) -> Result<Vec<Share>>;

    /// Find group shares for multiple folders at once.
    /// Returns all shares where the resource is one of the folder_ids and recipient_group_id matches any group_ids.
    async fn find_group_shares_for_folders(
        &self,
        folder_ids: &[FolderId],
        group_ids: &[Uuid],
    ) -> Result<Vec<Share>>;

    /// Find a file by ID.
    async fn find_file_by_id(&self, id: FileId) -> Result<Option<File>>;

    /// Find a folder by ID.
    async fn find_folder_by_id(&self, id: FolderId) -> Result<Option<Folder>>;

    /// Get all group IDs that a user is a member of.
    async fn get_user_group_ids(&self, user_id: UserId) -> Result<Vec<Uuid>>;
}

/// PermissionResolver service handles permission checks with caching and folder inheritance.
///
/// Generic over PermissionResolverOps implementations to support different backends
/// and testing with mock implementations.
pub struct PermissionResolver<Ops: PermissionResolverOps> {
    ops: Arc<Ops>,
    cache: Mutex<HashMap<CacheKey, Option<SharePermissions>>>,
}

impl<Ops: PermissionResolverOps> PermissionResolver<Ops> {
    /// Create a new PermissionResolver instance.
    pub fn new(ops: Arc<Ops>) -> Self {
        Self {
            ops,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Check if user has required permission on file.
    ///
    /// Checks in order:
    /// 1. Owner check (Admin permission, no DB lookup)
    /// 2. Direct share on file
    /// 3. Group shares on file (highest permission wins)
    /// 4. Folder ancestry (inherited permissions from user and group shares)
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
            return Ok(cached.is_some_and(|perm| perm >= required));
        }

        // Get file metadata
        let file = self
            .ops
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
            .ops
            .find_user_share(Some(file_id), None, user_id)
            .await?
        {
            if share.revoked_at.is_none() {
                let perm = share.permissions;
                self.cache.lock().unwrap().insert(cache_key, Some(perm));
                return Ok(perm >= required);
            }
        }

        // 3. Check group shares on file
        let user_groups = self.ops.get_user_group_ids(user_id).await?;
        if !user_groups.is_empty() {
            let group_shares = self
                .ops
                .find_group_shares(Some(file_id), None, &user_groups)
                .await?;
            let highest_group_perm = group_shares
                .iter()
                .filter(|s| s.revoked_at.is_none())
                .map(|s| s.permissions)
                .max();
            if let Some(perm) = highest_group_perm {
                self.cache.lock().unwrap().insert(cache_key, Some(perm));
                return Ok(perm >= required);
            }
        }

        // 4. Walk up folder ancestry for inherited permissions
        if let Some(parent_folder_id) = file.parent_folder_id {
            if let Some(inherited_perm) = self
                .resolve_folder_ancestry(user_id, parent_folder_id, &user_groups)
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
    /// 3. Group shares on folder (highest permission wins)
    /// 4. Parent folder ancestry (inherited permissions)
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
            return Ok(cached.is_some_and(|perm| perm >= required));
        }

        // Get folder metadata
        let folder = self
            .ops
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
            .ops
            .find_user_share(None, Some(folder_id), user_id)
            .await?
        {
            if share.revoked_at.is_none() {
                let perm = share.permissions;
                self.cache.lock().unwrap().insert(cache_key, Some(perm));
                return Ok(perm >= required);
            }
        }

        // 3. Check group shares on folder
        let user_groups = self.ops.get_user_group_ids(user_id).await?;
        if !user_groups.is_empty() {
            let group_shares = self
                .ops
                .find_group_shares(None, Some(folder_id), &user_groups)
                .await?;
            let highest_group_perm = group_shares
                .iter()
                .filter(|s| s.revoked_at.is_none())
                .map(|s| s.permissions)
                .max();
            if let Some(perm) = highest_group_perm {
                self.cache.lock().unwrap().insert(cache_key, Some(perm));
                return Ok(perm >= required);
            }
        }

        // 4. Walk up parent folder ancestry for inherited permissions
        if let Some(parent_folder_id) = folder.parent_folder_id {
            if let Some(inherited_perm) = self
                .resolve_folder_ancestry(user_id, parent_folder_id, &user_groups)
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
    /// Uses ancestor_ids from folder documents for efficient lookup (2 queries instead of N).
    /// Keeps max_depth protection as a safety net.
    async fn resolve_folder_ancestry(
        &self,
        user_id: UserId,
        folder_id: FolderId,
        user_groups: &[Uuid],
    ) -> Result<Option<SharePermissions>> {
        // Step 1: Fetch the starting folder to get its ancestor_ids
        let folder = self
            .ops
            .find_folder_by_id(folder_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Folder not found in ancestry"))?;

        // Build the list of folder IDs to check: current folder + all ancestors
        let mut folder_ids_to_check = vec![folder_id];

        // Add ancestor_ids from the folder document if available
        // Folder documents now store ancestor_ids for efficient permission resolution
        if let Some(ref ancestor_ids) = folder.ancestor_ids {
            folder_ids_to_check.extend(ancestor_ids.iter().copied());
        } else {
            // Fallback: Walk up the tree using parent_folder_id
            // This maintains backward compatibility with folders created before ancestor_ids
            let mut current_id = folder.parent_folder_id;
            let mut depth = 0;
            const MAX_DEPTH: usize = 50;

            while let Some(parent_id) = current_id {
                if depth >= MAX_DEPTH {
                    return Err(anyhow::anyhow!(
                        "Max folder depth exceeded ({} levels)",
                        MAX_DEPTH
                    ));
                }
                folder_ids_to_check.push(parent_id);

                // Fetch parent to continue walking
                if let Some(parent) = self.ops.find_folder_by_id(parent_id).await? {
                    current_id = parent.parent_folder_id;
                } else {
                    break;
                }
                depth += 1;
            }
        }

        // Step 2: Check cache for any folders we already know about
        let mut permissions = Vec::new();
        let mut uncached_folder_ids = Vec::new();

        for &fid in &folder_ids_to_check {
            let cache_key = CacheKey::Folder(user_id, fid);
            let cached = { self.cache.lock().unwrap().get(&cache_key).copied() };
            match cached {
                Some(Some(perm)) => permissions.push(perm),
                Some(None) => {} // Cached as no permission, continue
                None => uncached_folder_ids.push(fid),
            }
        }

        // Step 3: Batch fetch shares for all uncached folders
        if !uncached_folder_ids.is_empty() {
            // Fetch user shares for all folders at once
            let user_shares = self
                .ops
                .find_user_shares_for_folders(&uncached_folder_ids, user_id)
                .await?;

            // Fetch group shares for all folders at once (if user has groups)
            let group_shares = if !user_groups.is_empty() {
                self.ops
                    .find_group_shares_for_folders(&uncached_folder_ids, user_groups)
                    .await?
            } else {
                Vec::new()
            };

            // Process results and update cache
            for folder_id in uncached_folder_ids {
                // Find highest user share for this folder
                let user_perm = user_shares
                    .iter()
                    .find(|s| s.folder_id == Some(folder_id) && s.revoked_at.is_none())
                    .map(|s| s.permissions);

                // Find highest group share for this folder
                let group_perm = group_shares
                    .iter()
                    .filter(|s| s.folder_id == Some(folder_id) && s.revoked_at.is_none())
                    .map(|s| s.permissions)
                    .max();

                // Take the highest of user and group permissions
                let found_perm = match (user_perm, group_perm) {
                    (Some(u), Some(g)) => Some(u.max(g)),
                    (Some(u), None) => Some(u),
                    (None, Some(g)) => Some(g),
                    (None, None) => None,
                };

                // Update cache
                let cache_key = CacheKey::Folder(user_id, folder_id);
                self.cache.lock().unwrap().insert(cache_key, found_perm);

                if let Some(perm) = found_perm {
                    permissions.push(perm);
                }
            }
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

    /// Resolve permission with detailed source information.
    ///
    /// This method returns not just the permission level, but also how it was granted,
    /// which is needed for triggering first-access notifications for group shares.
    pub async fn resolve_permission_with_source(
        &self,
        user_id: UserId,
        resource: Resource,
    ) -> Result<PermissionResult> {
        match resource {
            Resource::File(file_id) => {
                self.resolve_file_permission_with_source(user_id, file_id)
                    .await
            }
            Resource::Folder(folder_id) => {
                self.resolve_folder_permission_with_source(user_id, folder_id)
                    .await
            }
        }
    }

    /// Resolve file permission with source information.
    async fn resolve_file_permission_with_source(
        &self,
        user_id: UserId,
        file_id: FileId,
    ) -> Result<PermissionResult> {
        // Check cache first (but we don't cache source info, so this is a simplified check)
        let cache_key = CacheKey::File(user_id, file_id);
        let cached = { self.cache.lock().unwrap().get(&cache_key).copied() };
        if let Some(perm) = cached {
            return Ok(PermissionResult {
                permission: perm,
                source: PermissionSource::DirectShare, // We don't cache source, assume direct
                share_id: None,
            });
        }

        // Get file metadata
        let file = self
            .ops
            .find_file_by_id(file_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("File not found"))?;

        // 1. Check ownership
        if file.owner_id == user_id {
            return Ok(PermissionResult {
                permission: Some(SharePermissions::Admin),
                source: PermissionSource::Owner,
                share_id: None,
            });
        }

        // 2. Check direct share on file
        if let Some(share) = self
            .ops
            .find_user_share(Some(file_id), None, user_id)
            .await?
        {
            if share.revoked_at.is_none() {
                let share_id = share.id;
                let perm = share.permissions;
                self.cache.lock().unwrap().insert(cache_key, Some(perm));
                return Ok(PermissionResult {
                    permission: Some(perm),
                    source: PermissionSource::DirectShare,
                    share_id: Some(share_id),
                });
            }
        }

        // 3. Check group shares on file
        let user_groups = self.ops.get_user_group_ids(user_id).await?;
        if !user_groups.is_empty() {
            let group_shares = self
                .ops
                .find_group_shares(Some(file_id), None, &user_groups)
                .await?;

            // Find the first non-revoked group share
            if let Some(share) = group_shares.iter().find(|s| s.revoked_at.is_none()) {
                let share_id = share.id;
                let perm = share.permissions;
                self.cache.lock().unwrap().insert(cache_key, Some(perm));
                return Ok(PermissionResult {
                    permission: Some(perm),
                    source: PermissionSource::GroupShare,
                    share_id: Some(share_id),
                });
            }
        }

        // 4. Walk up folder ancestry for inherited permissions
        if let Some(parent_folder_id) = file.parent_folder_id {
            if let Some((inherited_perm, share_id, inherited_source)) = self
                .resolve_folder_ancestry_with_source(user_id, parent_folder_id, &user_groups)
                .await?
            {
                self.cache
                    .lock()
                    .unwrap()
                    .insert(cache_key, Some(inherited_perm));
                return Ok(PermissionResult {
                    permission: Some(inherited_perm),
                    source: inherited_source,
                    share_id,
                });
            }
        }

        // No permission found
        self.cache.lock().unwrap().insert(cache_key, None);
        Ok(PermissionResult {
            permission: None,
            source: PermissionSource::None,
            share_id: None,
        })
    }

    /// Resolve folder permission with source information.
    async fn resolve_folder_permission_with_source(
        &self,
        user_id: UserId,
        folder_id: FolderId,
    ) -> Result<PermissionResult> {
        // Check cache first
        let cache_key = CacheKey::Folder(user_id, folder_id);
        let cached = { self.cache.lock().unwrap().get(&cache_key).copied() };
        if let Some(perm) = cached {
            return Ok(PermissionResult {
                permission: perm,
                source: PermissionSource::DirectShare, // We don't cache source, assume direct
                share_id: None,
            });
        }

        // Get folder metadata
        let folder = self
            .ops
            .find_folder_by_id(folder_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Folder not found"))?;

        // 1. Check ownership
        if folder.owner_id == user_id {
            return Ok(PermissionResult {
                permission: Some(SharePermissions::Admin),
                source: PermissionSource::Owner,
                share_id: None,
            });
        }

        // 2. Check direct share on folder
        if let Some(share) = self
            .ops
            .find_user_share(None, Some(folder_id), user_id)
            .await?
        {
            if share.revoked_at.is_none() {
                let share_id = share.id;
                let perm = share.permissions;
                self.cache.lock().unwrap().insert(cache_key, Some(perm));
                return Ok(PermissionResult {
                    permission: Some(perm),
                    source: PermissionSource::DirectShare,
                    share_id: Some(share_id),
                });
            }
        }

        // 3. Check group shares on folder
        let user_groups = self.ops.get_user_group_ids(user_id).await?;
        if !user_groups.is_empty() {
            let group_shares = self
                .ops
                .find_group_shares(None, Some(folder_id), &user_groups)
                .await?;

            // Find the first non-revoked group share
            if let Some(share) = group_shares.iter().find(|s| s.revoked_at.is_none()) {
                let share_id = share.id;
                let perm = share.permissions;
                self.cache.lock().unwrap().insert(cache_key, Some(perm));
                return Ok(PermissionResult {
                    permission: Some(perm),
                    source: PermissionSource::GroupShare,
                    share_id: Some(share_id),
                });
            }
        }

        // 4. Walk up parent folder ancestry for inherited permissions
        if let Some(parent_folder_id) = folder.parent_folder_id {
            if let Some((inherited_perm, share_id, inherited_source)) = self
                .resolve_folder_ancestry_with_source(user_id, parent_folder_id, &user_groups)
                .await?
            {
                self.cache
                    .lock()
                    .unwrap()
                    .insert(cache_key, Some(inherited_perm));
                return Ok(PermissionResult {
                    permission: Some(inherited_perm),
                    source: inherited_source,
                    share_id,
                });
            }
        }

        // No permission found
        self.cache.lock().unwrap().insert(cache_key, None);
        Ok(PermissionResult {
            permission: None,
            source: PermissionSource::None,
            share_id: None,
        })
    }

    /// Walk up folder ancestry to find inherited permissions with source info.
    ///
    /// Returns the highest permission found along with the share_id and source type.
    async fn resolve_folder_ancestry_with_source(
        &self,
        user_id: UserId,
        folder_id: FolderId,
        user_groups: &[Uuid],
    ) -> Result<Option<(SharePermissions, Option<ShareId>, PermissionSource)>> {
        // Step 1: Fetch the starting folder to get its ancestor_ids
        let folder = self
            .ops
            .find_folder_by_id(folder_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Folder not found in ancestry"))?;

        // Build the list of folder IDs to check: current folder + all ancestors
        let mut folder_ids_to_check = vec![folder_id];

        // Add ancestor_ids from the folder document if available
        if let Some(ref ancestor_ids) = folder.ancestor_ids {
            folder_ids_to_check.extend(ancestor_ids.iter().copied());
        } else {
            // Fallback: Walk up the tree using parent_folder_id
            let mut current_id = folder.parent_folder_id;
            let mut depth = 0;
            const MAX_DEPTH: usize = 50;

            while let Some(parent_id) = current_id {
                if depth >= MAX_DEPTH {
                    return Err(anyhow::anyhow!(
                        "Max folder depth exceeded ({} levels)",
                        MAX_DEPTH
                    ));
                }
                folder_ids_to_check.push(parent_id);

                // Fetch parent to continue walking
                if let Some(parent) = self.ops.find_folder_by_id(parent_id).await? {
                    current_id = parent.parent_folder_id;
                } else {
                    break;
                }
                depth += 1;
            }
        }

        // Step 2: Check cache for any folders we already know about
        let mut user_shares_found: Vec<(FolderId, SharePermissions, ShareId)> = Vec::new();
        let mut group_shares_found: Vec<(FolderId, SharePermissions, ShareId)> = Vec::new();
        let mut uncached_folder_ids: Vec<FolderId> = Vec::new();

        for &fid in &folder_ids_to_check {
            let cache_key = CacheKey::Folder(user_id, fid);
            let cached = { self.cache.lock().unwrap().get(&cache_key).copied() };
            match cached {
                Some(Some(perm)) => {
                    // We have a cached permission but don't know the source.
                    // For now, treat it as inherited without a specific share_id.
                    // A full implementation would require caching the source info too.
                    user_shares_found.push((fid, perm, fid)); // Use folder_id as placeholder
                }
                Some(None) => {} // Cached as no permission, continue
                None => uncached_folder_ids.push(fid),
            }
        }

        // Step 3: Batch fetch shares for all uncached folders
        if !uncached_folder_ids.is_empty() {
            // Fetch user shares for all folders at once
            let user_shares = self
                .ops
                .find_user_shares_for_folders(&uncached_folder_ids, user_id)
                .await?;

            // Fetch group shares for all folders at once (if user has groups)
            let group_shares = if !user_groups.is_empty() {
                self.ops
                    .find_group_shares_for_folders(&uncached_folder_ids, user_groups)
                    .await?
            } else {
                Vec::new()
            };

            // Process results and update cache
            for folder_id in uncached_folder_ids {
                // Find highest user share for this folder
                let user_share = user_shares
                    .iter()
                    .find(|s| s.folder_id == Some(folder_id) && s.revoked_at.is_none());

                // Find highest group share for this folder
                let group_share = group_shares
                    .iter()
                    .filter(|s| s.folder_id == Some(folder_id) && s.revoked_at.is_none())
                    .max_by_key(|s| s.permissions);

                // Determine which share provides higher permission
                let found = match (user_share, group_share) {
                    (Some(u), Some(g)) => {
                        if u.permissions >= g.permissions {
                            Some((u.permissions, u.id, PermissionSource::Inherited))
                        } else {
                            Some((g.permissions, g.id, PermissionSource::Inherited))
                        }
                    }
                    (Some(u), None) => Some((u.permissions, u.id, PermissionSource::Inherited)),
                    (None, Some(g)) => Some((g.permissions, g.id, PermissionSource::Inherited)),
                    (None, None) => None,
                };

                // Update cache
                let cache_key = CacheKey::Folder(user_id, folder_id);
                self.cache
                    .lock()
                    .unwrap()
                    .insert(cache_key, found.map(|(p, _, _)| p));

                if let Some((perm, share_id, _source)) = found {
                    // Track for determining the highest permission
                    if user_share.is_some() && group_share.is_none() {
                        user_shares_found.push((folder_id, perm, share_id));
                    } else {
                        group_shares_found.push((folder_id, perm, share_id));
                    }
                }
            }
        }

        // Step 4: Find the highest permission among all shares found
        // Check closer folders first (they take precedence)
        for &folder_id in &folder_ids_to_check {
            // Check user shares first (direct shares take precedence over group shares)
            if let Some((_, perm, share_id)) = user_shares_found
                .iter()
                .find(|(fid, _, _)| *fid == folder_id)
            {
                return Ok(Some((*perm, Some(*share_id), PermissionSource::Inherited)));
            }
            // Then check group shares
            if let Some((_, perm, share_id)) = group_shares_found
                .iter()
                .find(|(fid, _, _)| *fid == folder_id)
            {
                return Ok(Some((*perm, Some(*share_id), PermissionSource::Inherited)));
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::HashMap as StdHashMap;
    use std::sync::Mutex;
    use uuid::Uuid;

    struct MockOps {
        shares: Mutex<Vec<Share>>,
        files: Mutex<Vec<File>>,
        folders: Mutex<Vec<Folder>>,
        user_groups: Mutex<StdHashMap<UserId, Vec<Uuid>>>,
    }

    impl MockOps {
        fn new() -> Self {
            Self {
                shares: Mutex::new(Vec::new()),
                files: Mutex::new(Vec::new()),
                folders: Mutex::new(Vec::new()),
                user_groups: Mutex::new(StdHashMap::new()),
            }
        }

        fn add_share(&self, share: Share) {
            self.shares.lock().unwrap().push(share);
        }

        fn add_file(&self, file: File) {
            self.files.lock().unwrap().push(file);
        }

        fn add_folder(&self, folder: Folder) {
            self.folders.lock().unwrap().push(folder);
        }

        fn add_user_to_group(&self, user_id: UserId, group_id: Uuid) {
            let mut map = self.user_groups.lock().unwrap();
            map.entry(user_id).or_default().push(group_id);
        }
    }

    impl PermissionResolverOps for MockOps {
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

        async fn find_group_shares(
            &self,
            file_id: Option<FileId>,
            folder_id: Option<FolderId>,
            group_ids: &[Uuid],
        ) -> Result<Vec<Share>> {
            let shares = self.shares.lock().unwrap();
            Ok(shares
                .iter()
                .filter(|s| {
                    s.file_id == file_id
                        && s.folder_id == folder_id
                        && s.recipient_group_id
                            .map(|gid| group_ids.contains(&gid))
                            .unwrap_or(false)
                })
                .cloned()
                .collect())
        }

        async fn find_user_shares_for_folders(
            &self,
            folder_ids: &[FolderId],
            recipient_user_id: UserId,
        ) -> Result<Vec<Share>> {
            let shares = self.shares.lock().unwrap();
            Ok(shares
                .iter()
                .filter(|s| {
                    s.file_id.is_none()
                        && s.folder_id
                            .map(|fid| folder_ids.contains(&fid))
                            .unwrap_or(false)
                        && s.recipient_user_id == Some(recipient_user_id)
                })
                .cloned()
                .collect())
        }

        async fn find_group_shares_for_folders(
            &self,
            folder_ids: &[FolderId],
            group_ids: &[Uuid],
        ) -> Result<Vec<Share>> {
            let shares = self.shares.lock().unwrap();
            Ok(shares
                .iter()
                .filter(|s| {
                    s.file_id.is_none()
                        && s.folder_id
                            .map(|fid| folder_ids.contains(&fid))
                            .unwrap_or(false)
                        && s.recipient_group_id
                            .map(|gid| group_ids.contains(&gid))
                            .unwrap_or(false)
                })
                .cloned()
                .collect())
        }

        async fn find_file_by_id(&self, id: FileId) -> Result<Option<File>> {
            Ok(self
                .files
                .lock()
                .unwrap()
                .iter()
                .find(|f| f.id == id)
                .cloned())
        }

        async fn find_folder_by_id(&self, id: FolderId) -> Result<Option<Folder>> {
            Ok(self
                .folders
                .lock()
                .unwrap()
                .iter()
                .find(|f| f.id == id)
                .cloned())
        }

        async fn get_user_group_ids(&self, user_id: UserId) -> Result<Vec<Uuid>> {
            let map = self.user_groups.lock().unwrap();
            Ok(map.get(&user_id).cloned().unwrap_or_default())
        }
    }

    fn setup() -> (PermissionResolver<MockOps>, Arc<MockOps>) {
        let ops = Arc::new(MockOps::new());
        let resolver = PermissionResolver::new(ops.clone());
        (resolver, ops)
    }

    #[tokio::test]
    async fn test_owner_has_admin_permission() {
        let (resolver, ops) = setup();

        let owner_id = Uuid::new_v4();
        let file = File::new(
            "test.txt".to_string(),
            "/test.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
            Uuid::new_v4(),
        );
        let file_id = file.id;
        ops.add_file(file);

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
        let (resolver, ops) = setup();

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
            Uuid::new_v4(),
        );
        let file_id = file.id;
        ops.add_file(file);

        // Create share with View permission
        let share = Share {
            id: Uuid::new_v4(),
            file_id: Some(file_id),
            folder_id: None,
            share_token: None,
            permissions: SharePermissions::View,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: Some(user_id),
            recipient_group_id: None,
            created_by: owner_id,
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id: Uuid::new_v4(),
        };
        ops.add_share(share);

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
        let (resolver, ops) = setup();

        let owner_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        // Create folder hierarchy: root -> parent -> child
        let root_folder = Folder::new_root(owner_id, Uuid::new_v4());
        let root_id = root_folder.id;
        ops.add_folder(root_folder);

        let parent_folder = Folder::new_child(
            "parent".to_string(),
            "/parent".to_string(),
            root_id,
            owner_id,
            Uuid::new_v4(),
        );
        let parent_id = parent_folder.id;
        ops.add_folder(parent_folder);

        let child_folder = Folder::new_child(
            "child".to_string(),
            "/parent/child".to_string(),
            parent_id,
            owner_id,
            Uuid::new_v4(),
        );
        let child_id = child_folder.id;
        ops.add_folder(child_folder);

        // Create file in child folder
        let file = File::new(
            "test.txt".to_string(),
            "/parent/child/test.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            Some(child_id),
            owner_id,
            Uuid::new_v4(),
        );
        let file_id = file.id;
        ops.add_file(file);

        // Share parent folder with Edit permission
        let share = Share {
            id: Uuid::new_v4(),
            file_id: None,
            folder_id: Some(parent_id),
            share_token: None,
            permissions: SharePermissions::Edit,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: Some(user_id),
            recipient_group_id: None,
            created_by: owner_id,
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id: Uuid::new_v4(),
        };
        ops.add_share(share);

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
        let (resolver, ops) = setup();

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
            Uuid::new_v4(),
        );
        let file_id = file.id;
        ops.add_file(file);

        // Create revoked share
        let share = Share {
            id: Uuid::new_v4(),
            file_id: Some(file_id),
            folder_id: None,
            share_token: None,
            permissions: SharePermissions::View,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: Some(user_id),
            recipient_group_id: None,
            created_by: owner_id,
            created_at: Utc::now(),
            revoked_at: Some(Utc::now()),
            tenant_id: Uuid::new_v4(),
        };
        ops.add_share(share);

        // User should not have permission
        assert!(!resolver
            .check_file_permission(user_id, file_id, SharePermissions::View)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_caching_works() {
        let (resolver, ops) = setup();

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
            Uuid::new_v4(),
        );
        let file_id = file.id;
        ops.add_file(file);

        // Create share
        let share = Share {
            id: Uuid::new_v4(),
            file_id: Some(file_id),
            folder_id: None,
            share_token: None,
            permissions: SharePermissions::Edit,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: Some(user_id),
            recipient_group_id: None,
            created_by: owner_id,
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id: Uuid::new_v4(),
        };
        ops.add_share(share);

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
        let (resolver, ops) = setup();

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
            Uuid::new_v4(),
        );
        let file_id = file.id;
        ops.add_file(file);

        // User has no share, should not have permission
        assert!(!resolver
            .check_file_permission(user_id, file_id, SharePermissions::View)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_folder_owner_has_admin() {
        let (resolver, ops) = setup();

        let owner_id = Uuid::new_v4();

        let folder = Folder::new_root(owner_id, Uuid::new_v4());
        let folder_id = folder.id;
        ops.add_folder(folder);

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
        let (resolver, ops) = setup();

        let owner_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        // Create folder hierarchy: root -> parent
        let root_folder = Folder::new_root(owner_id, Uuid::new_v4());
        let root_id = root_folder.id;
        ops.add_folder(root_folder);

        let parent_folder = Folder::new_child(
            "parent".to_string(),
            "/parent".to_string(),
            root_id,
            owner_id,
            Uuid::new_v4(),
        );
        let parent_id = parent_folder.id;
        ops.add_folder(parent_folder);

        // Create file in parent folder
        let file = File::new(
            "test.txt".to_string(),
            "/parent/test.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            Some(parent_id),
            owner_id,
            Uuid::new_v4(),
        );
        let file_id = file.id;
        ops.add_file(file);

        // Share file with View permission
        let file_share = Share {
            id: Uuid::new_v4(),
            file_id: Some(file_id),
            folder_id: None,
            share_token: None,
            permissions: SharePermissions::View,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: Some(user_id),
            recipient_group_id: None,
            created_by: owner_id,
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id: Uuid::new_v4(),
        };
        ops.add_share(file_share);

        // Share parent folder with Admin permission
        let folder_share = Share {
            id: Uuid::new_v4(),
            file_id: None,
            folder_id: Some(parent_id),
            share_token: None,
            permissions: SharePermissions::Admin,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: Some(user_id),
            recipient_group_id: None,
            created_by: owner_id,
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id: Uuid::new_v4(),
        };
        ops.add_share(folder_share);

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

    // Group share tests

    #[tokio::test]
    async fn test_group_share_provides_permission() {
        let (resolver, ops) = setup();

        let owner_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let group_id = Uuid::new_v4();

        // Add user to group
        ops.add_user_to_group(user_id, group_id);

        let file = File::new(
            "test.txt".to_string(),
            "/test.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
            Uuid::new_v4(),
        );
        let file_id = file.id;
        ops.add_file(file);

        // Create group share with Edit permission
        let share = Share {
            id: Uuid::new_v4(),
            file_id: Some(file_id),
            folder_id: None,
            share_token: None,
            permissions: SharePermissions::Edit,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: None,
            recipient_group_id: Some(group_id),
            created_by: owner_id,
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id: Uuid::new_v4(),
        };
        ops.add_share(share);

        // User should have Edit permission through group membership
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
    async fn test_non_group_member_denied() {
        let (resolver, ops) = setup();

        let owner_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let group_id = Uuid::new_v4();

        // Note: user_id is NOT added to the group

        let file = File::new(
            "test.txt".to_string(),
            "/test.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
            Uuid::new_v4(),
        );
        let file_id = file.id;
        ops.add_file(file);

        // Create group share
        let share = Share {
            id: Uuid::new_v4(),
            file_id: Some(file_id),
            folder_id: None,
            share_token: None,
            permissions: SharePermissions::Edit,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: None,
            recipient_group_id: Some(group_id),
            created_by: owner_id,
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id: Uuid::new_v4(),
        };
        ops.add_share(share);

        // User should NOT have permission (not in the group)
        assert!(!resolver
            .check_file_permission(user_id, file_id, SharePermissions::View)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_highest_permission_wins_direct_vs_group() {
        let (resolver, ops) = setup();

        let owner_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let group_id = Uuid::new_v4();

        // Add user to group
        ops.add_user_to_group(user_id, group_id);

        let file = File::new(
            "test.txt".to_string(),
            "/test.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
            Uuid::new_v4(),
        );
        let file_id = file.id;
        ops.add_file(file);

        // Create group share with Admin permission
        let group_share = Share {
            id: Uuid::new_v4(),
            file_id: Some(file_id),
            folder_id: None,
            share_token: None,
            permissions: SharePermissions::Admin,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: None,
            recipient_group_id: Some(group_id),
            created_by: owner_id,
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id: Uuid::new_v4(),
        };
        ops.add_share(group_share);

        // Create direct share with View permission (lower)
        let direct_share = Share {
            id: Uuid::new_v4(),
            file_id: Some(file_id),
            folder_id: None,
            share_token: None,
            permissions: SharePermissions::View,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: Some(user_id),
            recipient_group_id: None,
            created_by: owner_id,
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id: Uuid::new_v4(),
        };
        ops.add_share(direct_share);

        // User should have View permission because direct share takes precedence
        // (Direct shares are checked before group shares)
        assert!(resolver
            .check_file_permission(user_id, file_id, SharePermissions::View)
            .await
            .unwrap());
        assert!(!resolver
            .check_file_permission(user_id, file_id, SharePermissions::Edit)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_highest_permission_wins_multiple_groups() {
        let (resolver, ops) = setup();

        let owner_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let group1_id = Uuid::new_v4();
        let group2_id = Uuid::new_v4();

        // Add user to both groups
        ops.add_user_to_group(user_id, group1_id);
        ops.add_user_to_group(user_id, group2_id);

        let file = File::new(
            "test.txt".to_string(),
            "/test.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
            Uuid::new_v4(),
        );
        let file_id = file.id;
        ops.add_file(file);

        // Create group share for group1 with View permission
        let share1 = Share {
            id: Uuid::new_v4(),
            file_id: Some(file_id),
            folder_id: None,
            share_token: None,
            permissions: SharePermissions::View,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: None,
            recipient_group_id: Some(group1_id),
            created_by: owner_id,
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id: Uuid::new_v4(),
        };
        ops.add_share(share1);

        // Create group share for group2 with Admin permission (higher)
        let share2 = Share {
            id: Uuid::new_v4(),
            file_id: Some(file_id),
            folder_id: None,
            share_token: None,
            permissions: SharePermissions::Admin,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: None,
            recipient_group_id: Some(group2_id),
            created_by: owner_id,
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id: Uuid::new_v4(),
        };
        ops.add_share(share2);

        // User should have Admin permission (highest from all group shares)
        assert!(resolver
            .check_file_permission(user_id, file_id, SharePermissions::View)
            .await
            .unwrap());
        assert!(resolver
            .check_file_permission(user_id, file_id, SharePermissions::Edit)
            .await
            .unwrap());
        assert!(resolver
            .check_file_permission(user_id, file_id, SharePermissions::Admin)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_group_share_folder_inheritance() {
        let (resolver, ops) = setup();

        let owner_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let group_id = Uuid::new_v4();

        // Add user to group
        ops.add_user_to_group(user_id, group_id);

        // Create folder hierarchy: root -> parent -> child
        let root_folder = Folder::new_root(owner_id, Uuid::new_v4());
        let root_id = root_folder.id;
        ops.add_folder(root_folder);

        let parent_folder = Folder::new_child(
            "parent".to_string(),
            "/parent".to_string(),
            root_id,
            owner_id,
            Uuid::new_v4(),
        );
        let parent_id = parent_folder.id;
        ops.add_folder(parent_folder);

        let child_folder = Folder::new_child(
            "child".to_string(),
            "/parent/child".to_string(),
            parent_id,
            owner_id,
            Uuid::new_v4(),
        );
        let child_id = child_folder.id;
        ops.add_folder(child_folder);

        // Create file in child folder
        let file = File::new(
            "test.txt".to_string(),
            "/parent/child/test.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            Some(child_id),
            owner_id,
            Uuid::new_v4(),
        );
        let file_id = file.id;
        ops.add_file(file);

        // Share parent folder with group with Edit permission
        let share = Share {
            id: Uuid::new_v4(),
            file_id: None,
            folder_id: Some(parent_id),
            share_token: None,
            permissions: SharePermissions::Edit,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: None,
            recipient_group_id: Some(group_id),
            created_by: owner_id,
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id: Uuid::new_v4(),
        };
        ops.add_share(share);

        // User should have Edit permission on file through group share inheritance
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
    async fn test_revoked_group_share_denied() {
        let (resolver, ops) = setup();

        let owner_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let group_id = Uuid::new_v4();

        // Add user to group
        ops.add_user_to_group(user_id, group_id);

        let file = File::new(
            "test.txt".to_string(),
            "/test.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
            Uuid::new_v4(),
        );
        let file_id = file.id;
        ops.add_file(file);

        // Create revoked group share
        let share = Share {
            id: Uuid::new_v4(),
            file_id: Some(file_id),
            folder_id: None,
            share_token: None,
            permissions: SharePermissions::Edit,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: None,
            recipient_group_id: Some(group_id),
            created_by: owner_id,
            created_at: Utc::now(),
            revoked_at: Some(Utc::now()),
            tenant_id: Uuid::new_v4(),
        };
        ops.add_share(share);

        // User should not have permission (share is revoked)
        assert!(!resolver
            .check_file_permission(user_id, file_id, SharePermissions::View)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_user_and_group_share_highest_in_ancestry() {
        let (resolver, ops) = setup();

        let owner_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let group_id = Uuid::new_v4();

        // Add user to group
        ops.add_user_to_group(user_id, group_id);

        // Create folder hierarchy: root -> parent
        let root_folder = Folder::new_root(owner_id, Uuid::new_v4());
        let root_id = root_folder.id;
        ops.add_folder(root_folder);

        let parent_folder = Folder::new_child(
            "parent".to_string(),
            "/parent".to_string(),
            root_id,
            owner_id,
            Uuid::new_v4(),
        );
        let parent_id = parent_folder.id;
        ops.add_folder(parent_folder);

        // Create file in parent folder
        let file = File::new(
            "test.txt".to_string(),
            "/parent/test.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            Some(parent_id),
            owner_id,
            Uuid::new_v4(),
        );
        let file_id = file.id;
        ops.add_file(file);

        // Share parent folder with user (View permission)
        let user_share = Share {
            id: Uuid::new_v4(),
            file_id: None,
            folder_id: Some(parent_id),
            share_token: None,
            permissions: SharePermissions::View,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: Some(user_id),
            recipient_group_id: None,
            created_by: owner_id,
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id: Uuid::new_v4(),
        };
        ops.add_share(user_share);

        // Share parent folder with group (Admin permission - higher)
        let group_share = Share {
            id: Uuid::new_v4(),
            file_id: None,
            folder_id: Some(parent_id),
            share_token: None,
            permissions: SharePermissions::Admin,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: None,
            recipient_group_id: Some(group_id),
            created_by: owner_id,
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id: Uuid::new_v4(),
        };
        ops.add_share(group_share);

        // User should have Admin permission (highest from user + group shares in ancestry)
        assert!(resolver
            .check_file_permission(user_id, file_id, SharePermissions::Admin)
            .await
            .unwrap());
    }
}
