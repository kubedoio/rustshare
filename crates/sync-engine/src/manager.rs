use crate::client::ApiClient;

use crate::planner::{
    generate_plan_with_db_files, DbFileState, RemoteFileInfo, RemoteFolderInfo, SyncPlan,
};
use crate::scanner::{scan_local_root, FileScanResult};
use crate::socket::SocketServer;
use crate::worker::SyncWorker;
use anyhow::{Context, Result};
use client_state::{BrokenRemoteEntry, Database, FileState};
use file_ops::FsWatcher;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use sync_domain::{EntryType, HydrationState, LocalEntry, RemoteEntry, SyncRoot};
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

const BROKEN_REMOTE_RETRY_AFTER_SECS: i64 = 60 * 60;

/// The current state of the sync engine
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncState {
    Idle,
    Scanning,
    Planning,
    Executing,
}

impl std::fmt::Display for SyncState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncState::Idle => write!(f, "idle"),
            SyncState::Scanning => write!(f, "scanning"),
            SyncState::Planning => write!(f, "planning"),
            SyncState::Executing => write!(f, "executing"),
        }
    }
}

/// Events that can trigger a sync operation
#[derive(Debug, Clone)]
pub enum SyncEvent {
    /// Local filesystem changed
    LocalChange { path: PathBuf },
    /// Periodic full sync
    PeriodicSync,
}

/// Orchestrates all sync components: scanner, planner, worker, and WebSocket client
pub struct SyncManager {
    database: Arc<Mutex<Database>>,
    client: ApiClient,
    workspace_root: PathBuf,
    worker: SyncWorker,
    state: Arc<Mutex<SyncState>>,
    shutdown_tx: Option<broadcast::Sender<()>>,
}

struct RemoteState {
    files: Vec<RemoteFileInfo>,
    dirs: Vec<RemoteFolderInfo>,
    absolute_folder_ids: HashMap<String, Uuid>,
}

impl SyncManager {
    pub fn new(database: Database, client: ApiClient, workspace_root: PathBuf) -> Self {
        let database = Arc::new(Mutex::new(database));
        let worker = SyncWorker::new(client.clone(), database.clone());

        Self {
            database,
            client,
            workspace_root,
            worker,
            state: Arc::new(Mutex::new(SyncState::Idle)),
            shutdown_tx: None,
        }
    }

    /// Get the current sync state
    pub async fn current_state(&self) -> SyncState {
        *self.state.lock().await
    }

    /// Start the sync manager with all components
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting Sync Manager...");

        let (shutdown_tx, mut shutdown_rx) = broadcast::channel(1);
        self.shutdown_tx = Some(shutdown_tx.clone());

        let (event_tx, mut event_rx) = mpsc::channel(100);

        // 1. Start filesystem watcher
        let fs_event_tx = event_tx.clone();
        let workspace_root = self.workspace_root.clone();
        tokio::spawn(async move {
            if let Err(e) = Self::run_fs_watcher(workspace_root, fs_event_tx).await {
                error!("Filesystem watcher error: {}", e);
            }
        });

