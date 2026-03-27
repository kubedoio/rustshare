//! Runtime cache for hot-path metadata access
//!
//! This cache provides in-memory acceleration for frequently accessed metadata.
//! It is NOT the source of truth - all writes go to durable storage first.

use super::schemas::*;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::RwLock;
use uuid::Uuid;

/// Runtime cache for metadata
///
/// This cache is designed for concurrent access and provides
/// invalidation and update hooks for maintaining consistency.
pub struct RuntimeMetadataCache {
    /// Folder children cache: folder_id -> children
    folder_children: RwLock<HashMap<Uuid, FolderChildrenIndex>>,
    
    /// User roots cache: user_id -> root folder IDs
    user_roots: RwLock<HashMap<Uuid, UserRootsIndex>>,
    
    /// Shared with me cache: user_id -> shares
    shared_with_me: RwLock<HashMap<Uuid, SharedWithMeIndex>>,
    
    /// File metadata cache: file_id -> file doc
    file_cache: RwLock<HashMap<Uuid, FileDocument>>,
    
    /// Folder metadata cache: folder_id -> folder doc
    folder_cache: RwLock<HashMap<Uuid, FolderDocument>>,
    
    /// Share metadata cache: share_id -> share doc
    share_cache: RwLock<HashMap<Uuid, ShareDocument>>,
}

impl RuntimeMetadataCache {
    /// Create a new empty cache
    pub fn new() -> Self {
        Self {
            folder_children: RwLock::new(HashMap::new()),
            user_roots: RwLock::new(HashMap::new()),
            shared_with_me: RwLock::new(HashMap::new()),
            file_cache: RwLock::new(HashMap::new()),
            folder_cache: RwLock::new(HashMap::new()),
            share_cache: RwLock::new(HashMap::new()),
        }
    }
    
    // ========================================================================
    // Folder Children Operations
    // ========================================================================
    
    /// Get cached folder children
    pub fn get_folder_children(&self, folder_id: Uuid) -> Option<FolderChildrenIndex> {
        self.folder_children.read().unwrap().get(&folder_id).cloned()
    }
    
    /// Cache folder children
    pub fn put_folder_children(&self, index: FolderChildrenIndex) {
        self.folder_children
            .write()
            .unwrap()
            .insert(index.folder_id, index);
    }
    
    /// Invalidate folder children cache
    pub fn invalidate_folder_children(&self, folder_id: Uuid) {
        self.folder_children.write().unwrap().remove(&folder_id);
    }
    
    /// Update folder children after file creation
    pub fn on_file_created(&self, file: &FileDocument) {
        if let Some(parent_id) = file.parent_id {
            let mut guard = self.folder_children.write().unwrap();
            
            if let Some(index) = guard.get_mut(&parent_id) {
                index.upsert_child(FolderChildEntry {
                    id: file.id,
                    kind: "file".to_string(),
                    name: file.name.clone(),
                    deleted: false,
                    size: Some(file.size),
                    mime: Some(file.mime_type.clone()),
                    updated_at: file.updated_at,
                });
            }
        }
    }
    
    /// Update folder children after file rename
    pub fn on_file_renamed(&self, file: &FileDocument, old_parent_id: Option<Uuid>) {
        // Remove from old parent if different
        if old_parent_id != file.parent_id {
            if let Some(old_parent) = old_parent_id {
                let mut guard = self.folder_children.write().unwrap();
                if let Some(index) = guard.get_mut(&old_parent) {
                    index.remove_child(file.id);
                }
            }
        }
        
        // Add to new parent
        if let Some(parent_id) = file.parent_id {
            let mut guard = self.folder_children.write().unwrap();
            
            if let Some(index) = guard.get_mut(&parent_id) {
                index.upsert_child(FolderChildEntry {
                    id: file.id,
                    kind: "file".to_string(),
                    name: file.name.clone(),
                    deleted: false,
                    size: Some(file.size),
                    mime: Some(file.mime_type.clone()),
                    updated_at: file.updated_at,
                });
            }
        }
    }
    
