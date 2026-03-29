//! Delta processing for remote changes
//!
//! Processes delta items received from the server and applies
//! the corresponding changes to the local filesystem.

use anyhow::Result;
use std::path::PathBuf;
use tracing::{debug, error, info, trace, warn};

use crate::api::client::{ApiClient, DeltaItem};
use crate::db::{Database, FileState};
use crate::sync::engine::SyncFolder;

/// Delta processor applies remote changes to local filesystem
#[derive(Clone)]
pub struct DeltaProcessor {
    api_client: ApiClient,
    db: Database,
}

/// Result of processing a delta item
#[derive(Debug)]
pub enum DeltaResult {
    Applied,
    Skipped(String),
    Failed(String),
    Conflict(String),
}

impl DeltaProcessor {
    /// Create a new delta processor
    pub fn new(api_client: ApiClient, db: Database) -> Self {
        Self { api_client, db }
    }

    /// Process a single delta item
    pub async fn process_delta(&self, item: &DeltaItem, folder: &SyncFolder) -> Result<DeltaResult> {
        trace!("Processing delta: {:?}", item);

        let result = match item {
            DeltaItem::FileCreated { file_id, path, name, content_hash, size, .. } => {
                self.handle_file_created(*file_id, path, name, content_hash, *size, folder).await
            }
            DeltaItem::FileModified { file_id, path, name, content_hash, size, .. } => {
                self.handle_file_modified(*file_id, path, name, content_hash, *size, folder).await
            }
            DeltaItem::FileRenamed { file_id, old_path, new_path, new_name, .. } => {
                self.handle_file_renamed(*file_id, old_path, new_path, new_name, folder).await
            }
            DeltaItem::FileMoved { file_id, old_path, new_path, .. } => {
                self.handle_file_moved(*file_id, old_path, new_path, folder).await
            }
            DeltaItem::FileDeleted { file_id, path, .. } => {
                self.handle_file_deleted(*file_id, path, folder).await
            }
            DeltaItem::FileRestored { file_id, path, name, .. } => {
                // Treat restore like create
                self.handle_file_created(*file_id, path, name, "", 0, folder).await
            }
            DeltaItem::FolderCreated { folder_id, path, name, .. } => {
                self.handle_folder_created(*folder_id, path, name, folder).await
            }
            DeltaItem::FolderRenamed { folder_id, old_path, new_path, new_name, .. } => {
                self.handle_folder_renamed(*folder_id, old_path, new_path, new_name, folder).await
            }
            DeltaItem::FolderMoved { folder_id, old_path, new_path, .. } => {
                self.handle_folder_moved(*folder_id, old_path, new_path, folder).await
            }
            DeltaItem::FolderDeleted { folder_id, path, .. } => {
                self.handle_folder_deleted(*folder_id, path, folder).await
            }
            DeltaItem::FolderRestored { folder_id, path, name, .. } => {
                self.handle_folder_created(*folder_id, path, name, folder).await
            }

        };

        match &result {
            Ok(DeltaResult::Applied) => trace!("Delta applied successfully"),
            Ok(DeltaResult::Skipped(reason)) => trace!("Delta skipped: {}", reason),
            Ok(DeltaResult::Failed(reason)) => error!("Delta failed: {}", reason),
            Ok(DeltaResult::Conflict(reason)) => warn!("Delta conflict: {}", reason),
            Err(e) => error!("Delta error: {}", e),
        }

        result
    }

    /// Handle file created delta
    async fn handle_file_created(
        &self,
        file_id: uuid::Uuid,
        server_path: &str,
        name: &str,
        content_hash: &str,
        size: i64,
        folder: &SyncFolder,
    ) -> Result<DeltaResult> {
        let local_path = self.server_path_to_local(server_path, folder)?;

        // Check if file already exists locally
        if local_path.exists() {
            // Check if it's the same file by hash
            let local_hash = self.compute_file_hash(&local_path).await?;
            if local_hash == content_hash {
                trace!("File {} already exists with matching hash", name);
                return Ok(DeltaResult::Skipped("Already exists with same hash".to_string()));
            }

            // File exists with different content - check for conflict
            trace!("File {} exists with different hash, checking for conflict", name);
            // TODO: Implement conflict detection based on timestamps
        }

        // Ensure parent directory exists
        if let Some(parent) = local_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Download the file
        debug!("Downloading new file {} to {}", name, local_path.display());
        match self.api_client.download_file(file_id).await {
            Ok(content) => {
                tokio::fs::write(&local_path, content).await?;

                // Update database
                let metadata = tokio::fs::metadata(&local_path).await?;
                let modified = metadata.modified()?;

                let file_state = FileState {
                    file_id,
                    folder_id: folder.folder_id,
                    local_path: local_path.strip_prefix(&folder.local_path)?.to_path_buf(),
                    server_path: server_path.to_string(),
                    name: name.to_string(),
                    content_hash: content_hash.to_string(),
                    size,
                    local_modified_at: chrono::DateTime::from(modified),
                    server_modified_at: chrono::Utc::now(), // TODO: use actual timestamp from delta
                    version: 1,
                    is_deleted: false,
                };
                self.db.set_file_state(&file_state)?;

                info!("Downloaded file {} to {}", name, local_path.display());
                Ok(DeltaResult::Applied)
            }
            Err(e) => {
                error!("Failed to download file {}: {}", file_id, e);
                Ok(DeltaResult::Failed(e.to_string()))
            }
        }
    }

