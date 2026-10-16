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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

tokio::task_local! {
    static PERMISSION_CACHE_EPOCH: u64;
}

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

/// Shared resource authorization interface used by share management.
#[async_trait::async_trait]
pub trait ResourcePermissionChecker: Send + Sync {
    async fn check_resource_permission(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
        resource: Resource,
        requested: SharePermissions,
    ) -> Result<bool>;
}

/// Cache key for permission lookups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum CacheKey {
    File(UserId, Uuid, FileId),
    Folder(UserId, Uuid, FolderId),
}

/// Combined trait for all operations needed by PermissionResolver.
///
/// This trait combines share resolution, file/folder metadata, and group membership
/// operations to simplify the generic bounds for PermissionResolver.
#[async_trait::async_trait]
pub trait PermissionResolverOps: Send + Sync {
    /// Check that the actor is a member of the tenant being authorized.
    async fn user_belongs_to_tenant(&self, user_id: UserId, tenant_id: Uuid) -> Result<bool>;

    /// Find a user share by resource and recipient, scoped to a tenant.
    async fn find_user_share(
        &self,
        file_id: Option<FileId>,
        folder_id: Option<FolderId>,
        recipient_user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Option<Share>>;

    /// Find group shares by resource and group IDs, scoped to a tenant.
    /// Returns all shares where recipient_group_id matches any of the provided group_ids.
    async fn find_group_shares(
        &self,
        file_id: Option<FileId>,
        folder_id: Option<FolderId>,
        group_ids: &[Uuid],
        tenant_id: Uuid,
    ) -> Result<Vec<Share>>;

    /// Find user shares for multiple folders at once, scoped to a tenant.
    /// Returns all shares where the resource is one of the folder_ids and recipient matches user_id.
    async fn find_user_shares_for_folders(
        &self,
        folder_ids: &[FolderId],
        recipient_user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Vec<Share>>;

    /// Find group shares for multiple folders at once, scoped to a tenant.
    /// Returns all shares where the resource is one of the folder_ids and recipient_group_id matches any group_ids.
    async fn find_group_shares_for_folders(
        &self,
        folder_ids: &[FolderId],
        group_ids: &[Uuid],
        tenant_id: Uuid,
    ) -> Result<Vec<Share>>;

    /// Find a file by ID, scoped to a tenant.
    async fn find_file_by_id(&self, id: FileId, tenant_id: Uuid) -> Result<Option<File>>;

    /// Find a folder by ID, scoped to a tenant.
    async fn find_folder_by_id(&self, id: FolderId, tenant_id: Uuid) -> Result<Option<Folder>>;

    /// Get all group IDs that a user is a member of, scoped to a tenant.
    async fn get_user_group_ids(&self, user_id: UserId, tenant_id: Uuid) -> Result<Vec<Uuid>>;

    /// Find all active user shares on a file (no recipient filter).
    async fn find_all_user_shares_for_file(
        &self,
        file_id: FileId,
        tenant_id: Uuid,
    ) -> Result<Vec<Share>>;

    /// Find all active group shares on a file (no recipient filter).
    async fn find_all_group_shares_for_file(
        &self,
        file_id: FileId,
        tenant_id: Uuid,
    ) -> Result<Vec<Share>>;

    /// Find all active user shares on any of the given folders.
    async fn find_all_user_shares_for_folders(
        &self,
        folder_ids: &[FolderId],
        tenant_id: Uuid,
    ) -> Result<Vec<Share>>;

    /// Find all active group shares on any of the given folders.
    async fn find_all_group_shares_for_folders(
        &self,
        folder_ids: &[FolderId],
        tenant_id: Uuid,
    ) -> Result<Vec<Share>>;
}

/// PermissionResolver service handles permission checks with caching and folder inheritance.
///
/// Generic over PermissionResolverOps implementations to support different backends
/// and testing with mock implementations.
pub struct PermissionResolver<Ops: PermissionResolverOps> {
    ops: Arc<Ops>,
    cache: RwLock<HashMap<CacheKey, (u64, PermissionResult)>>,
    cache_epoch: AtomicU64,
}

impl<Ops: PermissionResolverOps> PermissionResolver<Ops> {
    /// Create a new PermissionResolver instance.
    pub fn new(ops: Arc<Ops>) -> Self {
        Self {
            ops,
            cache: RwLock::new(HashMap::new()),
            cache_epoch: AtomicU64::new(0),
        }
    }

    /// Check if a share is currently active (not revoked and not expired).
    fn is_share_active(share: &Share) -> bool {
        share.is_active()
    }

    async fn cache_result(&self, key: CacheKey, result: PermissionResult) {
        let epoch = PERMISSION_CACHE_EPOCH
            .try_with(|epoch| *epoch)
            .unwrap_or_else(|_| self.cache_epoch.load(Ordering::SeqCst));
        let mut cache = self.cache.write().await;
        if epoch == self.cache_epoch.load(Ordering::SeqCst) {
            cache.insert(key, (epoch, result));
        }
    }

    async fn cached_result(&self, key: CacheKey) -> Option<PermissionResult> {
        let epoch = PERMISSION_CACHE_EPOCH.try_with(|epoch| *epoch).ok()?;
        self.cache
            .read()
            .await
            .get(&key)
            .filter(|(entry_epoch, _)| *entry_epoch == epoch)
            .map(|(_, result)| *result)
    }

    async fn cache_permission(
        &self,
        key: CacheKey,
        permission: Option<SharePermissions>,
        source: PermissionSource,
        share_id: Option<ShareId>,
    ) {
        self.cache_result(
            key,
            PermissionResult {
                permission,
                source,
                share_id,
            },
        )
        .await;
    }

    /// Check if user has required permission on file within a tenant.
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
        tenant_id: Uuid,
        file_id: FileId,
        required: SharePermissions,
    ) -> Result<bool> {
        let result = self
            .resolve_permission_with_source(user_id, tenant_id, Resource::File(file_id))
            .await?;
        Ok(result
            .permission
            .is_some_and(|permission| permission >= required))
    }

    /// Check if user has required permission on folder within a tenant.
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
        tenant_id: Uuid,
        folder_id: FolderId,
        required: SharePermissions,
    ) -> Result<bool> {
        let result = self
            .resolve_permission_with_source(user_id, tenant_id, Resource::Folder(folder_id))
            .await?;
        Ok(result
            .permission
            .is_some_and(|permission| permission >= required))
    }

