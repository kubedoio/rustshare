//! Conflict resolution for sync operations
//!
//! Handles conflicts that occur when both local and remote versions
//! of a file have been modified since the last sync.

use chrono::{DateTime, Utc};
use std::path::PathBuf;
use tracing::info;
use uuid::Uuid;

/// Conflict resolution strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictResolution {
    /// Server version wins (discard client changes)
    ServerWins,
    /// Client version wins (overwrite server)
    ClientWins,
    /// Last write wins based on timestamp
    LastWriteWins,
    /// Rename client version (keep both)
    Rename,
}

/// Conflict information for a sync operation
#[derive(Debug, Clone)]
pub struct ConflictInfo {
    /// The resource ID in conflict
    pub resource_id: Uuid,
    /// Type of resource ("file" or "folder")
    pub resource_type: String,
    /// Local path of the resource
    pub local_path: PathBuf,
    /// Server version timestamp
    pub server_timestamp: DateTime<Utc>,
    /// Client version timestamp
    pub client_timestamp: DateTime<Utc>,
    /// Server version number (for optimistic concurrency)
    pub server_version: u64,
    /// Client version number
    pub client_version: u64,
    /// Server content hash
    pub server_hash: String,
    /// Client content hash
    pub client_hash: String,
}

/// Result of conflict resolution
#[derive(Debug, Clone)]
pub enum ConflictResolutionResult {
    /// Use the server version
    UseServer,
    /// Use the client version
    UseClient,
    /// Rename the client version and use both
    UseBoth { server_path: PathBuf, client_path: PathBuf },
}

/// Conflict resolver
#[derive(Clone)]
pub struct ConflictResolver {
    strategy: ConflictResolution,
}

impl ConflictResolver {
    /// Create a new conflict resolver with the given strategy
    pub fn new(strategy: ConflictResolution) -> Self {
        Self { strategy }
    }

    /// Resolve a conflict
    pub fn resolve(&self, conflict: &ConflictInfo) -> ConflictResolutionResult {
        info!(
            "Resolving conflict for {} using strategy {:?}",
            conflict.local_path.display(),
            self.strategy
        );

        match self.strategy {
            ConflictResolution::ServerWins => ConflictResolutionResult::UseServer,
            ConflictResolution::ClientWins => ConflictResolutionResult::UseClient,
            ConflictResolution::LastWriteWins => self.resolve_last_write_wins(conflict),
            ConflictResolution::Rename => self.resolve_rename(conflict),
        }
    }

    /// Resolve using last-write-wins strategy
    ///
    /// If timestamps are equal, server wins (deterministic tiebreaker).
    fn resolve_last_write_wins(&self, conflict: &ConflictInfo) -> ConflictResolutionResult {
        if conflict.client_timestamp > conflict.server_timestamp {
            info!(
                "Client version is newer ({} > {}), using client version",
                conflict.client_timestamp, conflict.server_timestamp
            );
            ConflictResolutionResult::UseClient
        } else {
            info!(
                "Server version is newer or equal ({} <= {}), using server version",
                conflict.client_timestamp, conflict.server_timestamp
            );
            ConflictResolutionResult::UseServer
        }
    }

    /// Resolve using rename strategy
    fn resolve_rename(&self, conflict: &ConflictInfo) -> ConflictResolutionResult {
        // Generate a new name for the client version
        let client_path = self.generate_conflict_name(&conflict.local_path);
        
        info!(
            "Renaming client version to {} and keeping both",
            client_path.display()
        );
        
        ConflictResolutionResult::UseBoth {
            server_path: conflict.local_path.clone(),
            client_path,
        }
    }

    /// Generate a conflict filename for the client version
    fn generate_conflict_name(&self, original_path: &PathBuf) -> PathBuf {
        let binding = PathBuf::from(".");
        let parent = original_path.parent().unwrap_or(binding.as_path());
        let stem = original_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file");
        let extension = original_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        
        let new_name = if extension.is_empty() {
            format!("{} (conflict {})", stem, timestamp)
        } else {
            format!("{} (conflict {}).{}", stem, timestamp, extension)
        };
        
        parent.join(new_name)
    }

    /// Check if two versions are in conflict
    ///
    /// A conflict exists if both the local and remote versions have
    /// been modified since the last sync (indicated by different hashes).
    pub fn is_conflict(
        &self,
        local_hash: &str,
        remote_hash: &str,
        last_sync_hash: Option<&str>,
    ) -> bool {
        // If both match last sync, no changes
        if Some(local_hash) == last_sync_hash && Some(remote_hash) == last_sync_hash {
            return false;
        }
        
        // If both are the same now, no conflict (same content)
        if local_hash == remote_hash {
            return false;
        }
        
        // If only one changed, no conflict
        if local_hash == last_sync_hash.unwrap_or("") {
            return false; // Only remote changed
        }
        if remote_hash == last_sync_hash.unwrap_or("") {
            return false; // Only local changed
        }
        
        // Both changed and different - conflict
        true
    }
}

/// Conflict detection helper
pub struct ConflictDetector;

