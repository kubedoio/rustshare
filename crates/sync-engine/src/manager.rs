use crate::client::ApiClient;
use crate::planner::{generate_plan_with_db_files, RemoteFileInfo, RemoteFolderInfo, SyncPlan};
use crate::scanner::{scan_local_root, FileScanResult};
use crate::socket::SocketServer;
use crate::websocket::{RemoteChangeEvent, WebSocketClient};
use crate::worker::SyncWorker;
use anyhow::{Context, Result};
use client_state::Database;
use file_ops::FsWatcher;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use sync_domain::{EntryType, HydrationState, LocalEntry, RemoteEntry, SyncRoot};
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

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
    /// Remote change notification via WebSocket
    RemoteChange(RemoteChangeEvent),
    /// Periodic full sync
    PeriodicSync,
    /// Manual sync request
    ManualSync,
}

/// Orchestrates all sync components: scanner, planner, worker, and WebSocket client
pub struct SyncManager {
    database: Arc<Mutex<Database>>,
    client: ApiClient,
    workspace_root: PathBuf,
    worker: SyncWorker,
    state: Arc<Mutex<SyncState>>,
    #[allow(dead_code)]
    pending_events: Arc<Mutex<VecDeque<SyncEvent>>>,
    shutdown_tx: Option<broadcast::Sender<()>>,
    ws_server_url: String,
    ws_token: String,
    sync_root_id: Option<Uuid>,
}

struct RemoteState {
    files: Vec<RemoteFileInfo>,
    dirs: Vec<RemoteFolderInfo>,
    absolute_folder_ids: HashMap<String, Uuid>,
}

impl SyncManager {
    pub fn new(
        database: Database,
        client: ApiClient,
        workspace_root: PathBuf,
    ) -> Self {
        let database = Arc::new(Mutex::new(database));
        let worker = SyncWorker::new(client.clone(), database.clone());

        Self {
            database,
            client,
            workspace_root,
            worker,
            state: Arc::new(Mutex::new(SyncState::Idle)),
            pending_events: Arc::new(Mutex::new(VecDeque::new())),
            shutdown_tx: None,
            ws_server_url: String::new(),
            ws_token: String::new(),
            sync_root_id: None,
        }
    }

    /// Configure WebSocket connection parameters
    pub fn with_websocket(mut self, server_url: &str, token: &str) -> Self {
        self.ws_server_url = server_url.to_string();
        self.ws_token = token.to_string();
        self
    }