    /// Walk up folder ancestry to find inherited permissions.
    ///
    /// Returns the highest permission found in the folder tree.
    /// Uses ancestor_ids from folder documents for efficient lookup (2 queries instead of N).
    /// Keeps max_depth protection as a safety net.
    /// Clear the permission cache.
    ///
    /// Should be called at the end of each request to ensure cache doesn't leak between requests.
    pub async fn clear_cache(&self) {
        self.cache_epoch.fetch_add(1, Ordering::SeqCst);
        self.cache.write().await.clear();
    }

    /// Resolve all group IDs that a user is a member of, scoped to a tenant.
    pub async fn resolve_user_group_ids(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Vec<Uuid>> {
        self.ops.get_user_group_ids(user_id, tenant_id).await
    }

    /// Resolve the permission a user has on a resource (file or folder).
    ///
    /// Returns Some(permission) if the user has access, None otherwise.
    /// This is a convenience method that wraps check_file_permission and check_folder_permission.
    pub async fn resolve_permission(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
        resource: Resource,
    ) -> Result<Option<SharePermissions>> {
        Ok(self
            .resolve_permission_with_source(user_id, tenant_id, resource)
            .await?
            .permission)
    }

    /// Resolve every principal that has at least View access to a file.
    ///
    /// Returns a list of strings such as:
    /// - `owner:{user_id}`
    /// - `user:{user_id}`
    /// - `group:{group_id}`
    ///
    /// Public visibility is handled separately by the ACL filter.
    /// Expired or revoked shares are ignored.
    pub async fn resolve_read_principals(
        &self,
        file: &File,
        tenant_id: Uuid,
    ) -> Result<Vec<String>> {
        use std::collections::HashSet;

        let mut principals = HashSet::new();
        principals.insert(format!("owner:{}", file.owner_id));

        // Direct shares on the file.
        let user_shares = self
            .ops
            .find_all_user_shares_for_file(file.id, tenant_id)
            .await?;
        for share in user_shares {
            if Self::is_share_active(&share) && share.permissions >= SharePermissions::View {
                if let Some(uid) = share.recipient_user_id {
                    principals.insert(format!("user:{uid}"));
                }
            }
        }

        let group_shares = self
            .ops
            .find_all_group_shares_for_file(file.id, tenant_id)
            .await?;
        for share in group_shares {
            if Self::is_share_active(&share) && share.permissions >= SharePermissions::View {
                if let Some(gid) = share.recipient_group_id {
                    principals.insert(format!("group:{gid}"));
                }
            }
        }

        // Inherited shares from folder ancestry.
        if let Some(parent_folder_id) = file.parent_folder_id {
            let folder = match self
                .ops
                .find_folder_by_id(parent_folder_id, tenant_id)
                .await?
            {
                Some(f) => f,
                None => {
                    return Ok(principals.into_iter().collect());
                }
            };

            let mut folder_ids = vec![parent_folder_id];
            if let Some(ref ancestor_ids) = folder.ancestor_ids {
                folder_ids.extend(ancestor_ids.iter().copied());
            } else {
                let mut current_id = folder.parent_folder_id;
                let mut depth = 0;
                const MAX_DEPTH: usize = 50;
                while let Some(id) = current_id {
                    if depth >= MAX_DEPTH {
                        break;
                    }
                    folder_ids.push(id);
                    if let Some(parent) = self.ops.find_folder_by_id(id, tenant_id).await? {
                        current_id = parent.parent_folder_id;
                    } else {
                        break;
                    }
                    depth += 1;
                }
            }

            if !folder_ids.is_empty() {
                let folder_user_shares = self
                    .ops
                    .find_all_user_shares_for_folders(&folder_ids, tenant_id)
                    .await?;
                for share in folder_user_shares {
                    if Self::is_share_active(&share) && share.permissions >= SharePermissions::View
                    {
                        if let Some(uid) = share.recipient_user_id {
                            principals.insert(format!("user:{uid}"));
                        }
                    }
                }

                let folder_group_shares = self
                    .ops
                    .find_all_group_shares_for_folders(&folder_ids, tenant_id)
                    .await?;
                for share in folder_group_shares {
                    if Self::is_share_active(&share) && share.permissions >= SharePermissions::View
                    {
                        if let Some(gid) = share.recipient_group_id {
                            principals.insert(format!("group:{gid}"));
                        }
                    }
                }
            }
        }

        Ok(principals.into_iter().collect())
    }

    /// Resolve permission with detailed source information.
    ///
    /// This method returns not just the permission level, but also how it was granted,
    /// which is needed for triggering first-access notifications for group shares.
    pub async fn resolve_permission_with_source(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
        resource: Resource,
    ) -> Result<PermissionResult> {
        let epoch = self.cache_epoch.fetch_add(1, Ordering::SeqCst) + 1;
        PERMISSION_CACHE_EPOCH
            .scope(
                epoch,
                self.resolve_permission_with_source_inner(user_id, tenant_id, resource),
            )
            .await
    }

    async fn resolve_permission_with_source_inner(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
        resource: Resource,
    ) -> Result<PermissionResult> {
        self.cache.write().await.clear();
        if !self.ops.user_belongs_to_tenant(user_id, tenant_id).await? {
            return Ok(PermissionResult {
                permission: None,
                source: PermissionSource::None,
                share_id: None,
            });
        }
        match resource {
            Resource::File(file_id) => {
                self.resolve_file_permission_with_source(user_id, tenant_id, file_id)
                    .await
            }
            Resource::Folder(folder_id) => {
                self.resolve_folder_permission_with_source(user_id, tenant_id, folder_id)
                    .await
            }
        }
    }

    /// Resolve file permission with source information.
    async fn resolve_file_permission_with_source(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
        file_id: FileId,
    ) -> Result<PermissionResult> {
        let cache_key = CacheKey::File(user_id, tenant_id, file_id);
        if let Some(result) = self.cached_result(cache_key).await {
            return Ok(result);
        }
        let Some(file) = self.ops.find_file_by_id(file_id, tenant_id).await? else {
            let result = PermissionResult {
                permission: None,
                source: PermissionSource::None,
                share_id: None,
            };
            self.cache_result(cache_key, result).await;
            return Ok(result);
        };
        if file.owner_id == user_id {
            let result = PermissionResult {
                permission: Some(SharePermissions::Admin),
                source: PermissionSource::Owner,
                share_id: None,
            };
            self.cache_result(cache_key, result).await;
            return Ok(result);
        }
        let user_groups = self.ops.get_user_group_ids(user_id, tenant_id).await?;
        let direct = self
            .ops
            .find_all_user_shares_for_file(file_id, tenant_id)
            .await?;
        let direct = direct
            .iter()
            .filter(|share| {
                share.file_id == Some(file_id)
                    && share.recipient_user_id == Some(user_id)
                    && Self::is_share_active(share)
            })
            .max_by_key(|share| share.permissions);
        let mut best = direct.map(|share| {
            (
                share.permissions,
                PermissionSource::DirectShare,
                Some(share.id),
            )
        });
        if !user_groups.is_empty() {
            let groups = self
                .ops
                .find_group_shares(Some(file_id), None, &user_groups, tenant_id)
                .await?;
            if let Some(share) = groups
                .iter()
                .filter(|share| share.file_id == Some(file_id) && Self::is_share_active(share))
                .max_by_key(|share| share.permissions)
            {
                if best.is_none_or(|current| share.permissions > current.0) {
                    best = Some((
                        share.permissions,
                        PermissionSource::GroupShare,
                        Some(share.id),
                    ));
                }
            }
        }
        if let Some(parent) = file.parent_folder_id {
            if let Some(inherited) = self
                .resolve_folder_ancestry_with_source(user_id, tenant_id, parent, &user_groups)
                .await?
            {
                if best.is_none_or(|current| inherited.0 > current.0) {
                    best = Some((inherited.0, inherited.2, inherited.1));
                }
            }
        }
        let result = best.map_or(
            PermissionResult {
                permission: None,
                source: PermissionSource::None,
                share_id: None,
            },
            |(permission, source, share_id)| PermissionResult {
                permission: Some(permission),
                source,
                share_id,
            },
        );
        self.cache_result(cache_key, result).await;
        Ok(result)
    }

    /// Resolve folder permission with source information.
    async fn resolve_folder_permission_with_source(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
        folder_id: FolderId,
    ) -> Result<PermissionResult> {
        let cache_key = CacheKey::Folder(user_id, tenant_id, folder_id);
        if let Some(result) = self.cached_result(cache_key).await {
            return Ok(result);
        }
        let Some(folder) = self.ops.find_folder_by_id(folder_id, tenant_id).await? else {
            let result = PermissionResult {
                permission: None,
                source: PermissionSource::None,
                share_id: None,
            };
            self.cache_result(cache_key, result).await;
            return Ok(result);
        };
        if folder.owner_id == user_id {
            let result = PermissionResult {
                permission: Some(SharePermissions::Admin),
                source: PermissionSource::Owner,
                share_id: None,
            };
            self.cache_result(cache_key, result).await;
            return Ok(result);
        }
        let user_groups = self.ops.get_user_group_ids(user_id, tenant_id).await?;
        let direct = self
            .ops
            .find_user_shares_for_folders(&[folder_id], user_id, tenant_id)
            .await?;
        let direct = direct
            .iter()
            .filter(|share| share.folder_id == Some(folder_id) && Self::is_share_active(share))
            .max_by_key(|share| share.permissions);
        let mut best = direct.map(|share| {
            (
                share.permissions,
                PermissionSource::DirectShare,
                Some(share.id),
            )
        });
        if !user_groups.is_empty() {
            let groups = self
                .ops
                .find_group_shares(None, Some(folder_id), &user_groups, tenant_id)
                .await?;
            if let Some(share) = groups
                .iter()
                .filter(|share| share.folder_id == Some(folder_id) && Self::is_share_active(share))
                .max_by_key(|share| share.permissions)
            {
                if best.is_none_or(|current| share.permissions > current.0) {
                    best = Some((
                        share.permissions,
                        PermissionSource::GroupShare,
                        Some(share.id),
                    ));
                }
            }
        }
        if let Some(parent) = folder.parent_folder_id {
            if let Some(inherited) = self
                .resolve_folder_ancestry_with_source(user_id, tenant_id, parent, &user_groups)
                .await?
            {
                if best.is_none_or(|current| inherited.0 > current.0) {
                    best = Some((inherited.0, inherited.2, inherited.1));
                }
            }
        }
        let result = best.map_or(
            PermissionResult {
                permission: None,
                source: PermissionSource::None,
                share_id: None,
            },
            |(permission, source, share_id)| PermissionResult {
                permission: Some(permission),
                source,
                share_id,
            },
        );
        self.cache_result(cache_key, result).await;
        Ok(result)
    }

    /// Walk up folder ancestry to find inherited permissions with source info.
    ///
    /// Returns the highest permission found along with the share_id and source type.
    async fn resolve_folder_ancestry_with_source(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
        folder_id: FolderId,
        user_groups: &[Uuid],
    ) -> Result<Option<(SharePermissions, Option<ShareId>, PermissionSource)>> {
        // Step 1: Fetch the starting folder to get its ancestor_ids
        let folder = self
            .ops
            .find_folder_by_id(folder_id, tenant_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Folder not found in ancestry"))?;

        // Folder owners implicitly have Admin on all descendants
        if folder.owner_id == user_id {
            let cache_key = CacheKey::Folder(user_id, tenant_id, folder_id);
            self.cache_permission(
                cache_key,
                Some(SharePermissions::Admin),
                PermissionSource::Owner,
                None,
            )
            .await;
            return Ok(Some((
                SharePermissions::Admin,
                None,
                PermissionSource::Owner,
            )));
        }

        // Build the list of folder IDs to check: current folder + all ancestors
        let mut folder_ids_to_check = vec![folder_id];

        // Add ancestor_ids from the folder document if available
        if let Some(ref ancestor_ids) = folder.ancestor_ids {
            for &ancestor_id in ancestor_ids {
                if let Some(ancestor) = self.ops.find_folder_by_id(ancestor_id, tenant_id).await? {
                    if ancestor.owner_id == user_id {
                        let cache_key = CacheKey::Folder(user_id, tenant_id, ancestor_id);
                        self.cache_permission(
                            cache_key,
                            Some(SharePermissions::Admin),
                            PermissionSource::Owner,
                            None,
                        )
                        .await;
                        return Ok(Some((
                            SharePermissions::Admin,
                            None,
                            PermissionSource::Owner,
                        )));
                    }
                }
                folder_ids_to_check.push(ancestor_id);
            }
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
                if let Some(parent) = self.ops.find_folder_by_id(parent_id, tenant_id).await? {
                    // Ancestor owners also implicitly have Admin
                    if parent.owner_id == user_id {
                        let cache_key = CacheKey::Folder(user_id, tenant_id, parent_id);
                        self.cache_permission(
                            cache_key,
                            Some(SharePermissions::Admin),
                            PermissionSource::Owner,
                            None,
                        )
                        .await;
                        return Ok(Some((
                            SharePermissions::Admin,
                            None,
                            PermissionSource::Owner,
                        )));
                    }
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
            let cache_key = CacheKey::Folder(user_id, tenant_id, fid);
            let cached = self.cached_result(cache_key).await;
            match cached {
                Some(PermissionResult {
                    permission: Some(perm),
                    source,
                    share_id: Some(share_id),
                }) => match source {
                    PermissionSource::GroupShare => {
                        group_shares_found.push((fid, perm, share_id));
                    }
                    _ => {
                        user_shares_found.push((fid, perm, share_id));
                    }
                },
                Some(PermissionResult {
                    permission: Some(perm),
                    source: PermissionSource::Owner,
                    share_id: None,
                }) => {
                    return Ok(Some((perm, None, PermissionSource::Owner)));
                }
                Some(PermissionResult {
                    permission: Some(_),
                    share_id: None,
                    ..
                }) => {}
                Some(PermissionResult {
                    permission: None, ..
                }) => {}
                None => uncached_folder_ids.push(fid),
            }
        }

        // Step 3: Batch fetch shares for all uncached folders
        if !uncached_folder_ids.is_empty() {
            // Fetch user shares for all folders at once
            let user_shares = self
                .ops
                .find_user_shares_for_folders(&uncached_folder_ids, user_id, tenant_id)
                .await?;

            // Fetch group shares for all folders at once (if user has groups)
            let group_shares = if !user_groups.is_empty() {
                self.ops
                    .find_group_shares_for_folders(&uncached_folder_ids, user_groups, tenant_id)
                    .await?
            } else {
                Vec::new()
            };

            // Process results and update cache
            for folder_id in uncached_folder_ids {
                // Find highest user share for this folder
                let user_share = user_shares
                    .iter()
                    .filter(|s| {
                        s.folder_id == Some(folder_id)
                            && s.recipient_user_id == Some(user_id)
                            && Self::is_share_active(s)
                    })
                    .max_by_key(|s| s.permissions);

                // Find highest group share for this folder
                let group_share = group_shares
                    .iter()
                    .filter(|s| {
                        s.folder_id == Some(folder_id)
                            && s.recipient_group_id
                                .is_some_and(|group_id| user_groups.contains(&group_id))
                            && Self::is_share_active(s)
                    })
                    .max_by_key(|s| s.permissions);

                // Determine which share provides higher permission
                let found = match (user_share, group_share) {
                    (Some(u), Some(g)) => {
                        if u.permissions >= g.permissions {
                            Some((u.permissions, u.id, PermissionSource::DirectShare))
                        } else {
                            Some((g.permissions, g.id, PermissionSource::GroupShare))
                        }
                    }
                    (Some(u), None) => Some((u.permissions, u.id, PermissionSource::DirectShare)),
                    (None, Some(g)) => Some((g.permissions, g.id, PermissionSource::GroupShare)),
                    (None, None) => None,
                };

                // Update cache
                let cache_key = CacheKey::Folder(user_id, tenant_id, folder_id);
                if let Some((perm, share_id, source)) = found {
                    self.cache_permission(cache_key, Some(perm), source, Some(share_id))
                        .await;

                    // Track for determining the highest permission
                    match source {
                        PermissionSource::DirectShare => {
                            user_shares_found.push((folder_id, perm, share_id));
                        }
                        PermissionSource::GroupShare => {
                            group_shares_found.push((folder_id, perm, share_id));
                        }
                        _ => {}
                    }
                } else {
                    self.cache_permission(cache_key, None, PermissionSource::None, None)
                        .await;
                }
            }
        }

        let best_user = user_shares_found
            .iter()
            .max_by_key(|(_, permission, _)| *permission);
        let best_group = group_shares_found
            .iter()
            .max_by_key(|(_, permission, _)| *permission);
        let best = match (best_user, best_group) {
            (Some(user), Some(group)) if user.1 >= group.1 => {
                Some((user.1, user.2, PermissionSource::Inherited))
            }
            (Some(_), Some(group)) => Some((group.1, group.2, PermissionSource::Inherited)),
            (Some(user), None) => Some((user.1, user.2, PermissionSource::Inherited)),
            (None, Some(group)) => Some((group.1, group.2, PermissionSource::Inherited)),
            (None, None) => None,
        };
        Ok(best.map(|(permission, share_id, source)| (permission, Some(share_id), source)))
    }
}

#[async_trait::async_trait]
impl<Ops: PermissionResolverOps> ResourcePermissionChecker for PermissionResolver<Ops> {
    async fn check_resource_permission(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
        resource: Resource,
        requested: SharePermissions,
    ) -> Result<bool> {
        let result = self
            .resolve_permission_with_source(user_id, tenant_id, resource)
            .await?;
        Ok(result
            .permission
            .is_some_and(|permission| permission >= requested))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::HashMap as StdHashMap;
    use tokio::sync::Mutex;
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

        async fn add_share(&self, share: Share) {
            self.shares.lock().await.push(share);
        }

        async fn add_file(&self, file: File) {
            self.files.lock().await.push(file);
        }

        async fn add_folder(&self, folder: Folder) {
            self.folders.lock().await.push(folder);
        }

        async fn add_user_to_group(&self, user_id: UserId, group_id: Uuid) {
            let mut map = self.user_groups.lock().await;
            map.entry(user_id).or_default().push(group_id);
        }
    }

    #[async_trait::async_trait]

    impl PermissionResolverOps for MockOps {
        async fn user_belongs_to_tenant(
            &self,
            _user_id: UserId,
            _tenant_id: Uuid,
        ) -> anyhow::Result<bool> {
            Ok(true)
        }

        async fn find_user_share(
            &self,
            file_id: Option<FileId>,
            folder_id: Option<FolderId>,
            recipient_user_id: UserId,
            _tenant_id: Uuid,
        ) -> Result<Option<Share>> {
            let shares = self.shares.lock().await;
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
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            let shares = self.shares.lock().await;
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
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            let shares = self.shares.lock().await;
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
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            let shares = self.shares.lock().await;
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

        async fn find_file_by_id(&self, id: FileId, _tenant_id: Uuid) -> Result<Option<File>> {
            Ok(self.files.lock().await.iter().find(|f| f.id == id).cloned())
        }

        async fn find_folder_by_id(
            &self,
            id: FolderId,
            _tenant_id: Uuid,
        ) -> Result<Option<Folder>> {
            Ok(self
                .folders
                .lock()
                .await
                .iter()
                .find(|f| f.id == id)
                .cloned())
        }

        async fn get_user_group_ids(&self, user_id: UserId, _tenant_id: Uuid) -> Result<Vec<Uuid>> {
            let map = self.user_groups.lock().await;
            Ok(map.get(&user_id).cloned().unwrap_or_default())
        }

        async fn find_all_user_shares_for_file(
            &self,
            file_id: FileId,
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            let shares = self.shares.lock().await;
            Ok(shares
                .iter()
                .filter(|s| {
                    s.file_id == Some(file_id)
                        && s.folder_id.is_none()
                        && s.recipient_user_id.is_some()
                })
                .cloned()
                .collect())
        }

        async fn find_all_group_shares_for_file(
            &self,
            file_id: FileId,
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            let shares = self.shares.lock().await;
            Ok(shares
                .iter()
                .filter(|s| {
                    s.file_id == Some(file_id)
                        && s.folder_id.is_none()
                        && s.recipient_group_id.is_some()
                })
                .cloned()
                .collect())
        }

        async fn find_all_user_shares_for_folders(
            &self,
            folder_ids: &[FolderId],
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            let shares = self.shares.lock().await;
            Ok(shares
                .iter()
                .filter(|s| {
                    s.file_id.is_none()
                        && s.folder_id
                            .map(|fid| folder_ids.contains(&fid))
                            .unwrap_or(false)
                        && s.recipient_user_id.is_some()
                })
                .cloned()
                .collect())
        }

        async fn find_all_group_shares_for_folders(
            &self,
            folder_ids: &[FolderId],
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            let shares = self.shares.lock().await;
            Ok(shares
                .iter()
                .filter(|s| {
                    s.file_id.is_none()
                        && s.folder_id
                            .map(|fid| folder_ids.contains(&fid))
                            .unwrap_or(false)
                        && s.recipient_group_id.is_some()
                })
                .cloned()
                .collect())
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
        ops.add_file(file).await;

        // Owner should have Admin permission without any share record
        assert!(resolver
            .check_file_permission(owner_id, Uuid::nil(), file_id, SharePermissions::Admin)
            .await
            .unwrap());
        assert!(resolver
            .check_file_permission(owner_id, Uuid::nil(), file_id, SharePermissions::Edit)
            .await
            .unwrap());
        assert!(resolver
            .check_file_permission(owner_id, Uuid::nil(), file_id, SharePermissions::View)
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
        ops.add_file(file).await;

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
        ops.add_share(share).await;

        // User should have View permission
        assert!(resolver
            .check_file_permission(user_id, Uuid::nil(), file_id, SharePermissions::View)
            .await
            .unwrap());
        // But not Edit permission
        assert!(!resolver
            .check_file_permission(user_id, Uuid::nil(), file_id, SharePermissions::Edit)
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
        ops.add_folder(root_folder).await;

        let parent_folder = Folder::new_child(
            "parent".to_string(),
            "/parent".to_string(),
            root_id,
            owner_id,
            Uuid::new_v4(),
        );
        let parent_id = parent_folder.id;
        ops.add_folder(parent_folder).await;

        let child_folder = Folder::new_child(
            "child".to_string(),
            "/parent/child".to_string(),
            parent_id,
            owner_id,
            Uuid::new_v4(),
        );
        let child_id = child_folder.id;
        ops.add_folder(child_folder).await;

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
        ops.add_file(file).await;

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
        ops.add_share(share).await;

        // User should have Edit permission on file through inheritance
        assert!(resolver
            .check_file_permission(user_id, Uuid::nil(), file_id, SharePermissions::View)
            .await
            .unwrap());
        assert!(resolver
            .check_file_permission(user_id, Uuid::nil(), file_id, SharePermissions::Edit)
            .await
            .unwrap());
        assert!(!resolver
            .check_file_permission(user_id, Uuid::nil(), file_id, SharePermissions::Admin)
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
        ops.add_file(file).await;

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
        ops.add_share(share).await;

        // User should not have permission
        assert!(!resolver
            .check_file_permission(user_id, Uuid::nil(), file_id, SharePermissions::View)
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
        ops.add_file(file).await;

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
        ops.add_share(share).await;

        // First call should populate cache
        assert!(resolver
            .check_file_permission(user_id, Uuid::nil(), file_id, SharePermissions::View)
            .await
            .unwrap());

        // Second call should use cache (we can verify this by checking cache size)
        assert!(resolver
            .check_file_permission(user_id, Uuid::nil(), file_id, SharePermissions::Edit)
            .await
            .unwrap());
        assert_eq!(resolver.cache.read().await.len(), 1);

        // Clear cache
        resolver.clear_cache().await;
        assert_eq!(resolver.cache.read().await.len(), 0);
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
        ops.add_file(file).await;

        // User has no share, should not have permission
        assert!(!resolver
            .check_file_permission(user_id, Uuid::nil(), file_id, SharePermissions::View)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_folder_owner_has_admin() {
        let (resolver, ops) = setup();

        let owner_id = Uuid::new_v4();

        let folder = Folder::new_root(owner_id, Uuid::new_v4());
        let folder_id = folder.id;
        ops.add_folder(folder).await;

        // Owner should have Admin permission
        assert!(resolver
            .check_folder_permission(owner_id, Uuid::nil(), folder_id, SharePermissions::Admin)
            .await
            .unwrap());
        assert!(resolver
            .check_folder_permission(owner_id, Uuid::nil(), folder_id, SharePermissions::Edit)
            .await
            .unwrap());
        assert!(resolver
            .check_folder_permission(owner_id, Uuid::nil(), folder_id, SharePermissions::View)
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
        ops.add_folder(root_folder).await;

        let parent_folder = Folder::new_child(
            "parent".to_string(),
            "/parent".to_string(),
            root_id,
            owner_id,
            Uuid::new_v4(),
        );
        let parent_id = parent_folder.id;
        ops.add_folder(parent_folder).await;

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
        ops.add_file(file).await;

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
        ops.add_share(file_share).await;

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
        ops.add_share(folder_share).await;

        // The inherited Admin grant is stronger than the direct View grant.
        assert!(resolver
            .check_file_permission(user_id, Uuid::nil(), file_id, SharePermissions::Admin)
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
        ops.add_user_to_group(user_id, group_id).await;

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
        ops.add_file(file).await;

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
        ops.add_share(share).await;

        // User should have Edit permission through group membership
        assert!(resolver
            .check_file_permission(user_id, Uuid::nil(), file_id, SharePermissions::View)
            .await
            .unwrap());
        assert!(resolver
            .check_file_permission(user_id, Uuid::nil(), file_id, SharePermissions::Edit)
            .await
            .unwrap());
        assert!(!resolver
            .check_file_permission(user_id, Uuid::nil(), file_id, SharePermissions::Admin)
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
        ops.add_file(file).await;

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
        ops.add_share(share).await;

        // User should NOT have permission (not in the group)
        assert!(!resolver
            .check_file_permission(user_id, Uuid::nil(), file_id, SharePermissions::View)
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
        ops.add_user_to_group(user_id, group_id).await;

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
        ops.add_file(file).await;

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
        let group_share_id = group_share.id;
        ops.add_share(group_share).await;

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
        ops.add_share(direct_share).await;

        // The effective permission is the highest active grant, regardless of
        // whether it came from a direct or group share.
        assert!(resolver
            .check_file_permission(user_id, Uuid::nil(), file_id, SharePermissions::Admin)
            .await
            .unwrap());
        let result = resolver
            .resolve_permission_with_source(user_id, Uuid::nil(), Resource::File(file_id))
            .await
            .unwrap();
        assert_eq!(result.permission, Some(SharePermissions::Admin));
        assert_eq!(result.source, PermissionSource::GroupShare);
        assert_eq!(result.share_id, Some(group_share_id));
    }

    #[tokio::test]
    async fn test_highest_permission_wins_multiple_groups() {
        let (resolver, ops) = setup();

        let owner_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let group1_id = Uuid::new_v4();
        let group2_id = Uuid::new_v4();

        // Add user to both groups
        ops.add_user_to_group(user_id, group1_id).await;
        ops.add_user_to_group(user_id, group2_id).await;

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
        ops.add_file(file).await;

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
        ops.add_share(share1).await;

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
        ops.add_share(share2).await;

        // User should have Admin permission (highest from all group shares)
        assert!(resolver
            .check_file_permission(user_id, Uuid::nil(), file_id, SharePermissions::View)
            .await
            .unwrap());
        assert!(resolver
            .check_file_permission(user_id, Uuid::nil(), file_id, SharePermissions::Edit)
            .await
            .unwrap());
        assert!(resolver
            .check_file_permission(user_id, Uuid::nil(), file_id, SharePermissions::Admin)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_cached_source_preserves_group_share() {
        let (resolver, ops) = setup();

        let owner_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let group_id = Uuid::new_v4();
        ops.add_user_to_group(user_id, group_id).await;

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
        ops.add_file(file).await;

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
        let share_id = share.id;
        ops.add_share(share).await;

        assert!(resolver
            .check_file_permission(user_id, Uuid::nil(), file_id, SharePermissions::Edit)
            .await
            .unwrap());

        let result = resolver
            .resolve_permission_with_source(user_id, Uuid::nil(), Resource::File(file_id))
            .await
            .unwrap();

        assert_eq!(result.permission, Some(SharePermissions::Edit));
        assert_eq!(result.source, PermissionSource::GroupShare);
        assert_eq!(result.share_id, Some(share_id));
    }

    #[tokio::test]
    async fn stale_in_flight_check_cannot_repopulate_cache_after_invalidation() {
        let (resolver, _) = setup();
        let resolver = Arc::new(resolver);
        let old_epoch = resolver.cache_epoch.load(Ordering::SeqCst);
        let key = CacheKey::File(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let old_result = PermissionResult {
            permission: Some(SharePermissions::Admin),
            source: PermissionSource::DirectShare,
            share_id: Some(Uuid::new_v4()),
        };
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (finish_tx, finish_rx) = tokio::sync::oneshot::channel();
        let stale_resolver = Arc::clone(&resolver);
        let stale_check = tokio::spawn(async move {
            PERMISSION_CACHE_EPOCH
                .scope(old_epoch, async move {
                    let _ = started_tx.send(());
                    let _ = finish_rx.await;
                    stale_resolver.cache_result(key, old_result).await;
                })
                .await;
        });

        started_rx.await.unwrap();
        resolver.clear_cache().await;
        finish_tx.send(()).unwrap();
        stale_check.await.unwrap();

        assert!(resolver.cache.read().await.is_empty());
    }

    #[tokio::test]
    async fn test_group_share_folder_inheritance() {
        let (resolver, ops) = setup();

        let owner_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let group_id = Uuid::new_v4();

        // Add user to group
        ops.add_user_to_group(user_id, group_id).await;

        // Create folder hierarchy: root -> parent -> child
        let root_folder = Folder::new_root(owner_id, Uuid::new_v4());
        let root_id = root_folder.id;
        ops.add_folder(root_folder).await;

        let parent_folder = Folder::new_child(
            "parent".to_string(),
            "/parent".to_string(),
            root_id,
            owner_id,
            Uuid::new_v4(),
        );
        let parent_id = parent_folder.id;
        ops.add_folder(parent_folder).await;

        let child_folder = Folder::new_child(
            "child".to_string(),
            "/parent/child".to_string(),
            parent_id,
            owner_id,
            Uuid::new_v4(),
        );
        let child_id = child_folder.id;
        ops.add_folder(child_folder).await;

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
        ops.add_file(file).await;

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
        ops.add_share(share).await;

        // User should have Edit permission on file through group share inheritance
        assert!(resolver
            .check_file_permission(user_id, Uuid::nil(), file_id, SharePermissions::View)
            .await
            .unwrap());
        assert!(resolver
            .check_file_permission(user_id, Uuid::nil(), file_id, SharePermissions::Edit)
            .await
            .unwrap());
        assert!(!resolver
            .check_file_permission(user_id, Uuid::nil(), file_id, SharePermissions::Admin)
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
        ops.add_user_to_group(user_id, group_id).await;

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
        ops.add_file(file).await;

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
        ops.add_share(share).await;

        // User should not have permission (share is revoked)
        assert!(!resolver
            .check_file_permission(user_id, Uuid::nil(), file_id, SharePermissions::View)
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
        ops.add_user_to_group(user_id, group_id).await;

        // Create folder hierarchy: root -> parent
        let root_folder = Folder::new_root(owner_id, Uuid::new_v4());
        let root_id = root_folder.id;
        ops.add_folder(root_folder).await;

        let parent_folder = Folder::new_child(
            "parent".to_string(),
            "/parent".to_string(),
            root_id,
            owner_id,
            Uuid::new_v4(),
        );
        let parent_id = parent_folder.id;
        ops.add_folder(parent_folder).await;

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
        ops.add_file(file).await;

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
        ops.add_share(user_share).await;

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
        ops.add_share(group_share).await;

        // User should have Admin permission (highest from user + group shares in ancestry)
        assert!(resolver
            .check_file_permission(user_id, Uuid::nil(), file_id, SharePermissions::Admin)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_folder_ancestry_uses_highest_user_share() {
        let (resolver, ops) = setup();

        let owner_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        let root_folder = Folder::new_root(owner_id, Uuid::new_v4());
        let root_id = root_folder.id;
        ops.add_folder(root_folder).await;

        let parent_folder = Folder::new_child(
            "parent".to_string(),
            "/parent".to_string(),
            root_id,
            owner_id,
            Uuid::new_v4(),
        );
        let parent_id = parent_folder.id;
        ops.add_folder(parent_folder).await;

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
        ops.add_file(file).await;

        let view_share = Share {
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
        ops.add_share(view_share).await;

        let admin_share = Share {
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
        let admin_share_id = admin_share.id;
        ops.add_share(admin_share).await;

        let result = resolver
            .resolve_permission_with_source(user_id, Uuid::nil(), Resource::File(file_id))
            .await
            .unwrap();

        assert_eq!(result.permission, Some(SharePermissions::Admin));
        assert_eq!(result.source, PermissionSource::Inherited);
        assert_eq!(result.share_id, Some(admin_share_id));
    }

    struct ReadPrincipalOps {
        file_shares: Vec<Share>,
        folder_shares: Vec<Share>,
        folder: Option<Folder>,
    }

    #[async_trait::async_trait]

    impl PermissionResolverOps for ReadPrincipalOps {
        async fn user_belongs_to_tenant(
            &self,
            _user_id: UserId,
            _tenant_id: Uuid,
        ) -> anyhow::Result<bool> {
            Ok(true)
        }

        async fn find_user_share(
            &self,
            _file_id: Option<FileId>,
            _folder_id: Option<FolderId>,
            _recipient_user_id: UserId,
            _tenant_id: Uuid,
        ) -> Result<Option<Share>> {
            Ok(None)
        }
        async fn find_group_shares(
            &self,
            _file_id: Option<FileId>,
            _folder_id: Option<FolderId>,
            _group_ids: &[Uuid],
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            Ok(Vec::new())
        }
        async fn find_user_shares_for_folders(
            &self,
            _folder_ids: &[FolderId],
            _recipient_user_id: UserId,
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            Ok(Vec::new())
        }
        async fn find_group_shares_for_folders(
            &self,
            _folder_ids: &[FolderId],
            _group_ids: &[Uuid],
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            Ok(Vec::new())
        }
        async fn find_all_user_shares_for_file(
            &self,
            _file_id: FileId,
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            Ok(self
                .file_shares
                .iter()
                .filter(|s| s.recipient_user_id.is_some())
                .cloned()
                .collect())
        }
        async fn find_all_group_shares_for_file(
            &self,
            _file_id: FileId,
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            Ok(self
                .file_shares
                .iter()
                .filter(|s| s.recipient_group_id.is_some())
                .cloned()
                .collect())
        }
        async fn find_all_user_shares_for_folders(
            &self,
            _folder_ids: &[FolderId],
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            Ok(self
                .folder_shares
                .iter()
                .filter(|s| s.recipient_user_id.is_some())
                .cloned()
                .collect())
        }
        async fn find_all_group_shares_for_folders(
            &self,
            _folder_ids: &[FolderId],
            _tenant_id: Uuid,
        ) -> Result<Vec<Share>> {
            Ok(self
                .folder_shares
                .iter()
                .filter(|s| s.recipient_group_id.is_some())
                .cloned()
                .collect())
        }
        async fn find_file_by_id(&self, _id: FileId, _tenant_id: Uuid) -> Result<Option<File>> {
            Ok(None)
        }
        async fn find_folder_by_id(
            &self,
            _id: FolderId,
            _tenant_id: Uuid,
        ) -> Result<Option<Folder>> {
            Ok(self.folder.clone())
        }
        async fn get_user_group_ids(
            &self,
            _user_id: UserId,
            _tenant_id: Uuid,
        ) -> Result<Vec<Uuid>> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn resolve_read_principals_includes_owner_and_shares() {
        let owner = Uuid::new_v4();
        let user = Uuid::new_v4();
        let group = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let file = File::new(
            "note.md".to_string(),
            "/Workspace/Notes/X/note.md".to_string(),
            "hash".to_string(),
            100,
            "text/markdown".to_string(),
            None,
            owner,
            tenant_id,
        );

        let ops = ReadPrincipalOps {
            file_shares: vec![Share {
                id: Uuid::new_v4(),
                file_id: Some(file_id),
                folder_id: None,
                share_token: None,
                permissions: SharePermissions::View,
                password_hash: None,
                expires_at: None,
                upload_only: false,
                access_count: 0,
                recipient_user_id: Some(user),
                recipient_group_id: None,
                created_by: owner,
                created_at: Utc::now(),
                revoked_at: None,
                tenant_id,
            }],
            folder_shares: vec![Share {
                id: Uuid::new_v4(),
                file_id: None,
                folder_id: Some(Uuid::new_v4()),
                share_token: None,
                permissions: SharePermissions::View,
                password_hash: None,
                expires_at: None,
                upload_only: false,
                access_count: 0,
                recipient_user_id: None,
                recipient_group_id: Some(group),
                created_by: owner,
                created_at: Utc::now(),
                revoked_at: None,
                tenant_id,
            }],
            folder: None,
        };

        let resolver = PermissionResolver::new(Arc::new(ops));
        let mut principals = resolver
            .resolve_read_principals(&file, tenant_id)
            .await
            .unwrap();
        principals.sort();

        assert_eq!(
            principals,
            vec![format!("owner:{owner}"), format!("user:{user}"),]
        );
    }

    #[tokio::test]
    async fn resolve_read_principals_includes_inherited_folder_shares() {
        let owner = Uuid::new_v4();
        let user = Uuid::new_v4();
        let group = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let folder_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let file = File::new(
            "note.md".to_string(),
            "/Workspace/Notes/X/note.md".to_string(),
            "hash".to_string(),
            100,
            "text/markdown".to_string(),
            Some(folder_id),
            owner,
            tenant_id,
        );

        let folder = Folder::new_root_with_name("X".to_string(), owner, tenant_id);

        let ops = ReadPrincipalOps {
            file_shares: vec![Share {
                id: Uuid::new_v4(),
                file_id: Some(file_id),
                folder_id: None,
                share_token: None,
                permissions: SharePermissions::View,
                password_hash: None,
                expires_at: None,
                upload_only: false,
                access_count: 0,
                recipient_user_id: Some(user),
                recipient_group_id: None,
                created_by: owner,
                created_at: Utc::now(),
                revoked_at: None,
                tenant_id,
            }],
            folder_shares: vec![Share {
                id: Uuid::new_v4(),
                file_id: None,
                folder_id: Some(folder_id),
                share_token: None,
                permissions: SharePermissions::View,
                password_hash: None,
                expires_at: None,
                upload_only: false,
                access_count: 0,
                recipient_user_id: None,
                recipient_group_id: Some(group),
                created_by: owner,
                created_at: Utc::now(),
                revoked_at: None,
                tenant_id,
            }],
            folder: Some(folder),
        };

        let resolver = PermissionResolver::new(Arc::new(ops));
        let mut principals = resolver
            .resolve_read_principals(&file, tenant_id)
            .await
            .unwrap();
        principals.sort();

        let mut expected = vec![
            format!("owner:{owner}"),
            format!("group:{group}"),
            format!("user:{user}"),
        ];
        expected.sort();

        assert_eq!(principals, expected);
    }
}