impl ConflictDetector {
    /// Detect conflicts between local and remote states
    pub fn detect_conflicts(
        local_files: &[LocalFileInfo],
        remote_files: &[RemoteFileInfo],
        last_sync_state: &std::collections::HashMap<Uuid, String>, // file_id -> hash
    ) -> Vec<ConflictInfo> {
        let mut conflicts = Vec::new();
        
        let local_by_id: std::collections::HashMap<Uuid, &LocalFileInfo> = local_files
            .iter()
            .filter_map(|f| f.file_id.map(|id| (id, f)))
            .collect();
        
        let remote_by_id: std::collections::HashMap<Uuid, &RemoteFileInfo> = remote_files
            .iter()
            .map(|f| (f.file_id, f))
            .collect();
        
        // Check all files in both sets
        let all_ids: std::collections::HashSet<Uuid> = local_by_id
            .keys()
            .chain(remote_by_id.keys())
            .copied()
            .collect();
        
        for file_id in all_ids {
            let local = local_by_id.get(&file_id);
            let remote = remote_by_id.get(&file_id);
            let last_hash = last_sync_state.get(&file_id).map(|s| s.as_str());
            
            let local_hash = local.map(|l| l.content_hash.as_str()).unwrap_or("");
            let remote_hash = remote.map(|r| r.content_hash.as_str()).unwrap_or("");
            
            // Check if both exist and are different
            if local.is_some() && remote.is_some() && local_hash != remote_hash {
                // Both exist with different content - check if conflict
                if last_hash.map(|h| h != local_hash && h != remote_hash).unwrap_or(true) {
                    // Both changed since last sync - conflict
                    conflicts.push(ConflictInfo {
                        resource_id: file_id,
                        resource_type: "file".to_string(),
                        local_path: local.unwrap().path.clone(),
                        server_timestamp: remote.unwrap().modified_at,
                        client_timestamp: local.unwrap().modified_at,
                        server_version: remote.unwrap().version as u64,
                        client_version: local.unwrap().version as u64,
                        server_hash: remote_hash.to_string(),
                        client_hash: local_hash.to_string(),
                    });
                }
            }
        }
        
        conflicts
    }
}

/// Local file info for conflict detection
#[derive(Debug)]
pub struct LocalFileInfo {
    pub file_id: Option<Uuid>,
    pub path: PathBuf,
    pub content_hash: String,
    pub modified_at: DateTime<Utc>,
    pub version: i32,
}

/// Remote file info for conflict detection
#[derive(Debug)]
pub struct RemoteFileInfo {
    pub file_id: Uuid,
    pub content_hash: String,
    pub modified_at: DateTime<Utc>,
    pub version: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conflict_detection() {
        let resolver = ConflictResolver::new(ConflictResolution::LastWriteWins);
        
        // No conflict - both same as last sync
        assert!(!resolver.is_conflict("hash1", "hash1", Some("hash1")));
        
        // No conflict - only local changed
        assert!(!resolver.is_conflict("hash2", "hash1", Some("hash1")));
        
        // No conflict - only remote changed
        assert!(!resolver.is_conflict("hash1", "hash2", Some("hash1")));
        
        // No conflict - both changed to same hash
        assert!(!resolver.is_conflict("hash2", "hash2", Some("hash1")));
        
        // Conflict - both changed to different hashes
        assert!(resolver.is_conflict("hash2", "hash3", Some("hash1")));
        
        // Conflict - no previous sync state
        assert!(resolver.is_conflict("hash1", "hash2", None));
    }

    #[test]
    fn test_last_write_wins_resolution() {
        let resolver = ConflictResolver::new(ConflictResolution::LastWriteWins);
        
        let now = Utc::now();
        let earlier = now - chrono::Duration::minutes(5);
        
        // Client wins (newer)
        let conflict = ConflictInfo {
            resource_id: Uuid::new_v4(),
            resource_type: "file".to_string(),
            local_path: "test.txt".into(),
            server_timestamp: earlier,
            client_timestamp: now,
            server_version: 1,
            client_version: 2,
            server_hash: "hash1".to_string(),
            client_hash: "hash2".to_string(),
        };
        
        assert!(matches!(resolver.resolve(&conflict), ConflictResolutionResult::UseClient));
        
        // Server wins (newer)
        let conflict = ConflictInfo {
            resource_id: Uuid::new_v4(),
            resource_type: "file".to_string(),
            local_path: "test.txt".into(),
            server_timestamp: now,
            client_timestamp: earlier,
            server_version: 2,
            client_version: 1,
            server_hash: "hash2".to_string(),
            client_hash: "hash1".to_string(),
        };
        
        assert!(matches!(resolver.resolve(&conflict), ConflictResolutionResult::UseServer));
    }

    #[test]
    fn test_rename_resolution() {
        let resolver = ConflictResolver::new(ConflictResolution::Rename);
        
        let conflict = ConflictInfo {
            resource_id: Uuid::new_v4(),
            resource_type: "file".to_string(),
            local_path: "test.txt".into(),
            server_timestamp: Utc::now(),
            client_timestamp: Utc::now(),
            server_version: 2,
            client_version: 2,
            server_hash: "hash2".to_string(),
            client_hash: "hash3".to_string(),
        };
        
        let result = resolver.resolve(&conflict);
        
        match result {
            ConflictResolutionResult::UseBoth { server_path, client_path } => {
                assert_eq!(server_path, PathBuf::from("test.txt"));
                assert!(client_path.to_string_lossy().contains("conflict"));
            }
            _ => panic!("Expected UseBoth result"),
        }
    }

    #[test]
    fn test_generate_conflict_name() {
        let resolver = ConflictResolver::new(ConflictResolution::Rename);
        
        let original: PathBuf = "folder/test.txt".into();
        let conflict_name = resolver.generate_conflict_name(&original);
        
        let name = conflict_name.to_string_lossy();
        assert!(name.contains("conflict"));
        assert!(name.ends_with(".txt"));
    }
}