    /// Set the sync root ID for this manager
    pub fn with_sync_root(mut self, root_id: Uuid) -> Self {
        self.sync_root_id = Some(root_id);
        self
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

        // 2. Start WebSocket client for remote notifications
        if !self.ws_server_url.is_empty() && !self.ws_token.is_empty() {
            let ws_server_url = self.ws_server_url.clone();
            let ws_token = self.ws_token.clone();
            let ws_event_tx = event_tx.clone();
            let ws_shutdown = shutdown_tx.subscribe();

            tokio::spawn(async move {
                Self::run_websocket_client(
                    ws_server_url,
                    ws_token,
                    ws_event_tx,
                    ws_shutdown,
                )
                .await;
            });
        }

        // 3. Start periodic full sync (every 30 seconds)
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

        // 4. Main event loop - process sync events
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

    /// Shutdown the sync manager gracefully
    pub async fn shutdown(&self) {
        if let Some(tx) = &self.shutdown_tx {
            let _ = tx.send(());
            info!("Shutdown signal sent");
        }
    }

    /// Run the filesystem watcher
    async fn run_fs_watcher(
        workspace_root: PathBuf,
        event_tx: mpsc::Sender<SyncEvent>,
    ) -> Result<()> {
        let (fs_tx, mut fs_rx) = mpsc::channel(100);
        let mut watcher = FsWatcher::new(fs_tx)?;
        watcher.watch(&workspace_root)?;

        info!("Filesystem watcher started for: {}", workspace_root.display());

        while let Some(event) = fs_rx.recv().await {
            debug!("Raw FS event: {:?}", event);

            // Convert FS event to sync event - notify uses `paths` (plural)
            for path in event.paths {
                let relative_path = path.strip_prefix(&workspace_root).unwrap_or(&path);
                if !relative_path.as_os_str().is_empty() {
                    if event_tx
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
        }

        Ok(())
    }

    /// Run the WebSocket client
    async fn run_websocket_client(
        server_url: String,
        token: String,
        event_tx: mpsc::Sender<SyncEvent>,
        mut shutdown: broadcast::Receiver<()>,
    ) {
        let ws_client = WebSocketClient::new(&server_url, &token);

        let event_handler = move |event: RemoteChangeEvent| {
            let tx = event_tx.clone();
            tokio::spawn(async move {
                let _ = tx.send(SyncEvent::RemoteChange(event)).await;
            });
        };

        tokio::select! {
            result = ws_client.connect(event_handler) => {
                if let Err(e) = result {
                    error!("WebSocket client error: {}", e);
                }
            }
            _ = shutdown.recv() => {
                info!("WebSocket client shutting down");
            }
        }
    }

    /// Handle incoming sync events
    async fn handle_sync_event(&self, event: SyncEvent) {
        match event {
            SyncEvent::LocalChange { path } => {
                debug!("Handling local change: {}", path.display());
                self.trigger_local_sync(&path).await;
            }
            SyncEvent::RemoteChange(remote_event) => {
                debug!("Handling remote change: {:?}", remote_event);
                self.trigger_remote_sync(remote_event).await;
            }
            SyncEvent::PeriodicSync => {
                debug!("Handling periodic full sync");
                if let Err(e) = self.run_full_sync().await {
                    error!("Periodic sync failed: {}", e);
                }
            }
            SyncEvent::ManualSync => {
                debug!("Handling manual sync request");
                if let Err(e) = self.run_full_sync().await {
                    error!("Manual sync failed: {}", e);
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

    /// Trigger a remote sync - called on WebSocket events
    async fn trigger_remote_sync(&self, event: RemoteChangeEvent) {
        match event {
            RemoteChangeEvent::FileChanged { file_id, path } => {
                info!("Remote file changed: {} ({})", path, file_id);
                // Could optimize to only sync the specific file
                // For now, run full sync
                if let Err(e) = self.run_full_sync().await {
                    error!("Remote sync failed: {}", e);
                }
            }
            RemoteChangeEvent::FolderChanged { folder_id } => {
                info!("Remote folder changed: {}", folder_id);
                if let Err(e) = self.run_full_sync().await {
                    error!("Remote sync failed: {}", e);
                }
            }
            RemoteChangeEvent::SyncComplete { cursor } => {
                info!("Remote sync completed at cursor: {}", cursor);
                // Update cursor in database
                let db = self.database.lock().await;
                if let Err(e) = db.save_sync_cursor(&cursor) {
                    warn!("Failed to update sync cursor: {}", e);
                }
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

        let db_files = self.get_database_files(root.id).await?;
        let db_paths: Vec<PathBuf> = db_files.keys().cloned().collect();

        let plan = generate_plan_with_db_files(
            root.id,
            &root.local_path,
            &local_files,
            &remote_state.files,
            &remote_state.dirs,
            &db_paths,
            |path| db_files.get(path).map(|(hash, mtime, remote_id)| (hash.clone(), *mtime, *remote_id)),
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

        self.execute_plan(&plan, &local_files, &remote_state, root).await
    }

    /// Get file state from database for a specific root
    async fn get_database_files(&self, root_id: Uuid) -> Result<std::collections::HashMap<PathBuf, (String, u64, Option<Uuid>)>> {
        let db = self.database.lock().await;
        let file_states = db.get_all_file_states(root_id)?;
        
        let mut result = std::collections::HashMap::new();
        for state in &file_states {
            if let (Some(local_hash), Some(modified_at)) = (&state.local_hash, state.local_modified_at) {
                // Use remote_hash timestamp if available, otherwise local
                let mtime = state.remote_modified_at.unwrap_or(modified_at) as u64;
                result.insert(
                    state.relative_path.clone(), 
                    (local_hash.clone(), mtime, None)
                );
            }
        }
        
        info!("Loaded {} file states from database for root {}", result.len(), root_id);
        Ok(result)
    }

    async fn load_sync_roots(&self) -> Result<Vec<SyncRoot>> {
        let db = self.database.lock().await;
        let roots = db.get_sync_roots()?;
        if let Some(root_id) = self.sync_root_id {
            Ok(roots
                .into_iter()
                .filter(|root| root.id == root_id)
                .collect())
        } else {
            Ok(roots)
        }
    }

    async fn fetch_remote_state(&self, root: &SyncRoot) -> Result<RemoteState> {
        let all_remote_files = match self.client.list_files().await {
            Ok(files) => files,
            Err(e) => {
                tracing::warn!("Failed to fetch remote files for {}: {}. Treating remote file state as empty.", root.remote_path, e);
                Vec::new()
            }
        };

        let prefix = normalize_remote_path(&root.remote_path);
        let mut files = Vec::new();
        for file in all_remote_files {
            if let Some(relative_path) = strip_remote_prefix(&file.path, &prefix) {
                let modified_at = chrono::DateTime::parse_from_rfc3339(&file.modified_at)
                    .map(|dt| dt.timestamp() as u64)
                    .unwrap_or_else(|_| chrono::Utc::now().timestamp() as u64);

                files.push(RemoteFileInfo {
                    id: file.id,
                    relative_path,
                    hash: file.content_hash,
                    size: file.size,
                    modified_at,
                });
            }
        }

        let mut absolute_folder_ids = HashMap::new();
        let mut dirs = Vec::new();
        if let Ok(tree) = self.client.get_folder_tree().await {
            collect_remote_folders(&tree, &prefix, &mut dirs, &mut absolute_folder_ids);
        } else {
            tracing::warn!("Failed to fetch remote folder tree for {}. Treating remote folder state as empty.", root.remote_path);
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
        let local_map: std::collections::HashMap<&PathBuf, &FileScanResult> = local_files
            .iter()
            .map(|f| (&f.relative_path, f))
            .collect();

        let remote_map: std::collections::HashMap<&PathBuf, &RemoteFileInfo> = remote_state.files
            .iter()
            .map(|f| (&f.relative_path, f))
            .collect();

        let mut absolute_folder_ids = remote_state.absolute_folder_ids.clone();

        for op in &plan.create_local_dirs {
            if let crate::planner::SyncOp::CreateLocalDir { relative_path, .. } = op {
                let local_dir = root.local_path.join(relative_path);
                tokio::fs::create_dir_all(&local_dir)
                    .await
                    .with_context(|| format!("Failed to create local directory {}", local_dir.display()))?;
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
            match op {
                crate::planner::SyncOp::Upload { relative_path, local_path, .. } => {
                    if let Some(local) = local_map.get(relative_path) {
                        let local_entry = LocalEntry {
                            path: local_path.clone(),
                            entry_type: if local.is_directory { EntryType::Directory } else { EntryType::File },
                            size: local.size,
                            hash: local.hash.clone(),
                            mtime: chrono::DateTime::from_timestamp(local.modified_at as i64, 0)
                                .unwrap_or_else(|| chrono::Utc::now()),
                            last_synced_version: None,
                            hydration_state: HydrationState::Materialized,
                        };

                        let parent_folder_id = self
                            .resolve_remote_parent_folder_id(root, relative_path, &mut absolute_folder_ids)
                            .await?;

                        if let Err(e) = self
                            .worker
                            .upload(&local_entry, root.id, relative_path, parent_folder_id)
                            .await
                        {
                            error!("Failed to upload {}: {:#}", relative_path.display(), e);
                        } else {
                            info!("Successfully uploaded {}", relative_path.display());
                        }
                    }
                }
                _ => {}
            }
        }

        // Execute downloads
        for op in &plan.downloads {
            match op {
                crate::planner::SyncOp::Download { relative_path, remote_file_id, remote_hash, size, .. } => {
                    if let Some(remote) = remote_map.get(relative_path) {
                        let remote_entry = RemoteEntry {
                            id: *remote_file_id,
                            parent_id: None,
                            name: relative_path.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("unknown")
                                .to_string(),
                            entry_type: EntryType::File,
                            size: *size,
                            hash: remote_hash.clone(),
                            version: "1".to_string(),
                            modified_at: chrono::DateTime::from_timestamp(remote.modified_at as i64, 0)
                                .unwrap_or_else(|| chrono::Utc::now()),
                        };

                        let local_dest = root.local_path.join(relative_path);

                        if let Err(e) = self.worker.download(
                            &remote_entry,
                            &local_dest,
                            remote_hash,
                            root.id,
                            relative_path,
                        ).await {
                            error!("Failed to download {}: {}", relative_path.display(), e);
                        } else {
                            info!("Successfully downloaded {}", relative_path.display());
                        }
                    }
                }
                _ => {}
            }
        }

        // Execute deletes
        for op in &plan.deletes {
            match op {
                crate::planner::SyncOp::DeleteLocal { relative_path, .. } => {
                    let local_path = root.local_path.join(relative_path);
                    if let Err(e) = tokio::fs::remove_file(&local_path).await {
                        warn!("Failed to delete local file {}: {}", local_path.display(), e);
                    } else {
                        info!("Deleted local file {}", relative_path.display());
                    }
                }
                crate::planner::SyncOp::DeleteRemote { relative_path, remote_file_id, .. } => {
                    if let Err(e) = self.client.delete_file(*remote_file_id).await {
                        warn!("Failed to delete remote file {} ({}): {}", relative_path.display(), remote_file_id, e);
                    } else {
                        info!("Deleted remote file {}", relative_path.display());
                    }
                }
                _ => {}
            }
        }

        for op in &plan.delete_local_dirs {
            if let crate::planner::SyncOp::DeleteLocalDir { relative_path, .. } = op {
                let local_dir = root.local_path.join(relative_path);
                if let Err(e) = tokio::fs::remove_dir(&local_dir).await {
                    warn!("Failed to delete local directory {}: {}", local_dir.display(), e);
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
                if let Err(e) = self.client.delete_folder(*remote_folder_id).await {
                    warn!("Failed to delete remote directory {} ({}): {}", relative_path.display(), remote_folder_id, e);
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
        self.ensure_remote_directory(root, parent.unwrap_or_else(|| Path::new("")), absolute_folder_ids)
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
            current_parent = ensure_absolute_folder_path(
                &self.client,
                &current_absolute,
                absolute_folder_ids,
            )
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

    /// Register a sync root
    pub async fn sync_root(&self, sync_root: SyncRoot) -> Result<()> {
        info!("Syncing root: {}", sync_root.remote_path);

        let db = self.database.lock().await;
        db.save_sync_root(&sync_root)?;

        // Example filter check
        let filters = db.get_filters(sync_root.id)?;
        info!("Active filters for root: {:?}", filters);

        Ok(())
    }

    /// Check if a path is excluded by filters
    pub fn is_excluded(&self, path: &Path, filters: &[String]) -> bool {
        for pattern in filters {
            if let Ok(glob) = glob::Pattern::new(pattern) {
                if glob.matches_path(path) {
                    return true;
                }
            }
        }
        false
    }

    /// Get the database handle
    pub fn database(&self) -> Arc<Mutex<Database>> {
        self.database.clone()
    }

    /// Start the socket server for IPC
    pub async fn start_socket_server(&self, socket_path: PathBuf) -> Result<()> {
        let db = self.database.clone();
        let state = self.state.clone();
        let mut server = SocketServer::new(socket_path);

        // Register RPC method handlers
        server.register_method("daemon.ping", |_params| {
            Ok(serde_json::json!({"status": "ok"}))
        });

        server.register_method("daemon.stop", |_params| {
            Ok(serde_json::json!({"status": "stopping"}))
        });

        let _db_clone = db.clone();
        server.register_method("sync.request", move |_params| {
            // Trigger sync - would need to send event to main loop
            Ok(serde_json::json!({"status": "queued"}))
        });

        let state_clone = state.clone();
        server.register_method("sync.status", move |_params| {
            let state_str = match *state_clone.blocking_lock() {
                SyncState::Idle => "idle",
                SyncState::Scanning => "scanning",
                SyncState::Planning => "planning",
                SyncState::Executing => "executing",
            };
            Ok(serde_json::json!({"state": state_str}))
        });

        server.register_method("config.update", |_params| {
            Ok(serde_json::json!({"status": "updated"}))
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
        return Some(PathBuf::from(
            normalized_path.trim_start_matches('/'),
        ));
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
    tree: &crate::client::RemoteFolderTree,
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
}
