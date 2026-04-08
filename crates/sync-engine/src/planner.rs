use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

use crate::scanner::FileScanResult;

/// Information about a file on the remote server
#[derive(Debug, Clone)]
pub struct RemoteFileInfo {
    pub id: Uuid,
    pub relative_path: PathBuf,
    pub hash: String,
    pub size: u64,
    pub modified_at: u64,
}

/// An operation to be executed during sync
#[derive(Debug, Clone)]
pub enum SyncOp {
    /// Upload a local file to the remote
    Upload {
        root_id: Uuid,
        relative_path: PathBuf,
        local_path: PathBuf,
        size: u64,
    },
    /// Download a remote file to local
    Download {
        root_id: Uuid,
        relative_path: PathBuf,
        remote_file_id: Uuid,
        remote_hash: String,
        size: u64,
    },
    /// Delete a local file
    DeleteLocal {
        root_id: Uuid,
        relative_path: PathBuf,
    },
    /// Delete a remote file
    DeleteRemote {
        root_id: Uuid,
        relative_path: PathBuf,
        remote_file_id: Uuid,
    },
}

/// How to resolve a sync conflict
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictResolution {
    /// Upload local version (local is newer)
    UploadLocal,
    /// Download remote version (remote is newer)
    DownloadRemote,
}

/// A detected sync conflict
#[derive(Debug, Clone)]
pub struct Conflict {
    pub root_id: Uuid,
    pub relative_path: PathBuf,
    pub local_modified_at: u64,
    pub remote_modified_at: u64,
    pub resolution: ConflictResolution,
}

/// The complete plan for a sync operation
#[derive(Debug, Default)]
pub struct SyncPlan {
    pub uploads: Vec<SyncOp>,
    pub downloads: Vec<SyncOp>,
    pub deletes: Vec<SyncOp>,
    pub conflicts: Vec<Conflict>,
}

impl SyncPlan {
    /// Returns true if the plan has no operations or conflicts
    pub fn is_empty(&self) -> bool {
        self.uploads.is_empty()
            && self.downloads.is_empty()
            && self.deletes.is_empty()
            && self.conflicts.is_empty()
    }

    /// Returns the total number of operations (excluding conflicts)
    pub fn operation_count(&self) -> usize {
        self.uploads.len() + self.downloads.len() + self.deletes.len()
    }

    /// Returns the total number of conflicts
    pub fn conflict_count(&self) -> usize {
        self.conflicts.len()
    }
}

/// Database state for a single file
#[derive(Debug, Clone)]
struct DbFileState {
    hash: String,
    modified_at: u64,
    _remote_id: Option<Uuid>,
}

