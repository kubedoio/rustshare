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

/// Information about a folder on the remote server
#[derive(Debug, Clone)]
pub struct RemoteFolderInfo {
    pub id: Uuid,
    pub relative_path: PathBuf,
    pub modified_at: u64,
}

/// An operation to be executed during sync
#[derive(Debug, Clone)]
pub enum SyncOp {
    /// Create a local directory
    CreateLocalDir {
        root_id: Uuid,
        relative_path: PathBuf,
    },
    /// Create a remote directory
    CreateRemoteDir {
        root_id: Uuid,
        relative_path: PathBuf,
    },
    /// Delete a local directory
    DeleteLocalDir {
        root_id: Uuid,
        relative_path: PathBuf,
    },
    /// Delete a remote directory
    DeleteRemoteDir {
        root_id: Uuid,
        relative_path: PathBuf,
        remote_folder_id: Uuid,
    },
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
    pub create_local_dirs: Vec<SyncOp>,
    pub create_remote_dirs: Vec<SyncOp>,
    pub uploads: Vec<SyncOp>,
    pub downloads: Vec<SyncOp>,
    pub deletes: Vec<SyncOp>,
    pub delete_local_dirs: Vec<SyncOp>,
    pub delete_remote_dirs: Vec<SyncOp>,
    pub conflicts: Vec<Conflict>,
}

impl SyncPlan {
    /// Returns true if the plan has no operations or conflicts
    pub fn is_empty(&self) -> bool {
        self.create_local_dirs.is_empty()
            && self.create_remote_dirs.is_empty()
            && self.uploads.is_empty()
            && self.downloads.is_empty()
            && self.deletes.is_empty()
            && self.delete_local_dirs.is_empty()
            && self.delete_remote_dirs.is_empty()
            && self.conflicts.is_empty()
    }

    /// Returns the total number of operations (excluding conflicts)
    pub fn operation_count(&self) -> usize {
        self.create_local_dirs.len()
            + self.create_remote_dirs.len()
            + self.uploads.len()
            + self.downloads.len()
            + self.deletes.len()
            + self.delete_local_dirs.len()
            + self.delete_remote_dirs.len()
    }

    /// Returns the total number of conflicts
    pub fn conflict_count(&self) -> usize {
        self.conflicts.len()
    }
}

/// Database state for a single file
#[derive(Debug, Clone)]
pub(crate) struct DbFileState {
    /// Local content hash (SHA-256 when available).
    pub(crate) local_hash: String,
    /// Remote content hash or version token used for change detection.
    pub(crate) remote_hash: String,
    pub(crate) modified_at: u64,
    pub(crate) _remote_id: Option<Uuid>,
    pub(crate) is_directory: bool,
    pub(crate) sync_status: String,
    pub(crate) tombstone_side: Option<String>,
    pub(crate) tombstone_at: Option<u64>,
}

impl DbFileState {
    fn synced(hash: String, modified_at: u64, remote_id: Option<Uuid>) -> Self {
        Self {
            local_hash: hash.clone(),
            remote_hash: hash,
            modified_at,
            _remote_id: remote_id,
            is_directory: false,
            sync_status: "synced".to_string(),
            tombstone_side: None,
            tombstone_at: None,
        }
    }

    fn is_tombstone(&self) -> bool {
        self.sync_status == "tombstone"
    }
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
    generate_plan_with_db_files(
        root_id,
        _root_path,
        local_files,
        remote_files,
        &[],
        &[],
        |path| {
            db_lookup(path).map(|(hash, modified_at, remote_id)| {
                DbFileState::synced(hash, modified_at, remote_id)
            })
        },
    )
}

