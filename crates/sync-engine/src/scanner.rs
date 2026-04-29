use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing::{debug, trace, warn};
use walkdir::WalkDir;

/// Result of scanning a single file or directory
#[derive(Debug, Clone)]
pub struct FileScanResult {
    pub relative_path: PathBuf,
    pub absolute_path: PathBuf,
    pub hash: String, // SHA-256 (empty for directories)
    pub size: u64,
    pub modified_at: u64, // Unix timestamp
    pub is_directory: bool,
}

/// Represents changes detected between local scan and database state
#[derive(Debug, Default)]
pub struct LocalChanges {
    pub created: Vec<FileScanResult>,  // In scan, not in DB
    pub modified: Vec<FileScanResult>, // Hash or mtime different
    pub deleted: Vec<PathBuf>,         // In DB, not in scan
}

/// Scan a local root directory and return file metadata for all entries
pub fn scan_local_root(root_path: &Path) -> Result<Vec<FileScanResult>> {
    trace!(path = %root_path.display(), "Starting directory scan");

    let mut results = Vec::new();
    let canonical_root = root_path
        .canonicalize()
        .with_context(|| format!("Failed to canonicalize root path: {}", root_path.display()))?;

    for entry in WalkDir::new(&canonical_root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let absolute_path = entry.path();

        // Skip the root directory itself
        if absolute_path == canonical_root {
            debug!("Skipping root directory");
            continue;
        }

        // Compute relative path
        let relative_path = match absolute_path.strip_prefix(&canonical_root) {
            Ok(p) => p.to_path_buf(),
            Err(_) => {
                warn!(
                    "Could not compute relative path for: {}",
                    absolute_path.display()
                );
                continue;
            }
        };

        // Get metadata
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(e) => {
                warn!(
                    "Failed to get metadata for {}: {}",
                    absolute_path.display(),
                    e
                );
                continue;
            }
        };

        let is_directory = metadata.is_dir();
        let size = if is_directory { 0 } else { metadata.len() };

        // Get modified time as unix timestamp
        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Compute hash for files only (not directories)
        let hash = if is_directory {
            String::new()
        } else {
            match file_ops::calculate_hash(absolute_path) {
                Ok(h) => h,
                Err(e) => {
                    warn!(
                        "Failed to compute hash for {}: {}",
                        absolute_path.display(),
                        e
                    );
                    continue;
                }
            }
        };

        results.push(FileScanResult {
            relative_path,
            absolute_path: absolute_path.to_path_buf(),
            hash,
            size,
            modified_at,
            is_directory,
        });
    }

    debug!(count = results.len(), "Directory scan complete");
    Ok(results)
}

/// Detect changes by comparing current scan with database state
///
/// This function compares a fresh scan against known state (represented by
/// a function that retrieves the stored metadata for a given path) to
/// detect created, modified, and deleted files.
///
/// # Arguments
/// * `current_scan` - The current results from `scan_local_root`
/// * `db_lookup` - A function that returns the stored (hash, modified_at, size) for a path if known
pub fn detect_local_changes<F>(current_scan: &[FileScanResult], mut db_lookup: F) -> LocalChanges
where
    F: FnMut(&Path) -> Option<(String, u64, u64)>,
{
    trace!("Detecting local changes");

    let mut changes = LocalChanges::default();
    let mut scanned_paths: std::collections::HashSet<&Path> = std::collections::HashSet::new();

    for scan_result in current_scan {
        let path = &scan_result.relative_path;
        scanned_paths.insert(path);

        match db_lookup(path) {
            Some((stored_hash, stored_mtime, stored_size)) => {
                // File exists in DB, check if modified
                let hash_changed = !scan_result.is_directory && scan_result.hash != stored_hash;
                let mtime_changed = scan_result.modified_at != stored_mtime;
                let size_changed = scan_result.size != stored_size;

                if hash_changed || mtime_changed || size_changed {
                    debug!(
                        path = %path.display(),
                        hash_changed, mtime_changed, size_changed,
                        "Detected modified file"
                    );
                    changes.modified.push(scan_result.clone());
                }
            }
            None => {
                // File not in DB, it's new
                debug!(path = %path.display(), "Detected new file");
                changes.created.push(scan_result.clone());
            }
        }
    }

    // Find deleted files by iterating through known DB paths
    // This would require the caller to provide the full set of known paths
    // For now, we return the partial result; deleted detection is typically
    // done at a higher level with knowledge of all DB entries for the root

    debug!(
        created = changes.created.len(),
        modified = changes.modified.len(),
        "Change detection complete (deleted requires full DB scan)"
    );

    changes
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_compute_file_hash() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let mut file = File::create(&file_path).unwrap();
        file.write_all(b"hello world").unwrap();
        drop(file);

        let hash = file_ops::calculate_hash(&file_path).unwrap();

        // Known SHA-256 hash for "hello world"
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_scan_local_root() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create test files
        let file1 = root.join("file1.txt");
        let subdir = root.join("subdir");
        let file2 = subdir.join("file2.txt");

        std::fs::create_dir(&subdir).unwrap();
        File::create(&file1)
            .unwrap()
            .write_all(b"content1")
            .unwrap();
        File::create(&file2)
            .unwrap()
            .write_all(b"content2")
            .unwrap();

        let results = scan_local_root(root).unwrap();

        assert_eq!(results.len(), 3); // subdir, file1.txt, subdir/file2.txt

        // Check that we have the expected files
        let paths: Vec<_> = results
            .iter()
            .map(|r| r.relative_path.to_string_lossy().to_string())
            .collect();

        assert!(paths.contains(&"file1.txt".to_string()));
        assert!(paths.contains(&"subdir".to_string()));
        assert!(paths.contains(&"subdir/file2.txt".to_string()));
    }

    #[test]
    fn test_detect_local_changes_created() {
        let scan_result = FileScanResult {
            relative_path: PathBuf::from("new_file.txt"),
            absolute_path: PathBuf::from("/tmp/new_file.txt"),
            hash: "abc123".to_string(),
            size: 100,
            modified_at: 1234567890,
            is_directory: false,
        };

        let changes = detect_local_changes(&[scan_result], |_path| None);

        assert_eq!(changes.created.len(), 1);
        assert_eq!(changes.modified.len(), 0);
        assert_eq!(changes.deleted.len(), 0);
    }

    #[test]
    fn test_detect_local_changes_modified() {
        let scan_result = FileScanResult {
            relative_path: PathBuf::from("existing.txt"),
            absolute_path: PathBuf::from("/tmp/existing.txt"),
            hash: "new_hash".to_string(),
            size: 200,
            modified_at: 1234567890,
            is_directory: false,
        };

        // Simulate DB having different hash/mtime
        let changes = detect_local_changes(&[scan_result], |path| {
            if path == Path::new("existing.txt") {
                Some(("old_hash".to_string(), 1000000000, 100))
            } else {
                None
            }
        });

        assert_eq!(changes.created.len(), 0);
        assert_eq!(changes.modified.len(), 1);
        assert_eq!(changes.deleted.len(), 0);
    }
}