/// Generate a sync plan by comparing local, remote, and database states
///
/// # Arguments
/// * `root_id` - The UUID of the sync root
/// * `root_path` - The local filesystem path of the sync root
/// * `local_files` - Files discovered by the local scanner
/// * `remote_files` - Files from the remote server
/// * `db_lookup` - Function to retrieve stored file state from the database
///
/// # Algorithm
/// For each file in the union of local, remote, and DB:
/// - If in local AND in remote:
///     - If both changed (vs DB): CONFLICT → resolve by timestamp
///     - If only local changed: UPLOAD
///     - If only remote changed: DOWNLOAD
///     - If neither changed: No operation (in sync)
/// - If only in local (not in DB): UPLOAD (new file)
/// - If only in remote (not in DB): DOWNLOAD (new file)
/// - If in DB but missing locally: DELETE REMOTE
/// - If in DB but missing remotely: DELETE LOCAL
pub fn generate_plan<F>(
    root_id: Uuid,
    _root_path: &std::path::Path,
    local_files: &[FileScanResult],
    remote_files: &[RemoteFileInfo],
    mut db_lookup: F,
) -> SyncPlan
where
    F: FnMut(&PathBuf) -> Option<(String, u64, Option<Uuid>)>,
{
    let mut plan = SyncPlan::default();

    // Build lookup maps for efficient access
    let local_map: HashMap<&PathBuf, &FileScanResult> = local_files
        .iter()
        .map(|f| (&f.relative_path, f))
        .collect();

    let remote_map: HashMap<&PathBuf, &RemoteFileInfo> = remote_files
        .iter()
        .map(|f| (&f.relative_path, f))
        .collect();

    // Collect all unique paths from local, remote, and DB
    let mut all_paths: std::collections::HashSet<&PathBuf> = std::collections::HashSet::new();
    all_paths.extend(local_map.keys());
    all_paths.extend(remote_map.keys());

    // Also add paths that exist in DB but may not be in local or remote
    // (this requires the caller to provide DB paths separately if needed)

    for relative_path in all_paths {
        let local = local_map.get(relative_path).copied();
        let remote = remote_map.get(relative_path).copied();
        let db = db_lookup(relative_path)
            .map(|(hash, modified_at, remote_id)| DbFileState {
                hash,
                modified_at,
                _remote_id: remote_id,
            });

        match (local, remote, db) {
            // File exists in all three: local, remote, and DB
            (Some(local), Some(remote), Some(db)) => {
                let local_changed = local.hash != db.hash || local.modified_at != db.modified_at;
                let remote_changed = remote.hash != db.hash || remote.modified_at != db.modified_at;

                if local_changed && remote_changed {
                    // Conflict: both changed
                    let resolution = if local.modified_at >= remote.modified_at {
                        ConflictResolution::UploadLocal
                    } else {
                        ConflictResolution::DownloadRemote
                    };

                    plan.conflicts.push(Conflict {
                        root_id,
                        relative_path: relative_path.clone(),
                        local_modified_at: local.modified_at,
                        remote_modified_at: remote.modified_at,
                        resolution,
                    });

                    // Add the resolved operation
                    match resolution {
                        ConflictResolution::UploadLocal => {
                            plan.uploads.push(SyncOp::Upload {
                                root_id,
                                relative_path: relative_path.clone(),
                                local_path: local.absolute_path.clone(),
                                size: local.size,
                            });
                        }
                        ConflictResolution::DownloadRemote => {
                            plan.downloads.push(SyncOp::Download {
                                root_id,
                                relative_path: relative_path.clone(),
                                remote_file_id: remote.id,
                                remote_hash: remote.hash.clone(),
                                size: remote.size,
                            });
                        }
                    }
                } else if local_changed {
                    // Only local changed: upload
                    plan.uploads.push(SyncOp::Upload {
                        root_id,
                        relative_path: relative_path.clone(),
                        local_path: local.absolute_path.clone(),
                        size: local.size,
                    });
                } else if remote_changed {
                    // Only remote changed: download
                    plan.downloads.push(SyncOp::Download {
                        root_id,
                        relative_path: relative_path.clone(),
                        remote_file_id: remote.id,
                        remote_hash: remote.hash.clone(),
                        size: remote.size,
                    });
                }
                // If neither changed: no operation needed (already in sync)
            }

            // File in local and remote but not in DB: treat as new file on both sides
            (Some(local), Some(remote), None) => {
                // Both exist but not in DB - conflict, resolve by timestamp
                let resolution = if local.modified_at >= remote.modified_at {
                    ConflictResolution::UploadLocal
                } else {
                    ConflictResolution::DownloadRemote
                };

                plan.conflicts.push(Conflict {
                    root_id,
                    relative_path: relative_path.clone(),
                    local_modified_at: local.modified_at,
                    remote_modified_at: remote.modified_at,
                    resolution,
                });

                match resolution {
                    ConflictResolution::UploadLocal => {
                        plan.uploads.push(SyncOp::Upload {
                            root_id,
                            relative_path: relative_path.clone(),
                            local_path: local.absolute_path.clone(),
                            size: local.size,
                        });
                    }
                    ConflictResolution::DownloadRemote => {
                        plan.downloads.push(SyncOp::Download {
                            root_id,
                            relative_path: relative_path.clone(),
                            remote_file_id: remote.id,
                            remote_hash: remote.hash.clone(),
                            size: remote.size,
                        });
                    }
                }
            }

            // File only in local, not in remote or DB: upload new file
            (Some(local), None, None) => {
                plan.uploads.push(SyncOp::Upload {
                    root_id,
                    relative_path: relative_path.clone(),
                    local_path: local.absolute_path.clone(),
                    size: local.size,
                });
            }

            // File only in remote, not in local or DB: download new file
            (None, Some(remote), None) => {
                plan.downloads.push(SyncOp::Download {
                    root_id,
                    relative_path: relative_path.clone(),
                    remote_file_id: remote.id,
                    remote_hash: remote.hash.clone(),
                    size: remote.size,
                });
            }

            // File in DB and local but not remote: was deleted remotely, delete locally
            (Some(_local), None, Some(_db)) => {
                plan.deletes.push(SyncOp::DeleteLocal {
                    root_id,
                    relative_path: relative_path.clone(),
                });
            }

            // File in DB and remote but not local: was deleted locally, delete remotely
            (None, Some(remote), Some(_db)) => {
                plan.deletes.push(SyncOp::DeleteRemote {
                    root_id,
                    relative_path: relative_path.clone(),
                    remote_file_id: remote.id,
                });
            }

            // File only in DB: orphaned entry, no action needed (will be cleaned up)
            (None, None, Some(_)) => {
                // No operation - this is a database cleanup scenario
            }

            // Should never happen (all None)
            (None, None, None) => {}
        }
    }

    plan
}