    /// Update folder children after file deletion
    pub fn on_file_deleted(&self, file_id: Uuid, parent_id: Option<Uuid>) {
        if let Some(parent) = parent_id {
            let mut guard = self.folder_children.write().unwrap();
            
            if let Some(index) = guard.get_mut(&parent) {
                index.mark_deleted(file_id);
            }
        }
        
        // Also invalidate file cache
        self.file_cache.write().unwrap().remove(&file_id);
    }
    
    // ========================================================================
    // Folder Operations
    // ========================================================================
    
    /// Get cached folder
    pub fn get_folder(&self, folder_id: Uuid) -> Option<FolderDocument> {
        self.folder_cache.read().unwrap().get(&folder_id).cloned()
    }
    
    /// Cache folder
    pub fn put_folder(&self, folder: FolderDocument) {
        self.folder_cache.write().unwrap().insert(folder.id, folder);
    }
    
    /// Invalidate folder cache
    pub fn invalidate_folder(&self, folder_id: Uuid) {
        self.folder_cache.write().unwrap().remove(&folder_id);
        // Also invalidate children index
        self.invalidate_folder_children(folder_id);
    }
    
    /// Update cache after folder creation
    pub fn on_folder_created(&self, folder: &FolderDocument) {
        // Cache the folder
        self.put_folder(folder.clone());
        
        // Update parent's children if not root
        if let Some(parent_id) = folder.parent_id {
            let mut guard = self.folder_children.write().unwrap();
            
            if let Some(index) = guard.get_mut(&parent_id) {
                index.upsert_child(FolderChildEntry {
                    id: folder.id,
                    kind: "folder".to_string(),
                    name: folder.name.clone(),
                    deleted: false,
                    size: None,
                    mime: None,
                    updated_at: folder.updated_at,
                });
            }
        } else {
            // This is a root folder - update user roots
            let mut guard = self.user_roots.write().unwrap();
            
            guard
                .entry(folder.owner_id)
                .or_insert_with(|| UserRootsIndex {
                    schema_version: CURRENT_SCHEMA_VERSION,
                    user_id: folder.owner_id,
                    version: 0,
                    updated_at: folder.created_at,
                    root_folder_ids: Vec::new(),
                })
                .root_folder_ids
                .push(folder.id);
        }
    }
    
    /// Update cache after folder rename/move
    pub fn on_folder_moved(
        &self,
        folder: &FolderDocument,
        old_parent_id: Option<Uuid>,
        old_path: &str,
    ) {
        // Update cached folder
        self.put_folder(folder.clone());
        
        // Remove from old parent's children
        if let Some(old_parent) = old_parent_id {
            let mut guard = self.folder_children.write().unwrap();
            
            if let Some(index) = guard.get_mut(&old_parent) {
                index.remove_child(folder.id);
            }
        }
        
        // Add to new parent's children
        if let Some(new_parent) = folder.parent_id {
            let mut guard = self.folder_children.write().unwrap();
            
            if let Some(index) = guard.get_mut(&new_parent) {
                index.upsert_child(FolderChildEntry {
                    id: folder.id,
                    kind: "folder".to_string(),
                    name: folder.name.clone(),
                    deleted: false,
                    size: None,
                    mime: None,
                    updated_at: folder.updated_at,
                });
            }
        }
        
        // Note: Updating descendant paths requires rebuilding the affected subtrees
        // This is handled by invalidating affected folder caches
        if old_path != folder.path {
            // Invalidate any folder whose path started with old_path
            let guard = self.folder_cache.read().unwrap();
            let to_invalidate: Vec<Uuid> = guard
                .values()
                .filter(|f| f.path.starts_with(old_path) && f.id != folder.id)
                .map(|f| f.id)
                .collect();
            drop(guard);
            
            for id in to_invalidate {
                self.invalidate_folder(id);
            }
        }
    }
    
    /// Update cache after folder deletion
    pub fn on_folder_deleted(&self, folder: &FolderDocument) {
        let folder_id = folder.id;
        
        // Remove from parent's children
        if let Some(parent_id) = folder.parent_id {
            let mut guard = self.folder_children.write().unwrap();
            
            if let Some(index) = guard.get_mut(&parent_id) {
                index.mark_deleted(folder_id);
            }
        }
        
        // Invalidate all caches for this folder
        self.invalidate_folder(folder_id);
        
        // Note: Descendant folders should be handled separately by the caller
    }
    