        // 2. Start periodic full sync (every 30 seconds)
        let periodic_event_tx = event_tx.clone();
        let mut periodic_shutdown = shutdown_tx.subscribe();
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(30));
            ticker.tick().await; // Skip first immediate tick

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        if periodic_event_tx.send(SyncEvent::PeriodicSync).await.is_err() {
                            break;
                        }
                    }
                    _ = periodic_shutdown.recv() => {
                        info!("Periodic sync task shutting down");
                        break;
                    }
                }
            }
        });

        // 3. Main event loop - process sync events
        info!("Sync Manager started, waiting for events...");

        loop {
            tokio::select! {
                Some(event) = event_rx.recv() => {
                    self.handle_sync_event(event).await;
                }
                _ = shutdown_rx.recv() => {
                    info!("Shutdown signal received, stopping Sync Manager");
                    break;
                }
                else => {
                    // Channel closed
                    break;
                }
            }
        }

        info!("Sync Manager stopped");
        Ok(())
    }

    /// Run the filesystem watcher
    async fn run_fs_watcher(
        workspace_root: PathBuf,
        event_tx: mpsc::Sender<SyncEvent>,
    ) -> Result<()> {
        let (fs_tx, mut fs_rx) = mpsc::channel(100);
        let mut watcher = FsWatcher::new(fs_tx)?;
        watcher.watch(&workspace_root)?;

        info!(
            "Filesystem watcher started for: {}",
            workspace_root.display()
        );

        while let Some(event) = fs_rx.recv().await {
            debug!("Raw FS event: {:?}", event);

            // Convert FS event to sync event - notify uses `paths` (plural)
            for path in event.paths {
                let relative_path = path.strip_prefix(&workspace_root).unwrap_or(&path);
                if !relative_path.as_os_str().is_empty()
                    && event_tx
                        .send(SyncEvent::LocalChange {
                            path: relative_path.to_path_buf(),
                        })
                        .await
                        .is_err()
                {
                    return Ok(());
                }
            }
        }

        Ok(())
    }

    /// Run the WebSocket client
    /// Handle incoming sync events
    async fn handle_sync_event(&self, event: SyncEvent) {
        match event {
            SyncEvent::LocalChange { path } => {
                debug!("Handling local change: {}", path.display());
                self.trigger_local_sync(&path).await;
            }
            SyncEvent::PeriodicSync => {
                debug!("Handling periodic full sync");
                if let Err(e) = self.run_full_sync().await {
                    error!("Periodic sync failed: {}", e);
                }
            }
        }
    }

    /// Trigger a local sync - called on filesystem events
    /// Queues the request if a sync is already in progress
    async fn trigger_local_sync(&self, _path: &Path) {
        let state = self.state.lock().await;

        match *state {
            SyncState::Idle => {
                // Can start immediately - release lock and run
                drop(state);
                if let Err(e) = self.run_full_sync().await {
                    error!("Local sync failed: {}", e);
                }
            }
            _ => {
                // Queue for later - debounce local changes
                debug!("Sync in progress ({}), local change queued", *state);
                // Drop pending events older than 5 seconds to avoid buildup
                drop(state);
            }
        }
    }

    /// Run a complete sync cycle: scan → plan → execute
    ///
    /// State transitions: Idle → Scanning → Planning → Executing → Idle
    pub async fn run_full_sync(&self) -> Result<()> {
        // Only one sync at a time - check and transition to Scanning
        {
            let mut state = self.state.lock().await;
            if *state != SyncState::Idle {
                debug!("Sync already in progress ({}), skipping", *state);
                return Ok(());
            }
            *state = SyncState::Scanning;
            info!("Starting full sync - state: scanning");
        }

        let sync_roots = match self.load_sync_roots().await {
            Ok(roots) if !roots.is_empty() => roots,
            Ok(_) => {
                *self.state.lock().await = SyncState::Idle;
                return Err(anyhow::anyhow!("No sync roots configured in database"));
            }
            Err(e) => {
                *self.state.lock().await = SyncState::Idle;
                return Err(e.context("Failed to load sync roots"));
            }
        };

        let result = async {
            for root in &sync_roots {
                self.run_full_sync_for_root(root).await?;
            }
            Ok(())
        }
        .await;

        // Return to Idle
        {
            let mut state = self.state.lock().await;
            *state = SyncState::Idle;
            info!("Sync complete - state: idle");
        }

        result
    }

    /// Scan the local workspace for files
    async fn scan_local(&self, root_path: &Path) -> Result<Vec<FileScanResult>> {
        tokio::task::spawn_blocking({
            let root_path = root_path.to_path_buf();
            move || scan_local_root(&root_path)
        })
        .await
        .context("Scan task panicked")?
        .context("Failed to scan local root")
    }

    async fn run_full_sync_for_root(&self, root: &SyncRoot) -> Result<()> {
        let local_files = self
            .scan_local(&root.local_path)
            .await
            .with_context(|| format!("Scan phase failed for {}", root.local_path.display()))?;

        {
            let mut state = self.state.lock().await;
            *state = SyncState::Planning;
            info!(
                "Scan complete for {} - state: planning ({} local entries)",
                root.local_path.display(),
                local_files.len()
            );
        }

        let remote_state = self
            .fetch_remote_state(root)
            .await
            .with_context(|| format!("Failed to fetch remote state for {}", root.remote_path))?;

        self.persist_synced_directory_states(root.id, &local_files, &remote_state)
            .await?;

        let db_files = self.get_database_files(root.id).await?;
        let db_paths: Vec<PathBuf> = db_files.keys().cloned().collect();

        let plan = generate_plan_with_db_files(
            root.id,
            &root.local_path,
            &local_files,
            &remote_state.files,
            &remote_state.dirs,
            &db_paths,
            |path| db_files.get(path).cloned(),
        );

        if plan.is_empty() {
            info!(
                "No sync operations needed for {} -> {}",
                root.local_path.display(),
                root.remote_path
            );
            return Ok(());
        }

        info!(
            "Sync plan for {}: {} local dirs, {} remote dirs, {} uploads, {} downloads, {} deletes, {} conflicts",
            root.remote_path,
            plan.create_local_dirs.len(),
            plan.create_remote_dirs.len(),
            plan.uploads.len(),
            plan.downloads.len(),
            plan.deletes.len(),
            plan.conflicts.len()
        );

        {
            let mut state = self.state.lock().await;
            *state = SyncState::Executing;
            info!("State: executing");
        }

        self.execute_plan(&plan, &local_files, &remote_state, root)
            .await
    }

    /// Get file state from database for a specific root
    async fn get_database_files(
        &self,
        root_id: Uuid,
    ) -> Result<std::collections::HashMap<PathBuf, DbFileState>> {
        let db = self.database.lock().await;
        let file_states = db.get_all_file_states(root_id)?;

        let mut result = std::collections::HashMap::new();
        for state in &file_states {
            let local_hash = state.local_hash.clone().unwrap_or_default();
            let remote_hash = state.remote_hash.clone().unwrap_or_default();
            let modified_at = state
                .remote_modified_at
                .or(state.local_modified_at)
                .unwrap_or_else(|| state.last_sync_at.unwrap_or_default())
                as u64;
            result.insert(
                state.relative_path.clone(),
                DbFileState {
                    local_hash,
                    remote_hash,
                    modified_at,
                    _remote_id: state.remote_file_id,
                    is_directory: state.is_directory.unwrap_or(false),
                    sync_status: state
                        .sync_status
                        .clone()
                        .unwrap_or_else(|| "synced".to_string()),
                    tombstone_side: state.tombstone_side.clone(),
                    tombstone_at: state.tombstone_at.map(|value| value as u64),
                },
            );
        }

        info!(
            "Loaded {} file states from database for root {}",
            result.len(),
            root_id
        );
        Ok(result)
    }

    async fn load_sync_roots(&self) -> Result<Vec<SyncRoot>> {
        let db = self.database.lock().await;
        db.get_sync_roots()
    }

    async fn fetch_remote_state(&self, root: &SyncRoot) -> Result<RemoteState> {
        let quarantined_remote_files = self.get_quarantined_remote_files(root.id).await?;
        let all_remote_files = match self.client.list_files().await {
            Ok(files) => files,
            Err(e) => {
                tracing::warn!(
                    "Failed to fetch remote files for {}: {:#}. Treating remote file state as empty.",
                    root.remote_path,
                    e
                );
                Vec::new()
            }
        };

        let prefix = normalize_remote_path(&root.remote_path);
        let mut files = Vec::new();
        for file in all_remote_files {
            if let Some(relative_path) = strip_remote_prefix(&file.path, &prefix) {
                if quarantined_remote_files.iter().any(|entry| {
                    entry.relative_path == relative_path && entry.remote_file_id == file.id
                }) {
                    warn!(
                        "Skipping quarantined remote file {} ({}) for root {}",
                        relative_path.display(),
                        file.id,
                        root.id
                    );
                    continue;
                }

                let modified_at = chrono::DateTime::parse_from_rfc3339(&file.modified_at)
                    .map(|dt| dt.timestamp() as u64)
                    .unwrap_or_else(|_| chrono::Utc::now().timestamp() as u64);

                let remote_hash = file
                    .content_hash
                    .clone()
                    .unwrap_or_else(|| file.current_version.to_string());

                files.push(RemoteFileInfo {
                    id: file.id,
                    relative_path,
                    hash: remote_hash,
                    size: file.size,
                    modified_at,
                });
            }
        }

        let mut absolute_folder_ids = HashMap::new();
        let mut dirs = Vec::new();
        match self.client.get_folder_tree().await {
            Ok(tree) => collect_remote_folders(&tree, &prefix, &mut dirs, &mut absolute_folder_ids),
            Err(e) => {
                tracing::warn!(
                    "Failed to fetch remote folder tree for {}: {:#}. Treating remote folder state as empty.",
                    root.remote_path,
                    e
                );
            }
        }

        Ok(RemoteState {
            files,
            dirs,
            absolute_folder_ids,
        })
    }

    /// Execute the sync plan using the worker
    async fn execute_plan(
        &self,
        plan: &SyncPlan,
        local_files: &[FileScanResult],
        remote_state: &RemoteState,
        root: &SyncRoot,
    ) -> Result<()> {
        let local_map: std::collections::HashMap<&PathBuf, &FileScanResult> =
            local_files.iter().map(|f| (&f.relative_path, f)).collect();

        let remote_map: std::collections::HashMap<&PathBuf, &RemoteFileInfo> = remote_state
            .files
            .iter()
            .map(|f| (&f.relative_path, f))
            .collect();

        let mut absolute_folder_ids = remote_state.absolute_folder_ids.clone();

        for op in &plan.create_local_dirs {
            if let crate::planner::SyncOp::CreateLocalDir { relative_path, .. } = op {
                let local_dir = root.local_path.join(relative_path);
                tokio::fs::create_dir_all(&local_dir)
                    .await
                    .with_context(|| {
                        format!("Failed to create local directory {}", local_dir.display())
                    })?;
            }
        }

        for op in &plan.create_remote_dirs {
            if let crate::planner::SyncOp::CreateRemoteDir { relative_path, .. } = op {
                self.ensure_remote_directory(root, relative_path, &mut absolute_folder_ids)
                    .await?;
            }
        }

        // Execute uploads
        for op in &plan.uploads {
            if let crate::planner::SyncOp::Upload {
                relative_path,
                local_path,
                ..
            } = op
            {
                if let Some(local) = local_map.get(relative_path) {
                    let local_entry = LocalEntry {
                        path: local_path.clone(),
                        entry_type: if local.is_directory {
                            EntryType::Directory
                        } else {
                            EntryType::File
                        },
                        size: local.size,
                        hash: local.hash.clone(),
                        mtime: chrono::DateTime::from_timestamp(local.modified_at as i64, 0)
                            .unwrap_or_else(chrono::Utc::now),
                        last_synced_version: None,
                        hydration_state: HydrationState::Materialized,
                    };

                    let parent_folder_id = self
                        .resolve_remote_parent_folder_id(
                            root,
                            relative_path,
                            &mut absolute_folder_ids,
                        )
                        .await?;

                    if let Err(e) = self
                        .worker
                        .upload(&local_entry, root.id, relative_path, parent_folder_id)
                        .await
                    {
                        error!("Failed to upload {}: {:#}", relative_path.display(), e);
                    } else {
                        self.clear_broken_remote_file(root.id, relative_path).await;
                        info!("Successfully uploaded {}", relative_path.display());
                    }
                }
            }
        }

        // Execute downloads
        for op in &plan.downloads {
            if let crate::planner::SyncOp::Download {
                relative_path,
                remote_file_id,
                remote_hash,
                size,
                ..
            } = op
            {
                if let Some(remote) = remote_map.get(relative_path) {
                    let remote_entry = RemoteEntry {
                        id: *remote_file_id,
                        parent_id: None,
                        name: relative_path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown")
                            .to_string(),
                        entry_type: EntryType::File,
                        size: *size,
                        hash: remote_hash.clone(),
                        version: "1".to_string(),
                        modified_at: chrono::DateTime::from_timestamp(remote.modified_at as i64, 0)
                            .unwrap_or_else(chrono::Utc::now),
                    };

                    let local_dest = root.local_path.join(relative_path);

                    if let Err(e) = self
                        .worker
                        .download(
                            &remote_entry,
                            &local_dest,
                            remote_hash,
                            root.id,
                            relative_path,
                        )
                        .await
                    {
                        error!("Failed to download {}: {}", relative_path.display(), e);
                        if is_missing_remote_error(&e.to_string()) {
                            self.quarantine_broken_remote_file(
                                root.id,
                                relative_path,
                                *remote_file_id,
                                &e.to_string(),
                            )
                            .await;
                        }
                    } else {
                        self.clear_broken_remote_file(root.id, relative_path).await;
                        info!("Successfully downloaded {}", relative_path.display());
                    }
                }
            }
        }

        // Execute deletes
        for op in &plan.deletes {
            match op {
                crate::planner::SyncOp::DeleteLocal { relative_path, .. } => {
                    let local_path = root.local_path.join(relative_path);
                    match tokio::fs::remove_file(&local_path).await {
                        Ok(_) => {
                            self.mark_delete_tombstone(root.id, relative_path, "remote")
                                .await;
                            self.clear_broken_remote_file(root.id, relative_path).await;
                            info!("Deleted local file {}", relative_path.display());
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                            self.mark_delete_tombstone(root.id, relative_path, "remote")
                                .await;
                            self.clear_broken_remote_file(root.id, relative_path).await;
                            info!("Local file {} was already absent", relative_path.display());
                        }
                        Err(e) => {
                            warn!(
                                "Failed to delete local file {}: {}",
                                local_path.display(),
                                e
                            );
                        }
                    }
                }
                crate::planner::SyncOp::DeleteRemote {
                    relative_path,
                    remote_file_id,
                    ..
                } => match self.client.delete_file(*remote_file_id).await {
                    Ok(_) => {
                        self.mark_delete_tombstone(root.id, relative_path, "local")
                            .await;
                        self.clear_broken_remote_file(root.id, relative_path).await;
                        info!("Deleted remote file {}", relative_path.display());
                    }
                    Err(e) if is_missing_remote_error(&e.to_string()) => {
                        self.mark_delete_tombstone(root.id, relative_path, "local")
                            .await;
                        self.clear_broken_remote_file(root.id, relative_path).await;
                        info!(
                            "Remote file {} ({}) was already absent",
                            relative_path.display(),
                            remote_file_id
                        );
                    }
                    Err(e) => {
                        warn!(
                            "Failed to delete remote file {} ({}): {}",
                            relative_path.display(),
                            remote_file_id,
                            e
                        );
                    }
                },
                _ => {}
            }
        }

        for op in &plan.delete_local_dirs {
            if let crate::planner::SyncOp::DeleteLocalDir { relative_path, .. } = op {
                let local_dir = root.local_path.join(relative_path);
                match tokio::fs::remove_dir(&local_dir).await {
                    Ok(_) => {
                        self.mark_delete_tombstone(root.id, relative_path, "remote")
                            .await;
                        info!("Deleted local directory {}", relative_path.display());
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        self.mark_delete_tombstone(root.id, relative_path, "remote")
                            .await;
                        info!(
                            "Local directory {} was already absent",
                            relative_path.display()
                        );
                    }
                    Err(e) => {
                        warn!(
                            "Failed to delete local directory {}: {}",
                            local_dir.display(),
                            e
                        );
                    }
                }
            }
        }

        for op in &plan.delete_remote_dirs {
            if let crate::planner::SyncOp::DeleteRemoteDir {
                relative_path,
                remote_folder_id,
                ..
            } = op
            {
                match self.client.delete_folder(*remote_folder_id).await {
                    Ok(_) => {
                        self.mark_delete_tombstone(root.id, relative_path, "local")
                            .await;
                        info!("Deleted remote directory {}", relative_path.display());
                    }
                    Err(e) if is_missing_remote_error(&e.to_string()) => {
                        self.mark_delete_tombstone(root.id, relative_path, "local")
                            .await;
                        info!(
                            "Remote directory {} ({}) was already absent",
                            relative_path.display(),
                            remote_folder_id
                        );
                    }
                    Err(e) => {
                        warn!(
                            "Failed to delete remote directory {} ({}): {}",
                            relative_path.display(),
                            remote_folder_id,
                            e
                        );
                    }
                }
            }
        }

        Ok(())
    }

    async fn resolve_remote_parent_folder_id(
        &self,
        root: &SyncRoot,
        relative_path: &Path,
        absolute_folder_ids: &mut HashMap<String, Uuid>,
    ) -> Result<Option<Uuid>> {
        let parent = relative_path
            .parent()
            .filter(|path| !path.as_os_str().is_empty());
        self.ensure_remote_directory(
            root,
            parent.unwrap_or_else(|| Path::new("")),
            absolute_folder_ids,
        )
        .await
    }

    async fn ensure_remote_directory(
        &self,
        root: &SyncRoot,
        relative_path: &Path,
        absolute_folder_ids: &mut HashMap<String, Uuid>,
    ) -> Result<Option<Uuid>> {
        let mut current_parent: Option<Uuid> = None;
        let mut current_absolute = normalize_remote_path(&root.remote_path);

        if current_absolute != "/" {
            current_parent =
                ensure_absolute_folder_path(&self.client, &current_absolute, absolute_folder_ids)
                    .await?;
        }

        for component in relative_path.components() {
            let std::path::Component::Normal(name) = component else {
                continue;
            };
            let segment = name.to_string_lossy();
            current_absolute = join_remote_path(&current_absolute, &segment);
            current_parent = if let Some(existing) = absolute_folder_ids.get(&current_absolute) {
                Some(*existing)
            } else {
                let created = self.client.create_folder(&segment, current_parent).await?;
                absolute_folder_ids.insert(current_absolute.clone(), created.id);
                Some(created.id)
            };
        }

        Ok(current_parent)
    }

    async fn get_quarantined_remote_files(&self, root_id: Uuid) -> Result<Vec<BrokenRemoteEntry>> {
        let db = self.database.lock().await;
        let now = chrono::Utc::now().timestamp();
        Ok(db
            .get_broken_remote_entries(root_id)?
            .into_iter()
            .filter(|entry| now - entry.last_seen_at < BROKEN_REMOTE_RETRY_AFTER_SECS)
            .collect())
    }

    async fn quarantine_broken_remote_file(
        &self,
        root_id: Uuid,
        relative_path: &Path,
        remote_file_id: Uuid,
        error: &str,
    ) {
        let observed_at = chrono::Utc::now().timestamp();
        let db = self.database.lock().await;
        if let Err(db_error) = db.upsert_broken_remote_entry(
            root_id,
            relative_path,
            remote_file_id,
            error,
            observed_at,
        ) {
            warn!(
                "Failed to quarantine broken remote file {} ({}): {}",
                relative_path.display(),
                remote_file_id,
                db_error
            );
        } else {
            warn!(
                "Quarantined broken remote file {} ({}) after download failure",
                relative_path.display(),
                remote_file_id
            );
        }
    }

    async fn clear_broken_remote_file(&self, root_id: Uuid, relative_path: &Path) {
        let db = self.database.lock().await;
        if let Err(e) = db.clear_broken_remote_entries_for_path(root_id, relative_path) {
            warn!(
                "Failed to clear broken-remote quarantine for {}: {}",
                relative_path.display(),
                e
            );
        }
    }

    async fn mark_delete_tombstone(&self, root_id: Uuid, relative_path: &Path, source_side: &str) {
        let deleted_at = chrono::Utc::now().timestamp();
        let db = self.database.lock().await;
        if let Err(e) = db.mark_file_tombstone(root_id, relative_path, source_side, deleted_at) {
            warn!(
                "Failed to record tombstone for {} after {} delete: {}",
                relative_path.display(),
                source_side,
                e
            );
        }
    }

    async fn persist_synced_directory_states(
        &self,
        root_id: Uuid,
        local_files: &[FileScanResult],
        remote_state: &RemoteState,
    ) -> Result<()> {
        let local_dirs: HashMap<&PathBuf, &FileScanResult> = local_files
            .iter()
            .filter(|entry| entry.is_directory)
            .map(|entry| (&entry.relative_path, entry))
            .collect();
        let remote_dirs: HashMap<&PathBuf, &RemoteFolderInfo> = remote_state
            .dirs
            .iter()
            .map(|entry| (&entry.relative_path, entry))
            .collect();

        if local_dirs.is_empty() || remote_dirs.is_empty() {
            return Ok(());
        }

        let now = chrono::Utc::now().timestamp();
        let db = self.database.lock().await;

        for (relative_path, local_dir) in &local_dirs {
            if let Some(remote_dir) = remote_dirs.get(relative_path) {
                let state = FileState {
                    id: db
                        .get_file_state(root_id, relative_path)?
                        .and_then(|existing| existing.id),
                    root_id,
                    relative_path: (*relative_path).clone(),
                    local_hash: None,
                    remote_hash: None,
                    remote_file_id: Some(remote_dir.id),
                    local_modified_at: Some(local_dir.modified_at as i64),
                    remote_modified_at: Some(remote_dir.modified_at as i64),
                    size: Some(0),
                    is_directory: Some(true),
                    sync_status: Some("synced".to_string()),
                    tombstone_side: None,
                    tombstone_at: None,
                    last_sync_at: Some(now),
                };
                db.upsert_file_state(&state)?;
            }
        }

        Ok(())
    }

    /// Register a sync root
    pub async fn sync_root(&self, sync_root: SyncRoot) -> Result<()> {
        info!("Syncing root: {}", sync_root.remote_path);

        let db = self.database.lock().await;
        db.save_sync_root(&sync_root)?;

        Ok(())
    }

    /// Get the database handle
    pub fn database(&self) -> Arc<Mutex<Database>> {
        self.database.clone()
    }

    /// Start the socket server for IPC
    pub async fn start_socket_server(&self, socket_path: PathBuf) -> Result<()> {
        let mut server = SocketServer::new(socket_path);

        // Register RPC method handlers
        server.register_method("daemon.ping", |_params| {
            Ok(serde_json::json!({"status": "ok"}))
        });

        // Bind and run the server
        server.bind().await?;

        info!("Socket server starting on {:?}", server.socket_path());

        // Run the server in a spawned task so it doesn't block
        tokio::spawn(async move {
            if let Err(e) = server.run().await {
                tracing::error!("Socket server error: {}", e);
            }
        });

        Ok(())
    }
}