/// Extended version that also handles DB-only files (files that were tracked but may have been deleted)
///
/// This version requires the full list of paths from the database to properly detect deletions.
pub fn generate_plan_with_db_files<F>(
    root_id: Uuid,
    _root_path: &std::path::Path,
    local_files: &[FileScanResult],
    remote_files: &[RemoteFileInfo],
    db_paths: &[PathBuf],
    mut db_lookup: F,
) -> SyncPlan
where
    F: FnMut(&PathBuf) -> Option<(String, u64, Option<Uuid>)>,
{
    let mut plan = SyncPlan::default();

    // Build lookup maps
    let local_map: HashMap<&PathBuf, &FileScanResult> = local_files
        .iter()
        .map(|f| (&f.relative_path, f))
        .collect();

    let remote_map: HashMap<&PathBuf, &RemoteFileInfo> = remote_files
        .iter()
        .map(|f| (&f.relative_path, f))
        .collect();

    let db_set: std::collections::HashSet<&PathBuf> = db_paths.iter().collect();

    // First pass: process all paths from local and remote
    let mut all_paths: std::collections::HashSet<&PathBuf> = std::collections::HashSet::new();
    all_paths.extend(local_map.keys());
    all_paths.extend(remote_map.keys());
    all_paths.extend(&db_set);

    for relative_path in all_paths {
        let local = local_map.get(relative_path).copied();
        let remote = remote_map.get(relative_path).copied();
        let db = db_lookup(relative_path)
            .map(|(hash, modified_at, remote_id)| DbFileState {
                hash,
                modified_at,
                _remote_id: remote_id,
            });

        match (local, remote, db) {
            (Some(local), Some(remote), Some(db)) => {
                let local_changed = local.hash != db.hash || local.modified_at != db.modified_at;
                let remote_changed = remote.hash != db.hash || remote.modified_at != db.modified_at;

                if local_changed && remote_changed {
                    let resolution = if local.modified_at >= remote.modified_at {
                        ConflictResolution::UploadLocal
                    } else {
                        ConflictResolution::DownloadRemote
                    };

                    plan.conflicts.push(Conflict {
                        root_id,
                        relative_path: relative_path.clone(),
                        local_modified_at: local.modified_at,
                        remote_modified_at: remote.modified_at,
                        resolution,
                    });

                    match resolution {
                        ConflictResolution::UploadLocal => {
                            plan.uploads.push(SyncOp::Upload {
                                root_id,
                                relative_path: relative_path.clone(),
                                local_path: local.absolute_path.clone(),
                                size: local.size,
                            });
                        }
                        ConflictResolution::DownloadRemote => {
                            plan.downloads.push(SyncOp::Download {
                                root_id,
                                relative_path: relative_path.clone(),
                                remote_file_id: remote.id,
                                remote_hash: remote.hash.clone(),
                                size: remote.size,
                            });
                        }
                    }
                } else if local_changed {
                    plan.uploads.push(SyncOp::Upload {
                        root_id,
                        relative_path: relative_path.clone(),
                        local_path: local.absolute_path.clone(),
                        size: local.size,
                    });
                } else if remote_changed {
                    plan.downloads.push(SyncOp::Download {
                        root_id,
                        relative_path: relative_path.clone(),
                        remote_file_id: remote.id,
                        remote_hash: remote.hash.clone(),
                        size: remote.size,
                    });
                }
            }

            (Some(local), Some(remote), None) => {
                let resolution = if local.modified_at >= remote.modified_at {
                    ConflictResolution::UploadLocal
                } else {
                    ConflictResolution::DownloadRemote
                };

                plan.conflicts.push(Conflict {
                    root_id,
                    relative_path: relative_path.clone(),
                    local_modified_at: local.modified_at,
                    remote_modified_at: remote.modified_at,
                    resolution,
                });

                match resolution {
                    ConflictResolution::UploadLocal => {
                        plan.uploads.push(SyncOp::Upload {
                            root_id,
                            relative_path: relative_path.clone(),
                            local_path: local.absolute_path.clone(),
                            size: local.size,
                        });
                    }
                    ConflictResolution::DownloadRemote => {
                        plan.downloads.push(SyncOp::Download {
                            root_id,
                            relative_path: relative_path.clone(),
                            remote_file_id: remote.id,
                            remote_hash: remote.hash.clone(),
                            size: remote.size,
                        });
                    }
                }
            }

            (Some(local), None, None) => {
                plan.uploads.push(SyncOp::Upload {
                    root_id,
                    relative_path: relative_path.clone(),
                    local_path: local.absolute_path.clone(),
                    size: local.size,
                });
            }

            (None, Some(remote), None) => {
                plan.downloads.push(SyncOp::Download {
                    root_id,
                    relative_path: relative_path.clone(),
                    remote_file_id: remote.id,
                    remote_hash: remote.hash.clone(),
                    size: remote.size,
                });
            }

            (Some(_local), None, Some(_db)) => {
                plan.deletes.push(SyncOp::DeleteLocal {
                    root_id,
                    relative_path: relative_path.clone(),
                });
            }

            (None, Some(remote), Some(_db)) => {
                plan.deletes.push(SyncOp::DeleteRemote {
                    root_id,
                    relative_path: relative_path.clone(),
                    remote_file_id: remote.id,
                });
            }

            // File only in DB: delete from DB (cleanup)
            (None, None, Some(_)) => {
                // This file was tracked but no longer exists anywhere
                // The DB entry should be removed (handled elsewhere)
            }

            (None, None, None) => {}
        }
    }

    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn create_local_file(path: &str, hash: &str, modified_at: u64) -> FileScanResult {
        FileScanResult {
            relative_path: PathBuf::from(path),
            absolute_path: PathBuf::from("/test").join(path),
            hash: hash.to_string(),
            size: 100,
            modified_at,
            is_directory: false,
        }
    }

    fn create_remote_file(
        id: Uuid,
        path: &str,
        hash: &str,
        modified_at: u64,
    ) -> RemoteFileInfo {
        RemoteFileInfo {
            id,
            relative_path: PathBuf::from(path),
            hash: hash.to_string(),
            size: 100,
            modified_at,
        }
    }

    #[test]
    fn test_empty_plan() {
        let root_id = Uuid::new_v4();
        let plan = generate_plan(
            root_id,
            Path::new("/test"),
            &[],
            &[],
            |_path| None,
        );

        assert!(plan.is_empty());
        assert_eq!(plan.operation_count(), 0);
        assert_eq!(plan.conflict_count(), 0);
    }

    #[test]
    fn test_upload_new_local_file() {
        let root_id = Uuid::new_v4();
        let local = create_local_file("new.txt", "hash1", 1000);

        let plan = generate_plan(
            root_id,
            Path::new("/test"),
            &[local],
            &[],
            |_path| None,
        );

        assert_eq!(plan.uploads.len(), 1);
        assert_eq!(plan.downloads.len(), 0);
        assert_eq!(plan.deletes.len(), 0);
        assert!(plan.conflicts.is_empty());

        match &plan.uploads[0] {
            SyncOp::Upload { relative_path, .. } => {
                assert_eq!(relative_path, &PathBuf::from("new.txt"));
            }
            _ => panic!("Expected Upload operation"),
        }
    }

    #[test]
    fn test_download_new_remote_file() {
        let root_id = Uuid::new_v4();
        let remote_id = Uuid::new_v4();
        let remote = create_remote_file(remote_id, "remote.txt", "hash1", 1000);

        let plan = generate_plan(
            root_id,
            Path::new("/test"),
            &[],
            &[remote],
            |_path| None,
        );

        assert_eq!(plan.uploads.len(), 0);
        assert_eq!(plan.downloads.len(), 1);
        assert_eq!(plan.deletes.len(), 0);
        assert!(plan.conflicts.is_empty());

        match &plan.downloads[0] {
            SyncOp::Download { relative_path, remote_file_id, .. } => {
                assert_eq!(relative_path, &PathBuf::from("remote.txt"));
                assert_eq!(*remote_file_id, remote_id);
            }
            _ => panic!("Expected Download operation"),
        }
    }

    #[test]
    fn test_local_modification_upload() {
        let root_id = Uuid::new_v4();
        let remote_id = Uuid::new_v4();

        // Local file has different hash than what's in DB
        let local = create_local_file("modified.txt", "new_hash", 2000);
        let remote = create_remote_file(remote_id, "modified.txt", "old_hash", 1000);

        let plan = generate_plan(
            root_id,
            Path::new("/test"),
            &[local],
            &[remote],
            |path| {
                if path == &PathBuf::from("modified.txt") {
                    Some(("old_hash".to_string(), 1000, Some(remote_id)))
                } else {
                    None
                }
            },
        );

        assert_eq!(plan.uploads.len(), 1);
        assert!(plan.downloads.is_empty());
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn test_remote_modification_download() {
        let root_id = Uuid::new_v4();
        let remote_id = Uuid::new_v4();

        // Remote file has different hash than what's in DB
        let local = create_local_file("modified.txt", "old_hash", 1000);
        let remote = create_remote_file(remote_id, "modified.txt", "new_hash", 2000);

        let plan = generate_plan(
            root_id,
            Path::new("/test"),
            &[local],
            &[remote],
            |path| {
                if path == &PathBuf::from("modified.txt") {
                    Some(("old_hash".to_string(), 1000, Some(remote_id)))
                } else {
                    None
                }
            },
        );

        assert!(plan.uploads.is_empty());
        assert_eq!(plan.downloads.len(), 1);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn test_conflict_local_newer() {
        let root_id = Uuid::new_v4();
        let remote_id = Uuid::new_v4();

        // Both changed, local is newer
        let local = create_local_file("conflict.txt", "local_hash", 2000);
        let remote = create_remote_file(remote_id, "conflict.txt", "remote_hash", 1500);

        let plan = generate_plan(
            root_id,
            Path::new("/test"),
            &[local],
            &[remote],
            |path| {
                if path == &PathBuf::from("conflict.txt") {
                    Some(("old_hash".to_string(), 1000, Some(remote_id)))
                } else {
                    None
                }
            },
        );

        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(plan.conflicts[0].resolution, ConflictResolution::UploadLocal);
        assert_eq!(plan.uploads.len(), 1); // Resolved as upload
    }

    #[test]
    fn test_conflict_remote_newer() {
        let root_id = Uuid::new_v4();
        let remote_id = Uuid::new_v4();

        // Both changed, remote is newer
        let local = create_local_file("conflict.txt", "local_hash", 1500);
        let remote = create_remote_file(remote_id, "conflict.txt", "remote_hash", 2000);

        let plan = generate_plan(
            root_id,
            Path::new("/test"),
            &[local],
            &[remote],
            |path| {
                if path == &PathBuf::from("conflict.txt") {
                    Some(("old_hash".to_string(), 1000, Some(remote_id)))
                } else {
                    None
                }
            },
        );

        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(plan.conflicts[0].resolution, ConflictResolution::DownloadRemote);
        assert_eq!(plan.downloads.len(), 1); // Resolved as download
    }

    #[test]
    fn test_delete_local_when_remote_deleted() {
        let root_id = Uuid::new_v4();
        let remote_id = Uuid::new_v4();

        // File exists in DB and local, but not remote
        let local = create_local_file("deleted.txt", "hash", 1000);

        let plan = generate_plan(
            root_id,
            Path::new("/test"),
            &[local],
            &[],
            |path| {
                if path == &PathBuf::from("deleted.txt") {
                    Some(("hash".to_string(), 1000, Some(remote_id)))
                } else {
                    None
                }
            },
        );

        assert!(plan.uploads.is_empty());
        assert!(plan.downloads.is_empty());
        assert_eq!(plan.deletes.len(), 1);

        match &plan.deletes[0] {
            SyncOp::DeleteLocal { relative_path, .. } => {
                assert_eq!(relative_path, &PathBuf::from("deleted.txt"));
            }
            _ => panic!("Expected DeleteLocal operation"),
        }
    }

    #[test]
    fn test_delete_remote_when_local_deleted() {
        let root_id = Uuid::new_v4();
        let remote_id = Uuid::new_v4();

        // File exists in DB and remote, but not local
        let remote = create_remote_file(remote_id, "deleted.txt", "hash", 1000);

        let plan = generate_plan(
            root_id,
            Path::new("/test"),
            &[],
            &[remote],
            |path| {
                if path == &PathBuf::from("deleted.txt") {
                    Some(("hash".to_string(), 1000, Some(remote_id)))
                } else {
                    None
                }
            },
        );

        assert!(plan.uploads.is_empty());
        assert!(plan.downloads.is_empty());
        assert_eq!(plan.deletes.len(), 1);

        match &plan.deletes[0] {
            SyncOp::DeleteRemote { relative_path, remote_file_id, .. } => {
                assert_eq!(relative_path, &PathBuf::from("deleted.txt"));
                assert_eq!(*remote_file_id, remote_id);
            }
            _ => panic!("Expected DeleteRemote operation"),
        }
    }

    #[test]
    fn test_in_sync_no_operation() {
        let root_id = Uuid::new_v4();
        let remote_id = Uuid::new_v4();

        // All states match
        let local = create_local_file("synced.txt", "same_hash", 1000);
        let remote = create_remote_file(remote_id, "synced.txt", "same_hash", 1000);

        let plan = generate_plan(
            root_id,
            Path::new("/test"),
            &[local],
            &[remote],
            |path| {
                if path == &PathBuf::from("synced.txt") {
                    Some(("same_hash".to_string(), 1000, Some(remote_id)))
                } else {
                    None
                }
            },
        );

        assert!(plan.is_empty());
        assert_eq!(plan.operation_count(), 0);
    }
}
