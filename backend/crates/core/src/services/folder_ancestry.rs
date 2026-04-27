//! Folder ancestry builder for managing ancestor chain computations.
//!
//! This module provides:
//! - `FolderAncestryBuilder`: Builds and caches ancestor chains for folders
//! - Cycle detection for folder moves
//! - Batch updates for descendant folders during moves

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::domain::{Folder, FolderId};

/// Trait for folder repository operations needed by the ancestry builder.
///
/// This trait abstracts the storage layer to allow for testing without
/// database dependencies.
#[allow(async_fn_in_trait)]
pub trait AncestryFolderRepository: Send + Sync {
    /// Find a folder by ID.
    async fn find_folder_by_id(&self, id: FolderId) -> Result<Option<Folder>>;

    /// Find all descendant folders of a given folder.
    async fn find_descendant_folders(&self, folder_id: FolderId) -> Result<Vec<Folder>>;

    /// Update a folder in the metadata store.
    async fn update_folder(&self, folder: &Folder) -> Result<()>;
}

/// Cache entry for ancestor chains.
#[derive(Debug, Clone)]
struct CacheEntry {
    ancestor_ids: Vec<FolderId>,
    /// Timestamp for potential TTL eviction
    _timestamp: std::time::Instant,
}

/// FolderAncestryBuilder builds and caches ancestor chains for folders.
///
/// This builder is used to:
/// - Compute ancestor_ids when creating folders
/// - Validate moves don't create cycles
/// - Recompute ancestor_ids for moved folders and their descendants
///
/// The builder maintains an in-memory LRU-style cache for frequently
/// accessed ancestor chains to reduce database lookups.
pub struct FolderAncestryBuilder<R: AncestryFolderRepository> {
    repo: Arc<R>,
    /// Cache of folder_id -> ancestor_ids
    cache: RwLock<HashMap<FolderId, CacheEntry>>,
    /// Maximum cache size before eviction
    max_cache_size: usize,
}

impl<R: AncestryFolderRepository> FolderAncestryBuilder<R> {
    /// Create a new FolderAncestryBuilder.
    pub fn new(repo: Arc<R>) -> Self {
        Self {
            repo,
            cache: RwLock::new(HashMap::new()),
            max_cache_size: 1000,
        }
    }

    /// Create a new FolderAncestryBuilder with a custom cache size.
    pub fn with_cache_size(repo: Arc<R>, max_cache_size: usize) -> Self {
        Self {
            repo,
            cache: RwLock::new(HashMap::new()),
            max_cache_size,
        }
    }