fn normalize_remote_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "/" {
        "/".to_string()
    } else {
        let without_trailing = trimmed.trim_end_matches('/');
        if without_trailing.starts_with('/') {
            without_trailing.to_string()
        } else {
            format!("/{}", without_trailing)
        }
    }
}

fn join_remote_path(base: &str, segment: &str) -> String {
    let base = normalize_remote_path(base);
    if base == "/" {
        format!("/{}", segment)
    } else {
        format!("{}/{}", base, segment)
    }
}

fn strip_remote_prefix(path: &str, prefix: &str) -> Option<PathBuf> {
    let normalized_path = normalize_remote_path(path);
    let normalized_prefix = normalize_remote_path(prefix);

    if normalized_prefix == "/" {
        return Some(PathBuf::from(normalized_path.trim_start_matches('/')));
    }

    if normalized_path == normalized_prefix {
        return Some(PathBuf::new());
    }

    let full_prefix = format!("{}/", normalized_prefix);
    normalized_path
        .strip_prefix(&full_prefix)
        .map(PathBuf::from)
}

fn collect_remote_folders(
    tree: &sync_protocol::RemoteFolderTree,
    prefix: &str,
    dirs: &mut Vec<RemoteFolderInfo>,
    absolute_folder_ids: &mut HashMap<String, Uuid>,
) {
    let normalized_path = normalize_remote_path(&tree.folder.path);
    if tree.folder.id != Uuid::nil() {
        absolute_folder_ids.insert(normalized_path.clone(), tree.folder.id);
        if let Some(relative_path) = strip_remote_prefix(&normalized_path, prefix) {
            if !relative_path.as_os_str().is_empty() {
                dirs.push(RemoteFolderInfo {
                    id: tree.folder.id,
                    relative_path,
                    modified_at: tree.folder.updated_at.timestamp() as u64,
                });
            }
        }
    }

    for child in &tree.subfolders {
        collect_remote_folders(child, prefix, dirs, absolute_folder_ids);
    }
}