    // ========================================================================
    // File Operations
    // ========================================================================
    
    /// Get cached file
    pub fn get_file(&self, file_id: Uuid) -> Option<FileDocument> {
        self.file_cache.read().unwrap().get(&file_id).cloned()
    }
    
    /// Cache file
    pub fn put_file(&self, file: FileDocument) {
        self.file_cache.write().unwrap().insert(file.id, file);
    }
    
    /// Invalidate file cache
    pub fn invalidate_file(&self, file_id: Uuid) {
        self.file_cache.write().unwrap().remove(&file_id);
    }
    
    /// Update cache after file update
    pub fn on_file_updated(&self, file: &FileDocument) {
        self.put_file(file.clone());
        
        // Update in parent's children
        if let Some(parent_id) = file.parent_id {
            let mut guard = self.folder_children.write().unwrap();
            
            if let Some(index) = guard.get_mut(&parent_id) {
                index.upsert_child(FolderChildEntry {
                    id: file.id,
                    kind: "file".to_string(),
                    name: file.name.clone(),
                    deleted: false,
                    size: Some(file.size),
                    mime: Some(file.mime_type.clone()),
                    updated_at: file.updated_at,
                });
            }
        }
    }
    
    // ========================================================================
    // Share Operations
    // ========================================================================
    
    /// Get cached share
    pub fn get_share(&self, share_id: Uuid) -> Option<ShareDocument> {
        self.share_cache.read().unwrap().get(&share_id).cloned()
    }
    
    /// Cache share
    pub fn put_share(&self, share: ShareDocument) {
        self.share_cache.write().unwrap().insert(share.id, share);
    }
    
    /// Invalidate share cache
    pub fn invalidate_share(&self, share_id: Uuid) {
        self.share_cache.write().unwrap().remove(&share_id);
    }
    
    /// Update cache after share creation
    pub fn on_share_created(&self, share: &ShareDocument) {
        self.put_share(share.clone());
        
        // If user share, update recipient's shared_with_me
        if let Some(recipient_id) = share.recipient_user_id {
            let mut guard = self.shared_with_me.write().unwrap();
            
            guard
                .entry(recipient_id)
                .and_modify(|index| {
                    // Update existing entry
                    if let Some(entry) = index.shares.iter_mut().find(|e| e.share_id == share.id) {
                        entry.permissions = share.permissions;
                    } else {
                        index.shares.push(ShareEntry {
                            share_id: share.id,
                            resource_type: share.resource_type.clone(),
                            resource_id: share.resource_id,
                            resource_name: String::new(), // Would need to fetch
                            permissions: share.permissions,
                            shared_by: share.created_by,
                            shared_at: share.created_at,
                        });
                    }
                })
                .or_insert_with(|| SharedWithMeIndex {
                    schema_version: CURRENT_SCHEMA_VERSION,
                    user_id: recipient_id,
                    version: 1,
                    updated_at: Utc::now(),
                    shares: vec![ShareEntry {
                        share_id: share.id,
                        resource_type: share.resource_type.clone(),
                        resource_id: share.resource_id,
                        resource_name: String::new(),
                        permissions: share.permissions,
                        shared_by: share.created_by,
                        shared_at: share.created_at,
                    }],
                });
        }
    }
    
    /// Update cache after share revocation
    pub fn on_share_revoked(&self, share: &ShareDocument) {
        self.put_share(share.clone());
        
        // If user share, update recipient's shared_with_me
        if let Some(recipient_id) = share.recipient_user_id {
            let mut guard = self.shared_with_me.write().unwrap();
            
            if let Some(index) = guard.get_mut(&recipient_id) {
                index.shares.retain(|e| e.share_id != share.id);
            }
        }
    }
    
    // ========================================================================
    // Bulk Operations
    // ========================================================================
    
    /// Clear all caches
    pub fn clear_all(&self) {
        self.folder_children.write().unwrap().clear();
        self.user_roots.write().unwrap().clear();
        self.shared_with_me.write().unwrap().clear();
        self.file_cache.write().unwrap().clear();
        self.folder_cache.write().unwrap().clear();
        self.share_cache.write().unwrap().clear();
    }
    
