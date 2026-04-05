//! Local file index for efficient lookups
//!
//! Maintains an in-memory index of files in synced folders
//! for fast path-to-file resolution.

use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Entry in the file index
#[derive(Debug, Clone)]
pub struct FileIndexEntry {
    /// Relative path from sync root
    pub relative_path: PathBuf,
    /// Server file ID (if synced)
    pub file_id: Option<uuid::Uuid>,
    /// Content hash
    pub content_hash: String,
    /// File size
    pub size: u64,
    /// Last modified time (UNIX timestamp)
    pub modified_at: u64,
}

/// In-memory file index for a synced folder
#[derive(Debug, Default)]
pub struct FileIndex {
    /// Map from relative path to entry
    by_path: HashMap<PathBuf, FileIndexEntry>,
    /// Map from file ID to relative path
    by_id: HashMap<uuid::Uuid, PathBuf>,
    /// Map from content hash to entries (for rename detection)
    by_hash: HashMap<String, Vec<PathBuf>>,
}

impl FileIndex {
    /// Create a new empty index
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or update an entry
    pub fn insert(&mut self, entry: FileIndexEntry) {
        // Update by_id map
        if let Some(old_id) = self.by_path.get(&entry.relative_path).and_then(|e| e.file_id) {
            self.by_id.remove(&old_id);
        }
        if let Some(file_id) = entry.file_id {
            self.by_id.insert(file_id, entry.relative_path.clone());
        }

        // Update by_hash map
        let old_hash = self.by_path.get(&entry.relative_path).map(|e| e.content_hash.clone());
        if let Some(old_hash) = old_hash {
            if let Some(paths) = self.by_hash.get_mut(&old_hash) {
                paths.retain(|p| p != &entry.relative_path);
            }
        }
        self.by_hash
            .entry(entry.content_hash.clone())
            .or_default()
            .push(entry.relative_path.clone());

        // Update main index
        self.by_path.insert(entry.relative_path.clone(), entry);
    }

    /// Remove an entry by path
    pub fn remove_by_path(&mut self, path: &Path) -> Option<FileIndexEntry> {
        let entry = self.by_path.remove(path)?;

        // Clean up by_id
        if let Some(file_id) = entry.file_id {
            self.by_id.remove(&file_id);
        }

        // Clean up by_hash
        if let Some(paths) = self.by_hash.get_mut(&entry.content_hash) {
            paths.retain(|p| p != path);
        }

        Some(entry)
    }

    /// Remove an entry by file ID
    pub fn remove_by_id(&mut self, file_id: uuid::Uuid) -> Option<FileIndexEntry> {
        let path = self.by_id.remove(&file_id)?;
        self.remove_by_path(&path)
    }

    /// Get entry by path
    pub fn get_by_path(&self, path: &Path) -> Option<&FileIndexEntry> {
        self.by_path.get(path)
    }

    /// Get entry by file ID
    pub fn get_by_id(&self, file_id: uuid::Uuid) -> Option<&FileIndexEntry> {
        self.by_id.get(&file_id).and_then(|p| self.by_path.get(p))
    }

    /// Find entries by content hash (for rename detection)
    pub fn get_by_hash(&self, hash: &str) -> Vec<&FileIndexEntry> {
        self.by_hash
            .get(hash)
            .map(|paths| {
                paths
                    .iter()
                    .filter_map(|p| self.by_path.get(p))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Check if path exists in index
    pub fn contains_path(&self, path: &Path) -> bool {
        self.by_path.contains_key(path)
    }

    /// Check if file ID exists in index
    pub fn contains_id(&self, file_id: uuid::Uuid) -> bool {
        self.by_id.contains_key(&file_id)
    }

    /// Get all entries
    pub fn entries(&self) -> impl Iterator<Item = &FileIndexEntry> {
        self.by_path.values()
    }

    /// Get count of entries
    pub fn len(&self) -> usize {
        self.by_path.len()
    }

    /// Check if index is empty
    pub fn is_empty(&self) -> bool {
        self.by_path.is_empty()
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.by_path.clear();
        self.by_id.clear();
        self.by_hash.clear();
    }

    /// Find potential renames by looking for paths with same hash but different location
    pub fn find_renames(&self) -> Vec<(&Path, &Path)> {
        let renames = Vec::new();

        for (hash, paths) in &self.by_hash {
            if paths.len() >= 2 {
                // Multiple files with same hash - potential duplicates or moves
                // In a real implementation, we'd track file identity separately
                trace!("Found {} files with hash {}", paths.len(), hash);
            }
        }

        renames
    }

    /// Build index from database file states
    pub fn build_from_db(folder_id: uuid::Uuid, db: &crate::db::Database) -> Result<Self> {
        let mut index = Self::new();

        let file_states = db.get_folder_file_states(folder_id)?;
        
        for state in file_states {
            let entry = FileIndexEntry {
                relative_path: state.local_path.clone(),
                file_id: Some(state.file_id),
                content_hash: state.content_hash,
                size: state.size as u64,
                modified_at: state.local_modified_at.timestamp() as u64,
            };
            index.insert(entry);
        }

        Ok(index)
    }

    /// Build index from scan result
    pub fn build_from_scan(scan: &crate::fs::scanner::FolderScanResult) -> Self {
        let mut index = Self::new();

        for file in &scan.files {
            let entry = FileIndexEntry {
                relative_path: file.relative_path.clone(),
                file_id: None,
                content_hash: file.content_hash.clone().unwrap_or_default(),
                size: file.size,
                modified_at: file
                    .modified_at
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            };
            index.insert(entry);
        }

        index
    }
}

use tracing::trace;

#[cfg(test)]
mod tests {
    use super::*;

    fn create_entry(path: &str, file_id: Option<uuid::Uuid>, hash: &str) -> FileIndexEntry {
        FileIndexEntry {
            relative_path: path.into(),
            file_id,
            content_hash: hash.to_string(),
            size: 100,
            modified_at: 1234567890,
        }
    }

    #[test]
    fn test_insert_and_get() {
        let mut index = FileIndex::new();
        
        let file_id = uuid::Uuid::new_v4();
        let entry = create_entry("test.txt", Some(file_id), "abc123");
        
        index.insert(entry.clone());
        
        assert!(index.contains_path(Path::new("test.txt")));
        assert!(index.contains_id(file_id));
        
        let retrieved = index.get_by_path(Path::new("test.txt"));
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().file_id, Some(file_id));
    }

    #[test]
    fn test_remove() {
        let mut index = FileIndex::new();
        
        let file_id = uuid::Uuid::new_v4();
        let entry = create_entry("test.txt", Some(file_id), "abc123");
        
        index.insert(entry);
        
        let removed = index.remove_by_path(Path::new("test.txt"));
        assert!(removed.is_some());
        assert!(!index.contains_path(Path::new("test.txt")));
        assert!(!index.contains_id(file_id));
    }

    #[test]
    fn test_find_by_hash() {
        let mut index = FileIndex::new();
        
        let entry1 = create_entry("file1.txt", None, "same_hash");
        let entry2 = create_entry("file2.txt", None, "same_hash");
        
        index.insert(entry1);
        index.insert(entry2);
        
        let by_hash = index.get_by_hash("same_hash");
        assert_eq!(by_hash.len(), 2);
    }
}