fn is_missing_remote_error(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("404") || lower.contains("not found") || lower.contains("410")
}

async fn ensure_absolute_folder_path(
    client: &ApiClient,
    absolute_path: &str,
    absolute_folder_ids: &mut HashMap<String, Uuid>,
) -> Result<Option<Uuid>> {
    let normalized = normalize_remote_path(absolute_path);
    if normalized == "/" {
        return Ok(None);
    }

    let mut current_parent = None;
    let mut current_path = "/".to_string();

    for component in normalized.trim_start_matches('/').split('/') {
        current_path = join_remote_path(&current_path, component);
        current_parent = if let Some(existing) = absolute_folder_ids.get(&current_path) {
            Some(*existing)
        } else {
            let created = client.create_folder(component, current_parent).await?;
            absolute_folder_ids.insert(current_path.clone(), created.id);
            Some(created.id)
        };
    }

    Ok(current_parent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_state_display() {
        assert_eq!(SyncState::Idle.to_string(), "idle");
        assert_eq!(SyncState::Scanning.to_string(), "scanning");
        assert_eq!(SyncState::Planning.to_string(), "planning");
        assert_eq!(SyncState::Executing.to_string(), "executing");
    }

    #[test]
    fn test_strip_remote_prefix_scopes_to_sync_root() {
        assert_eq!(
            strip_remote_prefix("/designs/icons/logo.svg", "/designs"),
            Some(PathBuf::from("icons/logo.svg"))
        );
        assert_eq!(
            strip_remote_prefix("/designs", "/designs"),
            Some(PathBuf::new())
        );
        assert_eq!(strip_remote_prefix("/other/file.txt", "/designs"), None);
    }

    #[test]
    fn test_join_remote_path_preserves_root_shape() {
        assert_eq!(join_remote_path("/", "docs"), "/docs");
        assert_eq!(join_remote_path("/docs", "specs"), "/docs/specs");
    }

    #[test]
    fn test_missing_remote_error_detection() {
        assert!(is_missing_remote_error("HTTP 404 Not Found"));
        assert!(is_missing_remote_error("server returned 410 gone"));
        assert!(!is_missing_remote_error("HTTP 503 Service Unavailable"));
    }
}