    /// Handle file modified delta
    async fn handle_file_modified(
        &self,
        file_id: uuid::Uuid,
        server_path: &str,
        name: &str,
        content_hash: &str,
        size: i64,
        folder: &SyncFolder,
    ) -> Result<DeltaResult> {
        let local_path = self.server_path_to_local(server_path, folder)?;

        // Check local state
        if let Some(local_state) = self.db.get_file_state_by_id(file_id)? {
            // Check if local file was modified since last sync
            if local_path.exists() {
                let local_hash = self.compute_file_hash(&local_path).await?;
                if local_hash != local_state.content_hash {
                    // Local file was modified - potential conflict
                    warn!(
                        "Conflict detected for file {}: local modified, remote modified",
                        name
                    );
                    // TODO: Apply conflict resolution strategy
                    return Ok(DeltaResult::Conflict("Local and remote both modified".to_string()));
                }
            }
        }

        // Get current version from database if exists
        let current_version = self.db.get_file_state_by_id(file_id)?
            .map(|s| s.version)
            .unwrap_or(0);

        // Re-download the file
        debug!("Updating file {} at {}", name, local_path.display());
        match self.api_client.download_file(file_id).await {
            Ok(content) => {
                tokio::fs::write(&local_path, content).await?;

                // Update database
                let file_state = FileState {
                    file_id,
                    folder_id: folder.folder_id,
                    local_path: local_path.strip_prefix(&folder.local_path)?.to_path_buf(),
                    server_path: server_path.to_string(),
                    name: name.to_string(),
                    content_hash: content_hash.to_string(),
                    size,
                    local_modified_at: chrono::Utc::now(),
                    server_modified_at: chrono::Utc::now(),
                    version: current_version + 1,
                    is_deleted: false,
                };
                self.db.set_file_state(&file_state)?;

                info!("Updated file {} at {}", name, local_path.display());
                Ok(DeltaResult::Applied)
            }
            Err(e) => {
                error!("Failed to download file update {}: {}", file_id, e);
                Ok(DeltaResult::Failed(e.to_string()))
            }
        }
    }