    /// Invalidate all entries for a user
    pub fn invalidate_user(&self, user_id: Uuid) {
        self.user_roots.write().unwrap().remove(&user_id);
        self.shared_with_me.write().unwrap().remove(&user_id);
        
        // Remove all folders owned by user
        let folder_ids: Vec<Uuid> = self
            .folder_cache
            .read()
            .unwrap()
            .values()
            .filter(|f| f.owner_id == user_id)
            .map(|f| f.id)
            .collect();
        
        for id in folder_ids {
            self.invalidate_folder(id);
        }
        
        // Remove all files owned by user
        let file_ids: Vec<Uuid> = self
            .file_cache
            .read()
            .unwrap()
            .values()
            .filter(|f| f.owner_id == user_id)
            .map(|f| f.id)
            .collect();
        
        for id in file_ids {
            self.invalidate_file(id);
        }
    }
    
    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            folder_children_count: self.folder_children.read().unwrap().len(),
            user_roots_count: self.user_roots.read().unwrap().len(),
            shared_with_me_count: self.shared_with_me.read().unwrap().len(),
            file_cache_count: self.file_cache.read().unwrap().len(),
            folder_cache_count: self.folder_cache.read().unwrap().len(),
            share_cache_count: self.share_cache.read().unwrap().len(),
        }
    }
}

impl Default for RuntimeMetadataCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub folder_children_count: usize,
    pub user_roots_count: usize,
    pub shared_with_me_count: usize,
    pub file_cache_count: usize,
    pub folder_cache_count: usize,
    pub share_cache_count: usize,
}

impl CacheStats {
    /// Total number of cached entries
    pub fn total(&self) -> usize {
        self.folder_children_count
            + self.user_roots_count
            + self.shared_with_me_count
            + self.file_cache_count
            + self.folder_cache_count
            + self.share_cache_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_folder_children_cache() {
        let cache = RuntimeMetadataCache::new();
        
        let folder_id = Uuid::new_v4();
        let mut index = FolderChildrenIndex::new(folder_id);
        
        let child = FolderChildEntry {
            id: Uuid::new_v4(),
            kind: "file".to_string(),
            name: "test.txt".to_string(),
            deleted: false,
            size: Some(100),
            mime: Some("text/plain".to_string()),
            updated_at: Utc::now(),
        };
        
        index.upsert_child(child.clone());
        cache.put_folder_children(index);
        
        let retrieved = cache.get_folder_children(folder_id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().children.len(), 1);
    }
    
    #[test]
    fn test_file_cache_invalidation() {
        let cache = RuntimeMetadataCache::new();
        
        let file = FileDocument::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            None,
            "test.txt".to_string(),
            "/test.txt".to_string(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            100,
            "text/plain".to_string(),
            "abc123".to_string(),
        );
        
        cache.put_file(file.clone());
        assert!(cache.get_file(file.id).is_some());
        
        cache.invalidate_file(file.id);
        assert!(cache.get_file(file.id).is_none());
    }
    
    #[test]
    fn test_on_file_created_updates_children() {
        let cache = RuntimeMetadataCache::new();
        
        let parent_id = Uuid::new_v4();
        let mut index = FolderChildrenIndex::new(parent_id);
        index.children = vec![]; // Start empty
        cache.put_folder_children(index);
        
        let file = FileDocument::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Some(parent_id),
            "new_file.txt".to_string(),
            "/parent/new_file.txt".to_string(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            100,
            "text/plain".to_string(),
            "abc123".to_string(),
        );
        
        cache.on_file_created(&file);
        
        let children = cache.get_folder_children(parent_id).unwrap();
        assert_eq!(children.children.len(), 1);
        assert_eq!(children.children[0].name, "new_file.txt");
    }
    
    #[test]
    fn test_cache_stats() {
        let cache = RuntimeMetadataCache::new();
        
        // Add some entries
        cache.put_folder_children(FolderChildrenIndex::new(Uuid::new_v4()));
        cache.put_folder(FolderDocument::new_root(Uuid::new_v4(), Uuid::new_v4()));
        
        let stats = cache.stats();
        assert_eq!(stats.folder_children_count, 1);
        assert_eq!(stats.folder_cache_count, 1);
        assert_eq!(stats.total(), 2);
    }
}