    /// Build the ancestor chain for a folder.
    ///
    /// The ancestor chain is ordered from root to parent (oldest first).
    /// For example: [root_id, grandparent_id, parent_id]
    ///
    /// Returns an empty vector for root folders.
    pub async fn build_ancestor_chain(&self, folder_id: FolderId) -> Result<Vec<FolderId>> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.get(&folder_id) {
                return Ok(entry.ancestor_ids.clone());
            }
        }

        // Build chain by walking up parent_ids
        let mut ancestor_ids = Vec::new();
        let mut visited = HashSet::new();
        let mut current_id = Some(folder_id);

        while let Some(id) = current_id {
            // Circular reference detection during build
            if !visited.insert(id) {
                return Err(anyhow::anyhow!(
                    "Circular reference detected in folder hierarchy at folder {}",
                    id
                ));
            }

            // Fetch the folder
            let folder = match self.repo.find_folder_by_id(id).await? {
                Some(f) => f,
                None => break, // Folder not found, stop walking
            };

            // Move to parent
            current_id = folder.parent_folder_id;

            // If there's a parent, add it to ancestors (we'll reverse at the end)
            if let Some(parent_id) = folder.parent_folder_id {
                ancestor_ids.push(parent_id);
            }
        }

        // Reverse to get root-first order
        ancestor_ids.reverse();

        // Cache the result
        self.put_in_cache(folder_id, ancestor_ids.clone()).await;

        Ok(ancestor_ids)
    }

    /// Build ancestor chain from a parent folder's ancestors.
    ///
    /// This is the efficient path for creating new folders - we don't need
    /// to query the database, just append parent_id to parent's ancestors.
    pub fn build_ancestor_chain_from_parent(
        &self,
        parent_id: FolderId,
        parent_ancestor_ids: Option<&[FolderId]>,
    ) -> Vec<FolderId> {
        let mut ancestor_ids = parent_ancestor_ids
            .map(|ids| ids.to_vec())
            .unwrap_or_default();
        ancestor_ids.push(parent_id);
        ancestor_ids
    }

    /// Validate that moving a folder to a new parent won't create a cycle.
    ///
    /// Returns true if the move is valid (no cycle), false if it would create a cycle.
    ///
    /// A cycle would occur if the new_parent_id is the folder itself or any of its descendants.
    pub async fn validate_no_cycles(
        &self,
        folder_id: FolderId,
        new_parent_id: Option<FolderId>,
    ) -> Result<bool> {
        // If moving to root, always valid
        let new_parent_id = match new_parent_id {
            Some(id) => id,
            None => return Ok(true),
        };

        // Cannot move a folder into itself
        if folder_id == new_parent_id {
            return Ok(false);
        }

        // Get all descendants of the folder
        let descendants = self.repo.find_descendant_folders(folder_id).await?;

        // Check if new_parent is any of the descendants
        for descendant in descendants {
            if descendant.id == new_parent_id {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Validate that moving a folder to a new parent won't create a cycle (using ancestor_ids).
    ///
    /// This is a more efficient version that uses ancestor_ids when available.
    /// Returns true if the move is valid (no cycle), false if it would create a cycle.
    pub fn validate_no_cycles_with_ancestors(
        &self,
        folder_id: FolderId,
        new_parent_id: Option<FolderId>,
        new_parent_ancestor_ids: Option<&[FolderId]>,
    ) -> bool {
        // If moving to root, always valid
        let new_parent_id = match new_parent_id {
            Some(id) => id,
            None => return true,
        };

        // Cannot move a folder into itself
        if folder_id == new_parent_id {
            return false;
        }

        // Check if folder_id is in the new parent's ancestors
        // If so, moving would create a cycle
        if let Some(ancestors) = new_parent_ancestor_ids {
            if ancestors.contains(&folder_id) {
                return false;
            }
        }

        true
    }

    /// Recompute ancestor_ids for a moved folder and all its descendants.
    ///
    /// This is an expensive operation that updates the ancestor_ids for the
    /// entire subtree after a folder move. Consider running this in a
    /// background job for large subtrees.
    ///
    /// Returns the number of folders updated.
    pub async fn recompute_ancestors_for_move(
        &self,
        folder_id: FolderId,
        new_parent_id: Option<FolderId>,
    ) -> Result<usize> {
        // Get the folder being moved
        let mut folder = self
            .repo
            .find_folder_by_id(folder_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Folder not found: {}", folder_id))?;

        // Build new ancestor chain for the moved folder
        let new_ancestor_ids = if let Some(parent_id) = new_parent_id {
            // Get parent's ancestors
            let parent_ancestors = self.build_ancestor_chain(parent_id).await?;
            let mut new_ancestors = parent_ancestors;
            new_ancestors.push(parent_id);
            new_ancestors
        } else {
            Vec::new() // Root folder has no ancestors
        };

        // Update the folder's ancestor_ids
        folder.ancestor_ids = Some(new_ancestor_ids.clone());
        folder.updated_at = chrono::Utc::now();
        self.repo.update_folder(&folder).await?;

        // Update cache
        self.put_in_cache(folder_id, new_ancestor_ids.clone()).await;

        // Get all descendants and update their ancestor_ids
        let descendants = self.repo.find_descendant_folders(folder_id).await?;
        let mut updated_count = 1; // Count the moved folder itself

        for mut descendant in descendants {
            // Skip the folder itself (already updated above)
            if descendant.id == folder_id {
                continue;
            }

            // Build new ancestor chain for descendant
            // The descendant's ancestors = moved folder's ancestors + moved folder + path from moved folder to descendant
            let old_ancestor_ids = descendant.ancestor_ids.clone().unwrap_or_default();

            // Find where the old folder_id was in the ancestor chain
            // Replace everything up to and including folder_id with the new ancestors
            let mut new_descendant_ancestors = new_ancestor_ids.clone();
            new_descendant_ancestors.push(folder_id);

            // Find the position after folder_id in old ancestors
            if let Some(pos) = old_ancestor_ids.iter().position(|&id| id == folder_id) {
                // Append everything after folder_id
                new_descendant_ancestors.extend(&old_ancestor_ids[pos + 1..]);
            }

            descendant.ancestor_ids = Some(new_descendant_ancestors.clone());
            descendant.updated_at = chrono::Utc::now();
            self.repo.update_folder(&descendant).await?;

            // Update cache
            self.put_in_cache(descendant.id, new_descendant_ancestors).await;

            updated_count += 1;
        }

        Ok(updated_count)
    }

    /// Get ancestor_ids from cache if available.
    pub async fn get_cached_ancestors(&self, folder_id: FolderId) -> Option<Vec<FolderId>> {
        let cache = self.cache.read().await;
        cache.get(&folder_id).map(|e| e.ancestor_ids.clone())
    }

    /// Clear the cache.
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    /// Put an entry in the cache, evicting oldest entries if needed.
    async fn put_in_cache(&self, folder_id: FolderId, ancestor_ids: Vec<FolderId>) {
        let mut cache = self.cache.write().await;

        // Simple eviction: if at capacity, clear half the cache
        // (In production, use a proper LRU cache)
        if cache.len() >= self.max_cache_size {
            let keys_to_remove: Vec<_> = cache
                .keys()
                .take(self.max_cache_size / 2)
                .copied()
                .collect();
            for key in keys_to_remove {
                cache.remove(&key);
            }
        }

        cache.insert(
            folder_id,
            CacheEntry {
                ancestor_ids,
                _timestamp: std::time::Instant::now(),
            },
        );
    }

    /// Check if a folder is an ancestor of another folder.
    ///
    /// Returns true if `potential_ancestor` is in the ancestor chain of `folder_id`.
    pub async fn is_ancestor_of(
        &self,
        potential_ancestor: FolderId,
        folder_id: FolderId,
    ) -> Result<bool> {
        let ancestors = self.build_ancestor_chain(folder_id).await?;
        Ok(ancestors.contains(&potential_ancestor))
    }

    /// Check if a folder is an ancestor of another folder using cached data.
    ///
    /// This version uses ancestor_ids from the folder document if available,
    /// avoiding a database query.
    pub async fn is_ancestor_of_fast(
        &self,
        potential_ancestor: FolderId,
        folder_id: FolderId,
    ) -> Result<bool> {
        // Try cache first
        if let Some(ancestors) = self.get_cached_ancestors(folder_id).await {
            return Ok(ancestors.contains(&potential_ancestor));
        }

        // Try to get from folder document directly
        if let Some(folder) = self.repo.find_folder_by_id(folder_id).await? {
            if let Some(ref ancestors) = folder.ancestor_ids {
                // Update cache for next time
                self.put_in_cache(folder_id, ancestors.clone()).await;
                return Ok(ancestors.contains(&potential_ancestor));
            }
        }

        // Fall back to building the chain
        let ancestors = self.build_ancestor_chain(folder_id).await?;
        Ok(ancestors.contains(&potential_ancestor))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tokio::sync::Mutex;
    use uuid::Uuid;

    // Mock repository for testing
    struct MockFolderRepository {
        folders: Mutex<HashMap<FolderId, Folder>>,
    }

    impl MockFolderRepository {
        fn new() -> Self {
            Self {
                folders: Mutex::new(HashMap::new()),
            }
        }

        async fn add_folder(&self, folder: Folder) {
            self.folders.lock().await.insert(folder.id, folder);
        }
    }

    #[allow(async_fn_in_trait)]
    impl AncestryFolderRepository for MockFolderRepository {
        async fn find_folder_by_id(&self, id: FolderId) -> Result<Option<Folder>> {
            Ok(self.folders.lock().await.get(&id).cloned())
        }

        async fn find_descendant_folders(&self, folder_id: FolderId) -> Result<Vec<Folder>> {
            let folders = self.folders.lock().await;
            let mut descendants = Vec::new();
            let mut to_process = vec![folder_id];

            while let Some(current_id) = to_process.pop() {
                for folder in folders.values() {
                    if folder.parent_folder_id == Some(current_id) {
                        descendants.push(folder.clone());
                        to_process.push(folder.id);
                    }
                }
            }

            Ok(descendants)
        }

        async fn update_folder(&self, folder: &Folder) -> Result<()> {
            self.folders
                .lock()
                .await
                .insert(folder.id, folder.clone());
            Ok(())
        }
    }

    fn create_test_folder(
        id: FolderId,
        name: &str,
        parent_id: Option<FolderId>,
        ancestor_ids: Option<Vec<FolderId>>,
    ) -> Folder {
        let path = if parent_id.is_some() {
            format!("/parent/{}", name)
        } else {
            format!("/{}", name)
        };

        Folder {
            id,
            name: name.to_string(),
            path,
            parent_folder_id: parent_id,
            owner_id: Uuid::new_v4(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            starred_at: None,
            deleted_at: None,
            tenant_id: Uuid::new_v4(),
            ancestor_ids,
        }
    }

    #[tokio::test]
    async fn test_build_ancestor_chain_root() {
        let repo = Arc::new(MockFolderRepository::new());
        let builder = FolderAncestryBuilder::new(repo.clone());

        // Create root folder
        let root_id = Uuid::new_v4();
        let root = create_test_folder(root_id, "Root", None, Some(vec![]));
        repo.add_folder(root).await;

        // Build chain for root - should be empty
        let chain = builder.build_ancestor_chain(root_id).await.unwrap();
        assert!(chain.is_empty());
    }

    #[tokio::test]
    async fn test_build_ancestor_chain_nested() {
        let repo = Arc::new(MockFolderRepository::new());
        let builder = FolderAncestryBuilder::new(repo.clone());

        // Create hierarchy: root -> parent -> child
        let root_id = Uuid::new_v4();
        let parent_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();

        let root = create_test_folder(root_id, "Root", None, Some(vec![]));
        let parent = create_test_folder(parent_id, "Parent", Some(root_id), Some(vec![root_id]));
        let child = create_test_folder(
            child_id,
            "Child",
            Some(parent_id),
            Some(vec![root_id, parent_id]),
        );

        repo.add_folder(root).await;
        repo.add_folder(parent).await;
        repo.add_folder(child).await;

        // Build chain for child - should be [root, parent]
        let chain = builder.build_ancestor_chain(child_id).await.unwrap();
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0], root_id);
        assert_eq!(chain[1], parent_id);
    }

    #[tokio::test]
    async fn test_validate_no_cycles_valid() {
        let repo = Arc::new(MockFolderRepository::new());
        let builder = FolderAncestryBuilder::new(repo.clone());

        // Create hierarchy: root -> parent -> child
        let root_id = Uuid::new_v4();
        let parent_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();

        let root = create_test_folder(root_id, "Root", None, Some(vec![]));
        let parent = create_test_folder(parent_id, "Parent", Some(root_id), Some(vec![root_id]));
        let child = create_test_folder(
            child_id,
            "Child",
            Some(parent_id),
            Some(vec![root_id, parent_id]),
        );

        repo.add_folder(root).await;
        repo.add_folder(parent).await;
        repo.add_folder(child).await;

        // Moving child to root should be valid
        let valid = builder
            .validate_no_cycles(child_id, Some(root_id))
            .await
            .unwrap();
        assert!(valid);

        // Moving parent to root should be valid
        let valid = builder
            .validate_no_cycles(parent_id, Some(root_id))
            .await
            .unwrap();
        assert!(valid);
    }

    #[tokio::test]
    async fn test_validate_no_cycles_invalid() {
        let repo = Arc::new(MockFolderRepository::new());
        let builder = FolderAncestryBuilder::new(repo.clone());

        // Create hierarchy: root -> parent -> child
        let root_id = Uuid::new_v4();
        let parent_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();

        let root = create_test_folder(root_id, "Root", None, Some(vec![]));
        let parent = create_test_folder(parent_id, "Parent", Some(root_id), Some(vec![root_id]));
        let child = create_test_folder(
            child_id,
            "Child",
            Some(parent_id),
            Some(vec![root_id, parent_id]),
        );

        repo.add_folder(root).await;
        repo.add_folder(parent).await;
        repo.add_folder(child).await;

        // Moving root to child would create a cycle
        let valid = builder
            .validate_no_cycles(root_id, Some(child_id))
            .await
            .unwrap();
        assert!(!valid);

        // Moving parent to child would create a cycle
        let valid = builder
            .validate_no_cycles(parent_id, Some(child_id))
            .await
            .unwrap();
        assert!(!valid);

        // Moving folder to itself is invalid
        let valid = builder
            .validate_no_cycles(parent_id, Some(parent_id))
            .await
            .unwrap();
        assert!(!valid);
    }

    #[tokio::test]
    async fn test_validate_no_cycles_to_root() {
        let repo = Arc::new(MockFolderRepository::new());
        let builder = FolderAncestryBuilder::new(repo.clone());

        let folder_id = Uuid::new_v4();
        let folder = create_test_folder(folder_id, "Folder", None, Some(vec![]));
        repo.add_folder(folder).await;

        // Moving to root (None) should always be valid
        let valid = builder.validate_no_cycles(folder_id, None).await.unwrap();
        assert!(valid);
    }

    #[tokio::test]
    async fn test_recompute_ancestors_for_move() {
        let repo = Arc::new(MockFolderRepository::new());
        let builder = FolderAncestryBuilder::new(repo.clone());

        // Create two separate hierarchies
        // Hierarchy A: root_a -> parent_a -> child_a
        // Hierarchy B: root_b
        let root_a_id = Uuid::new_v4();
        let parent_a_id = Uuid::new_v4();
        let child_a_id = Uuid::new_v4();
        let root_b_id = Uuid::new_v4();

        let root_a = create_test_folder(root_a_id, "RootA", None, Some(vec![]));
        let parent_a = create_test_folder(
            parent_a_id,
            "ParentA",
            Some(root_a_id),
            Some(vec![root_a_id]),
        );
        let child_a = create_test_folder(
            child_a_id,
            "ChildA",
            Some(parent_a_id),
            Some(vec![root_a_id, parent_a_id]),
        );
        let root_b = create_test_folder(root_b_id, "RootB", None, Some(vec![]));

        repo.add_folder(root_a).await;
        repo.add_folder(parent_a).await;
        repo.add_folder(child_a).await;
        repo.add_folder(root_b).await;

        // Move parent_a (and its child) to root_b
        let updated = builder
            .recompute_ancestors_for_move(parent_a_id, Some(root_b_id))
            .await
            .unwrap();
        assert_eq!(updated, 2); // parent_a and child_a

        // Verify parent_a's new ancestors
        let parent_a = repo.find_folder_by_id(parent_a_id).await.unwrap().unwrap();
        assert_eq!(parent_a.ancestor_ids, Some(vec![root_b_id]));

        // Verify child_a's new ancestors
        let child_a = repo.find_folder_by_id(child_a_id).await.unwrap().unwrap();
        assert_eq!(child_a.ancestor_ids, Some(vec![root_b_id, parent_a_id]));
    }

    #[tokio::test]
    async fn test_build_ancestor_chain_from_parent() {
        let repo = Arc::new(MockFolderRepository::new());
        let builder = FolderAncestryBuilder::new(repo);

        let parent_id = Uuid::new_v4();
        let grandparent_id = Uuid::new_v4();

        // Test with parent's ancestors
        let chain = builder.build_ancestor_chain_from_parent(parent_id, Some(&[grandparent_id]));
        assert_eq!(chain, vec![grandparent_id, parent_id]);

        // Test with no parent ancestors (parent is root)
        let chain = builder.build_ancestor_chain_from_parent(parent_id, Some(&[]));
        assert_eq!(chain, vec![parent_id]);

        // Test with None parent ancestors
        let chain = builder.build_ancestor_chain_from_parent(parent_id, None);
        assert_eq!(chain, vec![parent_id]);
    }

    #[tokio::test]
    async fn test_is_ancestor_of() {
        let repo = Arc::new(MockFolderRepository::new());
        let builder = FolderAncestryBuilder::new(repo.clone());

        // Create hierarchy: root -> parent -> child
        let root_id = Uuid::new_v4();
        let parent_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();

        let root = create_test_folder(root_id, "Root", None, Some(vec![]));
        let parent = create_test_folder(parent_id, "Parent", Some(root_id), Some(vec![root_id]));
        let child = create_test_folder(
            child_id,
            "Child",
            Some(parent_id),
            Some(vec![root_id, parent_id]),
        );

        repo.add_folder(root).await;
        repo.add_folder(parent).await;
        repo.add_folder(child).await;

        // Root is ancestor of parent
        assert!(builder.is_ancestor_of(root_id, parent_id).await.unwrap());

        // Root is ancestor of child
        assert!(builder.is_ancestor_of(root_id, child_id).await.unwrap());

        // Parent is ancestor of child
        assert!(builder.is_ancestor_of(parent_id, child_id).await.unwrap());

        // Child is not ancestor of parent
        assert!(!builder.is_ancestor_of(child_id, parent_id).await.unwrap());

        // Parent is not ancestor of root
        assert!(!builder.is_ancestor_of(parent_id, root_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_caching() {
        let repo = Arc::new(MockFolderRepository::new());
        let builder = FolderAncestryBuilder::new(repo.clone());

        let root_id = Uuid::new_v4();
        let parent_id = Uuid::new_v4();

        let root = create_test_folder(root_id, "Root", None, Some(vec![]));
        let parent = create_test_folder(parent_id, "Parent", Some(root_id), Some(vec![root_id]));

        repo.add_folder(root).await;
        repo.add_folder(parent).await;

        // First call should populate cache
        let chain1 = builder.build_ancestor_chain(parent_id).await.unwrap();
        assert_eq!(chain1, vec![root_id]);

        // Second call should use cache
        let chain2 = builder.build_ancestor_chain(parent_id).await.unwrap();
        assert_eq!(chain2, vec![root_id]);

        // Verify it's in cache
        let cached = builder.get_cached_ancestors(parent_id).await;
        assert_eq!(cached, Some(vec![root_id]));

        // Clear cache
        builder.clear_cache().await;
        let cached = builder.get_cached_ancestors(parent_id).await;
        assert_eq!(cached, None);
    }

    #[tokio::test]
    async fn test_validate_no_cycles_with_ancestors() {
        let repo = Arc::new(MockFolderRepository::new());
        let builder = FolderAncestryBuilder::new(repo);

        let folder_a = Uuid::new_v4();
        let folder_b = Uuid::new_v4();
        let folder_c = Uuid::new_v4();

        // folder_a is ancestor of folder_b
        // folder_b is ancestor of folder_c
        // So: folder_a -> folder_b -> folder_c

        // Moving folder_a to folder_c would create a cycle
        let valid = builder.validate_no_cycles_with_ancestors(
            folder_a,
            Some(folder_c),
            Some(&[folder_a, folder_b]), // folder_c's ancestors include folder_a
        );
        assert!(!valid);

        // Moving folder_c to folder_a is valid
        let valid = builder.validate_no_cycles_with_ancestors(
            folder_c,
            Some(folder_a),
            Some(&[]), // folder_a has no ancestors
        );
        assert!(valid);

        // Moving to root is always valid
        let valid = builder.validate_no_cycles_with_ancestors(folder_b, None, None);
        assert!(valid);
    }
}
