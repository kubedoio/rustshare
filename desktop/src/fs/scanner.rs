//! Local file scanner for comparison with server state
//!
//! Scans synced folders and builds a snapshot of the local filesystem
//! that can be compared with the server state to detect changes.

use anyhow::Result;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing::{debug, trace, warn};
use walkdir::WalkDir;

/// Local file information
#[derive(Debug, Clone)]
pub struct LocalFileInfo {
    /// Relative path from sync root
    pub relative_path: PathBuf,
    /// Full local path
    pub absolute_path: PathBuf,
    /// File name
    pub name: String,
    /// File size in bytes
    pub size: u64,
    /// Last modified time
    pub modified_at: SystemTime,
    /// Content hash (SHA-256)
    pub content_hash: Option<String>,
}

/// Local folder information
#[derive(Debug, Clone)]
pub struct LocalFolderInfo {
    /// Relative path from sync root
    pub relative_path: PathBuf,
    /// Full local path
    pub absolute_path: PathBuf,
    /// Folder name
    pub name: String,
}

/// Scan result for a synced folder
#[derive(Debug)]
pub struct FolderScanResult {
    /// Root path that was scanned
    pub root_path: PathBuf,
    /// Files found
    pub files: Vec<LocalFileInfo>,
    /// Folders found
    pub folders: Vec<LocalFolderInfo>,
    /// Total size in bytes
    pub total_size: u64,
}

/// File scanner for local filesystem
pub struct FileScanner {
    /// Patterns to ignore (gitignore-style)
    ignore_patterns: Vec<String>,
    /// Whether to compute content hashes
    compute_hashes: bool,
}

impl FileScanner {
    /// Create a new file scanner
    pub fn new() -> Self {
        Self {
            ignore_patterns: Self::default_ignore_patterns(),
            compute_hashes: true,
        }
    }

    /// Create scanner without hash computation (faster)
    pub fn without_hashes() -> Self {
        Self {
            ignore_patterns: Self::default_ignore_patterns(),
            compute_hashes: false,
        }
    }

    /// Set ignore patterns
    pub fn with_ignore_patterns(mut self, patterns: Vec<String>) -> Self {
        self.ignore_patterns = patterns;
        self
    }

    /// Scan a folder
    pub fn scan_folder(&self, root_path: &Path) -> Result<FolderScanResult> {
        let canonical_root = root_path.canonicalize()?;
        
        trace!("Scanning folder: {}", canonical_root.display());

        let mut files = Vec::new();
        let mut folders = Vec::new();
        let mut total_size = 0u64;

        for entry in WalkDir::new(&canonical_root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !self.should_ignore(e.path()))
        {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    warn!("Error reading directory entry: {}", e);
                    continue;
                }
            };

            let absolute_path = entry.path().to_path_buf();
            let relative_path = pathdiff::diff_paths(&absolute_path, &canonical_root)
                .unwrap_or_else(|| absolute_path.clone());

            if entry.file_type().is_dir() {
                folders.push(LocalFolderInfo {
                    relative_path: relative_path.clone(),
                    absolute_path: absolute_path.clone(),
                    name: entry.file_name().to_string_lossy().to_string(),
                });
            } else if entry.file_type().is_file() {
                let metadata = match entry.metadata() {
                    Ok(m) => m,
                    Err(e) => {
                        warn!("Failed to read metadata for {}: {}", absolute_path.display(), e);
                        continue;
                    }
                };

                let size = metadata.len();
                let modified_at = metadata.modified().unwrap_or_else(|_| SystemTime::now());

                // Compute content hash if enabled
                let content_hash = if self.compute_hashes && size < 100 * 1024 * 1024 {
                    // Only hash files < 100MB during scan
                    match self.compute_file_hash(&absolute_path) {
                        Ok(hash) => Some(hash),
                        Err(e) => {
                            warn!("Failed to hash {}: {}", absolute_path.display(), e);
                            None
                        }
                    }
                } else {
                    None
                };

                files.push(LocalFileInfo {
                    relative_path: relative_path.clone(),
                    absolute_path: absolute_path.clone(),
                    name: entry.file_name().to_string_lossy().to_string(),
                    size,
                    modified_at,
                    content_hash,
                });

                total_size += size;
            }
        }

        debug!(
            "Scan complete: {} files, {} folders, {} bytes",
            files.len(),
            folders.len(),
            total_size
        );

        Ok(FolderScanResult {
            root_path: canonical_root,
            files,
            folders,
            total_size,
        })
    }

    /// Check if a path should be ignored
    fn should_ignore(&self, path: &Path) -> bool {
        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        // Check against ignore patterns
        for pattern in &self.ignore_patterns {
            if Self::matches_pattern(name, pattern) {
                trace!("Ignoring {} (matched pattern: {})", path.display(), pattern);
                return true;
            }
        }

        false
    }

    /// Simple pattern matching (supports * and ? wildcards)
    fn matches_pattern(name: &str, pattern: &str) -> bool {
        if pattern.starts_with("*") && pattern.ends_with("*") && pattern.len() > 2 {
            // *text* - contains
            let middle = &pattern[1..pattern.len()-1];
            name.contains(middle)
        } else if pattern.starts_with("*") {
            // *text - ends with
            name.ends_with(&pattern[1..])
        } else if pattern.ends_with("*") {
            // text* - starts with
            name.starts_with(&pattern[..pattern.len()-1])
        } else if pattern.starts_with(".") {
            // Hidden file pattern
            name.starts_with(".")
        } else {
            // Exact match
            name == pattern
        }
    }

    /// Compute SHA-256 hash of a file
    fn compute_file_hash(&self, path: &Path) -> Result<String> {
        use std::fs::File;
        use std::io::Read;

        let mut file = File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 8192];

        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        let result = hasher.finalize();
        Ok(hex::encode(result))
    }

    /// Default ignore patterns
    fn default_ignore_patterns() -> Vec<String> {
        vec![
            ".*".to_string(),
            "*.tmp".to_string(),
            "*.temp".to_string(),
            "*.swp".to_string(),
            "*.swo".to_string(),
            "*~".to_string(),
            "~$*".to_string(),
            "Thumbs.db".to_string(),
            ".DS_Store".to_string(),
            "desktop.ini".to_string(),
        ]
    }
}

