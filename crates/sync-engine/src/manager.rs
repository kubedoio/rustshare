use crate::client::ApiClient;
use crate::socket::SocketServer;
use client_state::Database;
use file_ops::FsWatcher;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use sync_domain::SyncRoot;
use tokio::sync::{mpsc, Mutex};
use tracing::info;

pub struct SyncManager {
    database: Arc<Mutex<Database>>,
    _client: ApiClient,
    workspace_root: PathBuf,
}

impl SyncManager {
    pub fn new(database: Database, client: ApiClient, workspace_root: PathBuf) -> Self {
        Self {
            database: Arc::new(Mutex::new(database)),
            _client: client,
            workspace_root,
        }
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        info!("Starting Sync Manager...");

        let (tx, mut rx) = mpsc::channel(100);
        let mut watcher = FsWatcher::new(tx)?;
        watcher.watch(&self.workspace_root)?;

        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                info!("FS Event: {:?}", event);
                // Trigger planning/syncing logic here
            }
        });

        Ok(())
    }

    pub async fn sync_root(&self, sync_root: SyncRoot) -> anyhow::Result<()> {
        info!("Syncing root: {}", sync_root.remote_path);

        let db = self.database.lock().await;
        db.save_sync_root(&sync_root)?;

        // Example filter check
        let filters = db.get_filters(sync_root.id)?;
        info!("Active filters for root: {:?}", filters);

        Ok(())
    }

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

    pub fn database(&self) -> Arc<Mutex<Database>> {
        self.database.clone()
    }

    pub async fn start_socket_server(&self, socket_path: PathBuf) -> anyhow::Result<()> {
        let db = self.database.clone();
        let mut server = SocketServer::new(socket_path);

        // Register RPC method handlers
        server.register_method("daemon.ping", |_params| {
            Ok(serde_json::json!({"status": "ok"}))
        });

        server.register_method("daemon.stop", |_params| {
            // Trigger shutdown - in practice this would signal the daemon to stop
            Ok(serde_json::json!({"status": "stopping"}))
        });

        let _db_clone = db.clone();
        server.register_method("sync.request", move |params| {
            // Extract path from params and trigger sync
            let _path = params.get("path").and_then(|p| p.as_str());
            Ok(serde_json::json!({"status": "queued"}))
        });

        let _db_clone2 = db.clone();
        server.register_method("sync.status", move |params| {
            // Query sync status from database
            let _path = params.get("path").and_then(|p| p.as_str());
            Ok(serde_json::json!({"status": "synced"}))
        });

        server.register_method("config.update", |_params| {
            // Update configuration
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
