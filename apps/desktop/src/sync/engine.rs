//! Main sync engine
//!
//! The SyncEngine is the central coordinator that:
//! - Manages sync cursors per folder
//! - Polls server delta API periodically
//! - Applies remote changes to local filesystem
//! - Detects local changes via filesystem watcher
//! - Uploads local changes to server
//! - Handles conflicts using configurable strategies

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;
use tracing::{error, info, trace};
use uuid::Uuid;

use crate::api::client::ApiClient;
use crate::config::{Config, SyncDirection};
use crate::db::{Database, SyncQueueItemType, SyncCursor as DbSyncCursor};
use crate::fs::watcher::WatchEvent;
use crate::sync::conflict::{ConflictResolver, ConflictResolution};
use crate::sync::cursor::CursorManager;
use crate::sync::delta::DeltaProcessor;

/// Sync engine configuration
#[derive(Debug, Clone)]
pub struct SyncEngineConfig {
    /// Sync interval for polling
    pub sync_interval: std::time::Duration,
    /// Maximum concurrent uploads
    pub max_concurrent_uploads: usize,
    /// Maximum concurrent downloads
    pub max_concurrent_downloads: usize,
    /// Enable real-time sync via WebSocket
    pub enable_websocket: bool,
    /// Conflict resolution strategy
    pub conflict_resolution: ConflictResolution,
    /// Batch size for delta requests
    pub delta_batch_size: usize,
}

impl Default for SyncEngineConfig {
    fn default() -> Self {
        Self {
            sync_interval: std::time::Duration::from_secs(30),
            max_concurrent_uploads: 3,
            max_concurrent_downloads: 3,
            enable_websocket: true,
            conflict_resolution: ConflictResolution::LastWriteWins,
            delta_batch_size: 100,
        }
    }
}

impl From<&Config> for SyncEngineConfig {
    fn from(config: &Config) -> Self {
        Self {
            sync_interval: config.sync_interval,
            max_concurrent_uploads: config.max_concurrent_uploads,
            max_concurrent_downloads: config.max_concurrent_downloads,
            enable_websocket: config.enable_websocket,
            conflict_resolution: ConflictResolution::LastWriteWins,
            delta_batch_size: 100,
        }
    }
}

/// Sync status for a folder
#[derive(Debug, Clone)]
pub struct FolderSyncStatus {
    pub folder_id: Uuid,
    pub local_path: PathBuf,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub pending_uploads: usize,
    pub pending_downloads: usize,
    pub is_syncing: bool,
}

/// Overall sync status
#[derive(Debug, Clone)]
pub struct SyncStatus {
    pub is_running: bool,
    pub folders: Vec<FolderSyncStatus>,
    pub total_pending: usize,
}

/// Main sync engine
pub struct SyncEngine {
    /// Configuration
    config: SyncEngineConfig,
    /// API client
    api_client: ApiClient,
    /// Database
    db: Database,
    /// Cursor manager
    cursor_manager: CursorManager,
    /// Delta processor
    delta_processor: DeltaProcessor,
    /// Conflict resolver
    conflict_resolver: ConflictResolver,
    /// Active sync folders
    folders: Arc<RwLock<HashMap<Uuid, SyncFolder>>>,
    /// Shutdown signal sender
    shutdown_tx: Option<mpsc::Sender<()>>,
}

/// Internal folder state
#[derive(Debug)]
pub struct SyncFolder {
    pub folder_id: Uuid,
    pub local_path: PathBuf,
    pub direction: SyncDirection,
    pub last_sync_at: Option<DateTime<Utc>>,
}

impl SyncEngine {
    /// Create a new sync engine
    pub async fn new(
        config: SyncEngineConfig,
        api_client: ApiClient,
        db: Database,
    ) -> Result<Self> {
        let cursor_manager = CursorManager::new(&db);
        let delta_processor = DeltaProcessor::new(api_client.clone(), db.clone());
        let conflict_resolver = ConflictResolver::new(config.conflict_resolution);

        // Load existing sync folders from config
        let app_config = Config::load()?;
        let folders = Arc::new(RwLock::new(HashMap::new()));

        {
            let mut folders_guard = folders.write().await;
            for sync_folder in app_config.enabled_sync_folders() {
                folders_guard.insert(
                    sync_folder.folder_id,
                    SyncFolder {
                        folder_id: sync_folder.folder_id,
                        local_path: sync_folder.local_path.clone(),
                        direction: sync_folder.direction,
                        last_sync_at: None,
                    },
                );
            }
        }

        Ok(SyncEngine {
            config,
            api_client,
            db,
            cursor_manager,
            delta_processor,
            conflict_resolver,
            folders,
            shutdown_tx: None,
        })
    }