/// Compare local and server states to detect changes
pub struct StateComparator;

/// Change type detected during comparison
#[derive(Debug, Clone)]
pub enum DetectedChange {
    /// New local file that needs to be uploaded
    LocalNew { relative_path: PathBuf },
    /// Local file modified since last sync
    LocalModified { relative_path: PathBuf },
    /// Local file deleted
    LocalDeleted { relative_path: PathBuf },
    /// File renamed locally (detected via hash match)
    LocalRenamed { from_path: PathBuf, to_path: PathBuf },
    /// New remote file that needs to be downloaded
    RemoteNew { file_id: uuid::Uuid, path: String },
    /// Remote file modified
    RemoteModified { file_id: uuid::Uuid, path: String },
    /// Remote file deleted
    RemoteDeleted { file_id: uuid::Uuid, path: String },
    /// Conflict: both local and remote modified
    Conflict { relative_path: PathBuf, file_id: uuid::Uuid },
}

impl StateComparator {
    /// Compare local scan with database state to find local changes
    pub fn find_local_changes(
        scan: &FolderScanResult,
        db_files: &[crate::db::FileState],
    ) -> Vec<DetectedChange> {
        let mut changes = Vec::new();

        // Build lookup maps
        let local_by_path: HashMap<&Path, &LocalFileInfo> = scan
            .files
            .iter()
            .map(|f| (f.relative_path.as_path(), f))
            .collect();

        let db_by_path: HashMap<&Path, &crate::db::FileState> = db_files
            .iter()
            .map(|f| (f.local_path.as_path(), f))
            .collect();

        // Find new and modified files
        for (path, local_file) in &local_by_path {
            match db_by_path.get(*path) {
                None => {
                    // Check if this might be a rename by looking for matching hash
                    let renamed_from = db_files.iter().find(|db_file| {
                        !local_by_path.contains_key(db_file.local_path.as_path())
                            && db_file.content_hash == local_file.content_hash.as_deref().unwrap_or("")
                    });

                    if let Some(old_file) = renamed_from {
                        changes.push(DetectedChange::LocalRenamed {
                            from_path: old_file.local_path.clone(),
                            to_path: (*path).to_path_buf(),
                        });
                    } else {
                        changes.push(DetectedChange::LocalNew {
                            relative_path: (*path).to_path_buf(),
                        });
                    }
                }
                Some(db_file) => {
                    // Check if modified
                    if local_file.content_hash.as_deref() != Some(&db_file.content_hash) {
                        changes.push(DetectedChange::LocalModified {
                            relative_path: (*path).to_path_buf(),
                        });
                    }
                }
            }
        }

        // Find deleted files
        for (path, db_file) in &db_by_path {
            if !local_by_path.contains_key(*path) {
                // Check if it was renamed
                let renamed_to = scan.files.iter().find(|local_file| {
                    local_file.content_hash.as_deref() == Some(&db_file.content_hash)
                });

                if renamed_to.is_none() {
                    changes.push(DetectedChange::LocalDeleted {
                        relative_path: (*path).to_path_buf(),
                    });
                }
            }
        }

        changes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_scan_folder() {
        let temp_dir = TempDir::new().unwrap();
        
        // Create test files
        let file1 = temp_dir.path().join("file1.txt");
        let mut f = File::create(&file1).unwrap();
        f.write_all(b"Hello").unwrap();
        
        let subdir = temp_dir.path().join("subdir");
        std::fs::create_dir(&subdir).unwrap();
        let file2 = subdir.join("file2.txt");
        let mut f = File::create(&file2).unwrap();
        f.write_all(b"World").unwrap();

        let scanner = FileScanner::new();
        let result = scanner.scan_folder(temp_dir.path()).unwrap();

        assert_eq!(result.files.len(), 2);
        assert_eq!(result.folders.len(), 2); // subdir + root
        assert_eq!(result.total_size, 10);
    }

    #[test]
    fn test_ignore_patterns() {
        assert!(FileScanner::matches_pattern(".hidden", ".*"));
        assert!(FileScanner::matches_pattern("test.tmp", "*.tmp"));
        assert!(FileScanner::matches_pattern("temp_file.txt", "temp*"));
        assert!(!FileScanner::matches_pattern("normal.txt", ".*"));
    }
}