    /// Handle file renamed delta
    async fn handle_file_renamed(
        &self,
        file_id: uuid::Uuid,
        old_path: &str,
        new_path: &str,
        new_name: &str,
        folder: &SyncFolder,
    ) -> Result<DeltaResult> {
        let old_local = self.server_path_to_local(old_path, folder)?;
        let new_local = self.server_path_to_local(new_path, folder)?;

        if old_local.exists() {
            // Ensure parent directory exists
            if let Some(parent) = new_local.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }

            tokio::fs::rename(&old_local, &new_local).await?;

            // Update database
            if let Some(mut state) = self.db.get_file_state_by_id(file_id)? {
                state.local_path = new_local.strip_prefix(&folder.local_path)?.to_path_buf();
                state.server_path = new_path.to_string();
                state.name = new_name.to_string();
                self.db.set_file_state(&state)?;
            }

            info!("Renamed file {} -> {}", old_local.display(), new_local.display());
            Ok(DeltaResult::Applied)
        } else {
            warn!("Cannot rename file: old path does not exist: {}", old_local.display());
            Ok(DeltaResult::Skipped("Old path does not exist".to_string()))
        }
    }

    /// Handle file moved delta
    async fn handle_file_moved(
        &self,
        file_id: uuid::Uuid,
        old_path: &str,
        new_path: &str,
        folder: &SyncFolder,
    ) -> Result<DeltaResult> {
        // Treat move as rename (path change)
        self.handle_file_renamed(file_id, old_path, new_path, "", folder).await
    }

    /// Handle file deleted delta
    async fn handle_file_deleted(
        &self,
        file_id: uuid::Uuid,
        server_path: &str,
        folder: &SyncFolder,
    ) -> Result<DeltaResult> {
        let local_path = self.server_path_to_local(server_path, folder)?;

        if local_path.exists() {
            // Check if local file was modified since last sync
            if let Some(local_state) = self.db.get_file_state_by_id(file_id)? {
                let local_hash = self.compute_file_hash(&local_path).await?;
                if local_hash != local_state.content_hash {
                    warn!(
                        "Conflict: file {} deleted on server but modified locally",
                        local_path.display()
                    );
                    return Ok(DeltaResult::Conflict("Local modified, remote deleted".to_string()));
                }
            }

            tokio::fs::remove_file(&local_path).await?;

            // Update database
            self.db.mark_file_deleted(file_id)?;

            info!("Deleted local file {}", local_path.display());
            Ok(DeltaResult::Applied)
        } else {
            trace!("File already deleted locally: {}", local_path.display());
            Ok(DeltaResult::Skipped("Already deleted".to_string()))
        }
    }

    /// Handle folder created delta
    async fn handle_folder_created(
        &self,
        folder_id: uuid::Uuid,
        server_path: &str,
        name: &str,
        folder: &SyncFolder,
    ) -> Result<DeltaResult> {
        let local_path = self.server_path_to_local(server_path, folder)?;

        if !local_path.exists() {
            tokio::fs::create_dir_all(&local_path).await?;
            info!("Created local folder {} at {}", name, local_path.display());
        }

        Ok(DeltaResult::Applied)
    }

    /// Handle folder renamed delta
    async fn handle_folder_renamed(
        &self,
        folder_id: uuid::Uuid,
        old_path: &str,
        new_path: &str,
        new_name: &str,
        folder: &SyncFolder,
    ) -> Result<DeltaResult> {
        let old_local = self.server_path_to_local(old_path, folder)?;
        let new_local = self.server_path_to_local(new_path, folder)?;

        if old_local.exists() && !new_local.exists() {
            tokio::fs::rename(&old_local, &new_local).await?;
            info!(
                "Renamed folder {} -> {}",
                old_local.display(),
                new_local.display()
            );
            Ok(DeltaResult::Applied)
        } else {
            Ok(DeltaResult::Skipped("Folder state inconsistent".to_string()))
        }
    }

    /// Handle folder moved delta
    async fn handle_folder_moved(
        &self,
        folder_id: uuid::Uuid,
        old_path: &str,
        new_path: &str,
        folder: &SyncFolder,
    ) -> Result<DeltaResult> {
        self.handle_folder_renamed(folder_id, old_path, new_path, "", folder).await
    }

    /// Handle folder deleted delta
    async fn handle_folder_deleted(
        &self,
        folder_id: uuid::Uuid,
        server_path: &str,
        folder: &SyncFolder,
    ) -> Result<DeltaResult> {
        let local_path = self.server_path_to_local(server_path, folder)?;

        if local_path.exists() {
            // Only delete if empty to be safe
            let entries: Vec<_> = std::fs::read_dir(&local_path)?.collect();
            if entries.is_empty() {
                tokio::fs::remove_dir(&local_path).await?;
                info!("Deleted local folder {}", local_path.display());
                Ok(DeltaResult::Applied)
            } else {
                warn!("Not deleting non-empty folder: {}", local_path.display());
                Ok(DeltaResult::Skipped("Folder not empty".to_string()))
            }
        } else {
            Ok(DeltaResult::Skipped("Already deleted".to_string()))
        }
    }

    /// Convert server path to local path
    fn server_path_to_local(&self, server_path: &str, folder: &SyncFolder) -> Result<PathBuf> {
        // Server path format: "/folder/path/file.txt"
        // Convert to local path: "{sync_root}/path/file.txt"
        
        let relative = server_path.strip_prefix('/').unwrap_or(server_path);
        let local = folder.local_path.join(relative);
        
        // Security check: ensure the path is within the sync root
        let canonical_local = local.canonicalize().unwrap_or(local.clone());
        let canonical_root = folder.local_path.canonicalize()?;
        
        if !canonical_local.starts_with(&canonical_root) {
            anyhow::bail!("Path traversal detected: {}", server_path);
        }
        
        Ok(local)
    }

    /// Compute SHA-256 hash of a file
    async fn compute_file_hash(&self, path: &PathBuf) -> Result<String> {
        use sha2::Digest;
        use tokio::io::AsyncReadExt;

        let mut file = tokio::fs::File::open(path).await?;
        let mut hasher = sha2::Sha256::new();
        let mut buffer = [0u8; 8192];

        loop {
            let bytes_read = file.read(&mut buffer).await?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        Ok(hex::encode(hasher.finalize()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests would require mocking the API client and filesystem
    // For now, we just verify the module compiles

    #[test]
    fn test_delta_result_variants() {
        let applied = DeltaResult::Applied;
        let skipped = DeltaResult::Skipped("test".to_string());
        let failed = DeltaResult::Failed("test".to_string());
        let conflict = DeltaResult::Conflict("test".to_string());

        assert!(matches!(applied, DeltaResult::Applied));
        assert!(matches!(skipped, DeltaResult::Skipped(_)));
        assert!(matches!(failed, DeltaResult::Failed(_)));
        assert!(matches!(conflict, DeltaResult::Conflict(_)));
    }
}