/// Extended version that also handles DB-only files (files that were tracked but may have been deleted)
///
/// This version requires the full list of paths from the database to properly detect deletions.
pub(crate) fn generate_plan_with_db_files<F>(
    root_id: Uuid,
    _root_path: &std::path::Path,
    local_files: &[FileScanResult],
    remote_files: &[RemoteFileInfo],
    remote_dirs: &[RemoteFolderInfo],
    db_paths: &[PathBuf],
    mut db_lookup: F,
) -> SyncPlan
where
    F: FnMut(&PathBuf) -> Option<DbFileState>,
{
    let mut plan = SyncPlan::default();

    // Build lookup maps
    let local_dir_map: HashMap<&PathBuf, &FileScanResult> = local_files
        .iter()
        .filter(|f| f.is_directory)
        .map(|f| (&f.relative_path, f))
        .collect();

    let remote_dir_map: HashMap<&PathBuf, &RemoteFolderInfo> =
        remote_dirs.iter().map(|f| (&f.relative_path, f)).collect();

    let local_map: HashMap<&PathBuf, &FileScanResult> = local_files
        .iter()
        .filter(|f| !f.is_directory)
        .map(|f| (&f.relative_path, f))
        .collect();

    let remote_map: HashMap<&PathBuf, &RemoteFileInfo> =
        remote_files.iter().map(|f| (&f.relative_path, f)).collect();

    let db_set: std::collections::HashSet<&PathBuf> = db_paths.iter().collect();

    let db_dir_set: std::collections::HashSet<&PathBuf> = db_paths
        .iter()
        .filter(|path| {
            db_lookup(path)
                .as_ref()
                .map(|state| state.is_directory)
                .unwrap_or(false)
        })
        .collect();

    let mut all_dir_paths: std::collections::HashSet<&PathBuf> = std::collections::HashSet::new();
    all_dir_paths.extend(local_dir_map.keys());
    all_dir_paths.extend(remote_dir_map.keys());
    all_dir_paths.extend(db_dir_set.iter().copied());

    for relative_path in all_dir_paths {
        let local_dir = local_dir_map.get(relative_path).copied();
        let remote_dir = remote_dir_map.get(relative_path).copied();
        let db_dir = if db_dir_set.contains(relative_path) {
            db_lookup(relative_path)
        } else {
            None
        };

        if let Some(db_state) = db_dir.as_ref().filter(|state| state.is_tombstone()) {
            match (local_dir, remote_dir) {
                (Some(_), None) => {
                    plan.create_remote_dirs.push(SyncOp::CreateRemoteDir {
                        root_id,
                        relative_path: relative_path.clone(),
                    });
                }
                (None, Some(_)) => {
                    plan.create_local_dirs.push(SyncOp::CreateLocalDir {
                        root_id,
                        relative_path: relative_path.clone(),
                    });
                }
                (None, None) => {
                    let _ = (&db_state.tombstone_side, &db_state.tombstone_at);
                }
                (Some(_), Some(_)) => {}
            }
            continue;
        }

        match (local_dir, remote_dir, db_dir) {
            (Some(_), None, Some(_)) => {
                plan.delete_local_dirs.push(SyncOp::DeleteLocalDir {
                    root_id,
                    relative_path: relative_path.clone(),
                });
            }
            (None, Some(remote), Some(_)) => {
                plan.delete_remote_dirs.push(SyncOp::DeleteRemoteDir {
                    root_id,
                    relative_path: relative_path.clone(),
                    remote_folder_id: remote.id,
                });
            }
            (Some(_), None, None) => {
                plan.create_remote_dirs.push(SyncOp::CreateRemoteDir {
                    root_id,
                    relative_path: relative_path.clone(),
                });
            }
            (None, Some(_), None) => {
                plan.create_local_dirs.push(SyncOp::CreateLocalDir {
                    root_id,
                    relative_path: relative_path.clone(),
                });
            }
            (Some(_), Some(_), _) | (None, None, _) => {}
        }
    }

    // First pass: process all paths from local and remote
    let mut all_paths: std::collections::HashSet<&PathBuf> = std::collections::HashSet::new();
    all_paths.extend(local_map.keys());
    all_paths.extend(remote_map.keys());
    all_paths.extend(&db_set);

    for relative_path in all_paths {
        let local = local_map.get(relative_path).copied();
        let remote = remote_map.get(relative_path).copied();
        let db = db_lookup(relative_path);

        if let Some(db_state) = db.as_ref().filter(|state| state.is_tombstone()) {
            match (local, remote) {
                (Some(local), None) => {
                    plan.uploads.push(SyncOp::Upload {
                        root_id,
                        relative_path: relative_path.clone(),
                        local_path: local.absolute_path.clone(),
                        size: local.size,
                    });
                }
                (None, Some(remote)) => {
                    plan.downloads.push(SyncOp::Download {
                        root_id,
                        relative_path: relative_path.clone(),
                        remote_file_id: remote.id,
                        remote_hash: remote.hash.clone(),
                        size: remote.size,
                    });
                }
                (Some(local), Some(remote)) => {
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
                (None, None) => {
                    let _ = (&db_state.tombstone_side, &db_state.tombstone_at);
                }
            }
            continue;
        }

        match (local, remote, db) {
            (Some(local), Some(remote), Some(db)) => {
                let local_changed =
                    local.hash != db.local_hash || local.modified_at != db.modified_at;
                let remote_changed =
                    remote.hash != db.remote_hash || remote.modified_at != db.modified_at;

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

    sort_directory_ops(&mut plan.create_local_dirs);
    sort_directory_ops(&mut plan.create_remote_dirs);
    sort_directory_ops_desc(&mut plan.delete_local_dirs);
    sort_directory_ops_desc(&mut plan.delete_remote_dirs);

    plan
}

fn sort_directory_ops(ops: &mut [SyncOp]) {
    ops.sort_by_key(op_depth);
}

fn sort_directory_ops_desc(ops: &mut [SyncOp]) {
    ops.sort_by_key(|op| std::cmp::Reverse(op_depth(op)));
}

fn op_depth(op: &SyncOp) -> usize {
    match op {
        SyncOp::CreateLocalDir { relative_path, .. }
        | SyncOp::CreateRemoteDir { relative_path, .. }
        | SyncOp::DeleteLocalDir { relative_path, .. }
        | SyncOp::DeleteRemoteDir { relative_path, .. }
        | SyncOp::Upload { relative_path, .. }
        | SyncOp::Download { relative_path, .. }
        | SyncOp::DeleteLocal { relative_path, .. }
        | SyncOp::DeleteRemote { relative_path, .. } => relative_path.components().count(),
    }
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

    fn create_local_directory(path: &str, modified_at: u64) -> FileScanResult {
        FileScanResult {
            relative_path: PathBuf::from(path),
            absolute_path: PathBuf::from("/test").join(path),
            hash: String::new(),
            size: 0,
            modified_at,
            is_directory: true,
        }
    }

    fn create_remote_file(id: Uuid, path: &str, hash: &str, modified_at: u64) -> RemoteFileInfo {
        RemoteFileInfo {
            id,
            relative_path: PathBuf::from(path),
            hash: hash.to_string(),
            size: 100,
            modified_at,
        }
    }

    fn create_remote_directory(id: Uuid, path: &str, modified_at: u64) -> RemoteFolderInfo {
        RemoteFolderInfo {
            id,
            relative_path: PathBuf::from(path),
            modified_at,
        }
    }

    #[test]
    fn test_empty_plan() {
        let root_id = Uuid::new_v4();
        let plan = generate_plan(root_id, Path::new("/test"), &[], &[], |_path| None);

        assert!(plan.is_empty());
        assert_eq!(plan.operation_count(), 0);
        assert_eq!(plan.conflict_count(), 0);
    }

    #[test]
    fn test_upload_new_local_file() {
        let root_id = Uuid::new_v4();
        let local = create_local_file("new.txt", "hash1", 1000);

        let plan = generate_plan(root_id, Path::new("/test"), &[local], &[], |_path| None);

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

        let plan = generate_plan(root_id, Path::new("/test"), &[], &[remote], |_path| None);

        assert_eq!(plan.uploads.len(), 0);
        assert_eq!(plan.downloads.len(), 1);
        assert_eq!(plan.deletes.len(), 0);
        assert!(plan.conflicts.is_empty());

        match &plan.downloads[0] {
            SyncOp::Download {
                relative_path,
                remote_file_id,
                ..
            } => {
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

        let plan = generate_plan(root_id, Path::new("/test"), &[local], &[remote], |path| {
            if path == &PathBuf::from("modified.txt") {
                Some(("old_hash".to_string(), 1000, Some(remote_id)))
            } else {
                None
            }
        });

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

        let plan = generate_plan(root_id, Path::new("/test"), &[local], &[remote], |path| {
            if path == &PathBuf::from("modified.txt") {
                Some(("old_hash".to_string(), 1000, Some(remote_id)))
            } else {
                None
            }
        });

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

        let plan = generate_plan(root_id, Path::new("/test"), &[local], &[remote], |path| {
            if path == &PathBuf::from("conflict.txt") {
                Some(("old_hash".to_string(), 1000, Some(remote_id)))
            } else {
                None
            }
        });

        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(
            plan.conflicts[0].resolution,
            ConflictResolution::UploadLocal
        );
        assert_eq!(plan.uploads.len(), 1); // Resolved as upload
    }

    #[test]
    fn test_conflict_remote_newer() {
        let root_id = Uuid::new_v4();
        let remote_id = Uuid::new_v4();

        // Both changed, remote is newer
        let local = create_local_file("conflict.txt", "local_hash", 1500);
        let remote = create_remote_file(remote_id, "conflict.txt", "remote_hash", 2000);

        let plan = generate_plan(root_id, Path::new("/test"), &[local], &[remote], |path| {
            if path == &PathBuf::from("conflict.txt") {
                Some(("old_hash".to_string(), 1000, Some(remote_id)))
            } else {
                None
            }
        });

        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(
            plan.conflicts[0].resolution,
            ConflictResolution::DownloadRemote
        );
        assert_eq!(plan.downloads.len(), 1); // Resolved as download
    }

    #[test]
    fn test_delete_local_when_remote_deleted() {
        let root_id = Uuid::new_v4();
        let remote_id = Uuid::new_v4();

        // File exists in DB and local, but not remote
        let local = create_local_file("deleted.txt", "hash", 1000);

        let plan = generate_plan(root_id, Path::new("/test"), &[local], &[], |path| {
            if path == &PathBuf::from("deleted.txt") {
                Some(("hash".to_string(), 1000, Some(remote_id)))
            } else {
                None
            }
        });

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

        let plan = generate_plan(root_id, Path::new("/test"), &[], &[remote], |path| {
            if path == &PathBuf::from("deleted.txt") {
                Some(("hash".to_string(), 1000, Some(remote_id)))
            } else {
                None
            }
        });

        assert!(plan.uploads.is_empty());
        assert!(plan.downloads.is_empty());
        assert_eq!(plan.deletes.len(), 1);

        match &plan.deletes[0] {
            SyncOp::DeleteRemote {
                relative_path,
                remote_file_id,
                ..
            } => {
                assert_eq!(relative_path, &PathBuf::from("deleted.txt"));
                assert_eq!(*remote_file_id, remote_id);
            }
            _ => panic!("Expected DeleteRemote operation"),
        }
    }

    #[test]
    fn test_tombstone_keeps_deleted_path_deleted_when_both_sides_absent() {
        let root_id = Uuid::new_v4();

        let plan = generate_plan_with_db_files(
            root_id,
            Path::new("/test"),
            &[],
            &[],
            &[],
            &[PathBuf::from("deleted.txt")],
            |path| {
                if path == &PathBuf::from("deleted.txt") {
                    Some(DbFileState {
                        local_hash: "hash".to_string(),
                        remote_hash: "hash".to_string(),
                        modified_at: 1000,
                        _remote_id: None,
                        is_directory: false,
                        sync_status: "tombstone".to_string(),
                        tombstone_side: Some("local".to_string()),
                        tombstone_at: Some(1200),
                    })
                } else {
                    None
                }
            },
        );

        assert!(plan.is_empty());
    }

    #[test]
    fn test_local_recreation_after_tombstone_uploads_instead_of_deleting_local() {
        let root_id = Uuid::new_v4();
        let local = create_local_file("deleted.txt", "new_hash", 2000);

        let plan = generate_plan_with_db_files(
            root_id,
            Path::new("/test"),
            &[local],
            &[],
            &[],
            &[PathBuf::from("deleted.txt")],
            |path| {
                if path == &PathBuf::from("deleted.txt") {
                    Some(DbFileState {
                        local_hash: "old_hash".to_string(),
                        remote_hash: "old_hash".to_string(),
                        modified_at: 1000,
                        _remote_id: None,
                        is_directory: false,
                        sync_status: "tombstone".to_string(),
                        tombstone_side: Some("local".to_string()),
                        tombstone_at: Some(1200),
                    })
                } else {
                    None
                }
            },
        );

        assert_eq!(plan.uploads.len(), 1);
        assert!(plan.deletes.is_empty());
    }

    #[test]
    fn test_remote_recreation_after_tombstone_downloads_instead_of_deleting_remote() {
        let root_id = Uuid::new_v4();
        let remote_id = Uuid::new_v4();
        let remote = create_remote_file(remote_id, "deleted.txt", "new_hash", 2000);

        let plan = generate_plan_with_db_files(
            root_id,
            Path::new("/test"),
            &[],
            &[remote],
            &[],
            &[PathBuf::from("deleted.txt")],
            |path| {
                if path == &PathBuf::from("deleted.txt") {
                    Some(DbFileState {
                        local_hash: "old_hash".to_string(),
                        remote_hash: "old_hash".to_string(),
                        modified_at: 1000,
                        _remote_id: Some(remote_id),
                        is_directory: false,
                        sync_status: "tombstone".to_string(),
                        tombstone_side: Some("remote".to_string()),
                        tombstone_at: Some(1200),
                    })
                } else {
                    None
                }
            },
        );

        assert_eq!(plan.downloads.len(), 1);
        assert!(plan.deletes.is_empty());
    }

    #[test]
    fn test_in_sync_no_operation() {
        let root_id = Uuid::new_v4();
        let remote_id = Uuid::new_v4();

        // All states match
        let local = create_local_file("synced.txt", "same_hash", 1000);
        let remote = create_remote_file(remote_id, "synced.txt", "same_hash", 1000);

        let plan = generate_plan(root_id, Path::new("/test"), &[local], &[remote], |path| {
            if path == &PathBuf::from("synced.txt") {
                Some(("same_hash".to_string(), 1000, Some(remote_id)))
            } else {
                None
            }
        });

        assert!(plan.is_empty());
        assert_eq!(plan.operation_count(), 0);
    }

    #[test]
    fn test_create_remote_directories_for_empty_local_folders() {
        let root_id = Uuid::new_v4();
        let parent = create_local_directory("docs", 1000);
        let child = create_local_directory("docs/specs", 1001);

        let plan = generate_plan_with_db_files(
            root_id,
            Path::new("/test"),
            &[parent, child],
            &[],
            &[],
            &[],
            |_path| None,
        );

        assert_eq!(plan.create_remote_dirs.len(), 2);

        match &plan.create_remote_dirs[0] {
            SyncOp::CreateRemoteDir { relative_path, .. } => {
                assert_eq!(relative_path, &PathBuf::from("docs"));
            }
            _ => panic!("Expected CreateRemoteDir operation"),
        }

        match &plan.create_remote_dirs[1] {
            SyncOp::CreateRemoteDir { relative_path, .. } => {
                assert_eq!(relative_path, &PathBuf::from("docs/specs"));
            }
            _ => panic!("Expected CreateRemoteDir operation"),
        }
    }

    #[test]
    fn test_create_local_directories_for_empty_remote_folders() {
        let root_id = Uuid::new_v4();
        let parent_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();

        let plan = generate_plan_with_db_files(
            root_id,
            Path::new("/test"),
            &[],
            &[],
            &[
                create_remote_directory(parent_id, "assets", 1000),
                create_remote_directory(child_id, "assets/icons", 1001),
            ],
            &[],
            |_path| None,
        );

        assert_eq!(plan.create_local_dirs.len(), 2);

        match &plan.create_local_dirs[0] {
            SyncOp::CreateLocalDir { relative_path, .. } => {
                assert_eq!(relative_path, &PathBuf::from("assets"));
            }
            _ => panic!("Expected CreateLocalDir operation"),
        }

        match &plan.create_local_dirs[1] {
            SyncOp::CreateLocalDir { relative_path, .. } => {
                assert_eq!(relative_path, &PathBuf::from("assets/icons"));
            }
            _ => panic!("Expected CreateLocalDir operation"),
        }
    }

    #[test]
    fn test_delete_local_directory_when_remote_deleted_and_dir_was_synced() {
        let root_id = Uuid::new_v4();
        let local_dir = create_local_directory("docs", 1000);

        let plan = generate_plan_with_db_files(
            root_id,
            Path::new("/test"),
            &[local_dir],
            &[],
            &[],
            &[PathBuf::from("docs")],
            |path| {
                if path == &PathBuf::from("docs") {
                    Some(DbFileState {
                        local_hash: String::new(),
                        remote_hash: String::new(),
                        modified_at: 1000,
                        _remote_id: Some(Uuid::new_v4()),
                        is_directory: true,
                        sync_status: "synced".to_string(),
                        tombstone_side: None,
                        tombstone_at: None,
                    })
                } else {
                    None
                }
            },
        );

        assert!(plan.create_remote_dirs.is_empty());
        assert_eq!(plan.delete_local_dirs.len(), 1);

        match &plan.delete_local_dirs[0] {
            SyncOp::DeleteLocalDir { relative_path, .. } => {
                assert_eq!(relative_path, &PathBuf::from("docs"));
            }
            _ => panic!("Expected DeleteLocalDir operation"),
        }
    }

    #[test]
    fn test_delete_remote_directory_when_local_deleted_and_dir_was_synced() {
        let root_id = Uuid::new_v4();
        let remote_dir_id = Uuid::new_v4();

        let plan = generate_plan_with_db_files(
            root_id,
            Path::new("/test"),
            &[],
            &[],
            &[create_remote_directory(remote_dir_id, "assets", 1000)],
            &[PathBuf::from("assets")],
            |path| {
                if path == &PathBuf::from("assets") {
                    Some(DbFileState {
                        local_hash: String::new(),
                        remote_hash: String::new(),
                        modified_at: 1000,
                        _remote_id: Some(remote_dir_id),
                        is_directory: true,
                        sync_status: "synced".to_string(),
                        tombstone_side: None,
                        tombstone_at: None,
                    })
                } else {
                    None
                }
            },
        );

        assert!(plan.create_local_dirs.is_empty());
        assert_eq!(plan.delete_remote_dirs.len(), 1);

        match &plan.delete_remote_dirs[0] {
            SyncOp::DeleteRemoteDir {
                relative_path,
                remote_folder_id,
                ..
            } => {
                assert_eq!(relative_path, &PathBuf::from("assets"));
                assert_eq!(*remote_folder_id, remote_dir_id);
            }
            _ => panic!("Expected DeleteRemoteDir operation"),
        }
    }

    #[test]
    fn test_no_op_when_local_hash_matches_and_remote_version_matches() {
        let root_id = Uuid::new_v4();
        let remote_id = Uuid::new_v4();

        let local = create_local_file("synced.txt", "sha256_local", 1000);
        let remote = create_remote_file(remote_id, "synced.txt", "2", 1000);

        let plan = generate_plan_with_db_files(
            root_id,
            Path::new("/test"),
            &[local],
            &[remote],
            &[],
            &[PathBuf::from("synced.txt")],
            |path| {
                if path == &PathBuf::from("synced.txt") {
                    Some(DbFileState {
                        local_hash: "sha256_local".to_string(),
                        remote_hash: "2".to_string(),
                        modified_at: 1000,
                        _remote_id: Some(remote_id),
                        is_directory: false,
                        sync_status: "synced".to_string(),
                        tombstone_side: None,
                        tombstone_at: None,
                    })
                } else {
                    None
                }
            },
        );

        assert!(plan.is_empty());
    }
}
