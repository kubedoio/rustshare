use anyhow::{bail, Context, Result};
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

/// Scan a local root directory and return file metadata for all entries
pub fn scan_local_root(root_path: &Path) -> Result<Vec<FileScanResult>> {
    trace!(path = %root_path.display(), "Starting directory scan");

    let mut results = Vec::new();
    let canonical_root = root_path
        .canonicalize()
        .with_context(|| format!("Failed to canonicalize root path: {}", root_path.display()))?;

    for entry in WalkDir::new(&canonical_root).follow_links(false) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                // Abort the scan instead of treating the subtree as absent: a
                // transient read error must never look like the files were
                // deleted, or the planner would propagate DeleteRemote for
                // them (and DeleteLocal for local edits under the subtree).
                bail!("Failed to walk {}: {}", canonical_root.display(), e);
            }
        };
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
}