    /// Start the sync engine
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting sync engine");

        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx);

        // Spawn sync loop
        let folders = self.folders.clone();
        let api_client = self.api_client.clone();
        let db = self.db.clone();
        let cursor_manager = self.cursor_manager.clone();
        let delta_processor = self.delta_processor.clone();
        let conflict_resolver = self.conflict_resolver.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            let mut ticker = interval(config.sync_interval);

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        if let Err(e) = Self::sync_all_folders(
                            &folders,
                            &api_client,
                            &db,
                            &cursor_manager,
                            &delta_processor,
                            &conflict_resolver,
                            &config,
                        ).await {
                            error!("Sync error: {}", e);
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("Sync engine shutting down");
                        break;
                    }
                }
            }
        });

        info!("Sync engine started");
        Ok(())
    }

    /// Stop the sync engine
    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping sync engine");
        
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }
        
        Ok(())
    }

    /// Add a folder to sync
    pub async fn add_folder(&self, folder_id: Uuid, local_path: PathBuf, direction: SyncDirection) -> Result<()> {
        // Validate path exists
        if !local_path.exists() {
            std::fs::create_dir_all(&local_path)?;
        }

        // Add to database
        self.db.add_sync_folder(folder_id, &local_path, "/")?;

        // Add to active folders
        let mut folders = self.folders.write().await;
        folders.insert(
            folder_id,
            SyncFolder {
                folder_id,
                local_path: local_path.clone(),
                direction,
                last_sync_at: None,
            },
        );

        // Update config
        let mut config = Config::load()?;
        config.add_sync_folder(folder_id, local_path.clone())?;

        info!("Added sync folder {} at {}", folder_id, local_path.display());
        Ok(())
    }

    /// Remove a folder from sync
    pub async fn remove_folder(&self, folder_id: Uuid) -> Result<()> {
        // Remove from active folders
        let mut folders = self.folders.write().await;
        folders.remove(&folder_id);

        // Remove from database
        self.db.remove_sync_folder(folder_id)?;

        // Update config
        let mut config = Config::load()?;
        config.remove_sync_folder(folder_id)?;

        info!("Removed sync folder {}", folder_id);
        Ok(())
    }

    /// Get current sync status
    pub async fn get_status(&self) -> SyncStatus {
        let folders = self.folders.read().await;
        let (pending_uploads, pending_downloads) = self.db.get_queue_stats().unwrap_or((0, 0));

        let folder_statuses: Vec<FolderSyncStatus> = folders
            .values()
            .map(|f| FolderSyncStatus {
                folder_id: f.folder_id,
                local_path: f.local_path.clone(),
                last_sync_at: f.last_sync_at,
                last_error: None,
                pending_uploads,
                pending_downloads,
                is_syncing: false,
            })
            .collect();

        SyncStatus {
            is_running: self.shutdown_tx.is_some(),
            folders: folder_statuses,
            total_pending: pending_uploads + pending_downloads,
        }
    }

    /// Handle filesystem watch event
    pub async fn handle_watch_event(&self, event: WatchEvent) -> Result<()> {
        match event {
            WatchEvent::Created { path, folder_id } => {
                trace!("File created: {} in folder {}", path.display(), folder_id);
                self.db.enqueue(
                    SyncQueueItemType::Upload,
                    None,
                    folder_id,
                    &path,
                )?;
            }
            WatchEvent::Modified { path, folder_id } => {
                trace!("File modified: {} in folder {}", path.display(), folder_id);
                self.db.enqueue(
                    SyncQueueItemType::Upload,
                    None,
                    folder_id,
                    &path,
                )?;
            }
            WatchEvent::Deleted { path, folder_id } => {
                trace!("File deleted: {} in folder {}", path.display(), folder_id);
                self.db.enqueue(
                    SyncQueueItemType::DeleteRemote,
                    None,
                    folder_id,
                    &path,
                )?;
            }
            WatchEvent::Renamed { from_path, to_path, folder_id } => {
                trace!(
                    "File renamed: {} -> {} in folder {}",
                    from_path.display(),
                    to_path.display(),
                    folder_id
                );
                // Handle as delete + create for simplicity
                self.db.enqueue(
                    SyncQueueItemType::DeleteRemote,
                    None,
                    folder_id,
                    &from_path,
                )?;
                self.db.enqueue(
                    SyncQueueItemType::Upload,
                    None,
                    folder_id,
                    &to_path,
                )?;
            }
        }

        Ok(())
    }

    /// Force a sync of all folders
    pub async fn force_sync(&self) -> Result<()> {
        Self::sync_all_folders(
            &self.folders,
            &self.api_client,
            &self.db,
            &self.cursor_manager,
            &self.delta_processor,
            &self.conflict_resolver,
            &self.config,
        ).await
    }

    /// Sync all folders
    async fn sync_all_folders(
        folders: &Arc<RwLock<HashMap<Uuid, SyncFolder>>>,
        api_client: &ApiClient,
        db: &Database,
        cursor_manager: &CursorManager,
        delta_processor: &DeltaProcessor,
        conflict_resolver: &ConflictResolver,
        config: &SyncEngineConfig,
    ) -> Result<()> {
        let folders_guard = folders.read().await;

        for folder in folders_guard.values() {
            if let Err(e) = Self::sync_folder(
                folder,
                api_client,
                db,
                cursor_manager,
                delta_processor,
                conflict_resolver,
                config,
            ).await {
                error!("Failed to sync folder {}: {}", folder.folder_id, e);
            }
        }

        Ok(())
    }

    /// Sync a single folder
    async fn sync_folder(
        folder: &SyncFolder,
        api_client: &ApiClient,
        db: &Database,
        cursor_manager: &CursorManager,
        delta_processor: &DeltaProcessor,
        _conflict_resolver: &ConflictResolver,
        config: &SyncEngineConfig,
    ) -> Result<()> {
        trace!("Syncing folder {}", folder.folder_id);

        // Skip if upload-only
        if folder.direction == SyncDirection::UploadOnly {
            trace!("Folder {} is upload-only, skipping download sync", folder.folder_id);
        } else {
            // Get or create cursor
            let cursor = cursor_manager.get_or_create_cursor(folder.folder_id).await?;

            // Fetch and process deltas
            let mut has_more = true;
            let mut current_cursor = cursor.cursor_token;

            while has_more {
                match api_client.get_delta(&current_cursor, Some(config.delta_batch_size)).await {
                    Ok(delta) => {
                        for item in &delta.items {
                            if let Err(e) = delta_processor.process_delta(item, folder).await {
                                error!("Failed to process delta: {}", e);
                            }
                        }

                        has_more = delta.has_more;

                        // Update cursor
                        if let Some(next_cursor) = delta.next_cursor {
                            current_cursor = next_cursor;
                            
                            // Save cursor to database
                            let db_cursor = DbSyncCursor {
                                folder_id: folder.folder_id,
                                cursor_token: current_cursor.clone(),
                                last_event_id: Uuid::nil(), // TODO: extract from last item
                                updated_at: Utc::now(),
                            };
                            db.set_cursor(&db_cursor)?;
                        }
                    }
                    Err(e) => {
                        error!("Failed to fetch delta: {}", e);
                        break;
                    }
                }
            }
        }

        // Process local changes (upload queue)
        if folder.direction != SyncDirection::DownloadOnly {
            Self::process_upload_queue(folder, api_client, db).await?;
        }

        Ok(())
    }

    /// Process upload queue
    async fn process_upload_queue(
        _folder: &SyncFolder,
        api_client: &ApiClient,
        db: &Database,
    ) -> Result<()> {
        let pending = db.get_pending_queue_items(10)?;

        for item in pending {
            match item.item_type {
                SyncQueueItemType::Upload => {
                    // Upload file
                    match Self::upload_file(&item.local_path, item.folder_id, api_client).await {
                        Ok(_) => {
                            db.remove_from_queue(item.id)?;
                        }
                        Err(e) => {
                            let retry_at = Utc::now() + Duration::seconds(30);
                            db.mark_queue_item_failed(item.id, &e.to_string(), retry_at)?;
                        }
                    }
                }
                SyncQueueItemType::DeleteRemote => {
                    // Delete remote file
                    if let Some(file_id) = item.file_id {
                        match api_client.delete_file(file_id).await {
                            Ok(_) => {
                                db.remove_from_queue(item.id)?;
                            }
                            Err(e) => {
                                let retry_at = Utc::now() + Duration::seconds(30);
                                db.mark_queue_item_failed(item.id, &e.to_string(), retry_at)?;
                            }
                        }
                    } else {
                        db.remove_from_queue(item.id)?;
                    }
                }
                _ => {
                    // TODO: Handle other queue item types
                    db.remove_from_queue(item.id)?;
                }
            }
        }

        Ok(())
    }

    /// Upload a file
    async fn upload_file(
        path: &PathBuf,
        folder_id: Uuid,
        api_client: &ApiClient,
    ) -> Result<()> {
        use crate::api::upload::upload_simple;

        // TODO: Get parent folder ID from the path relative to sync root
        let parent_id = Some(folder_id);
        
        upload_simple(api_client, path, parent_id).await?;
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_engine_config_default() {
        let config = SyncEngineConfig::default();
        assert_eq!(config.max_concurrent_uploads, 3);
        assert_eq!(config.delta_batch_size, 100);
    }
}
