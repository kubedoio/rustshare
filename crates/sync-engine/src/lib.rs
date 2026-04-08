pub mod client;
pub mod daemon;
pub mod manager;
pub mod planner;
pub mod retry;
pub mod scanner;
pub mod socket;
pub mod websocket;
pub mod worker;

use anyhow::Result;
pub use client::ApiClient;
pub use client_state::Database;
pub use daemon::{stop_daemon, wait_for_stop, DaemonHandle};
pub use manager::SyncManager;
pub use socket::{RpcRequest, RpcResponse, SocketClient, SocketServer};
use std::path::PathBuf;
pub use sync_domain::{SyncRoot, SyncStatus};

pub struct SyncCore {
    pub manager: SyncManager,
    socket_path: PathBuf,
}

impl SyncCore {
    pub fn new(
        database: Database,
        client: ApiClient,
        workspace_root: PathBuf,
        socket_path: PathBuf,
    ) -> Self {
        let manager = SyncManager::new(database, client, workspace_root);
        Self {
            manager,
            socket_path,
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        self.manager
            .start_socket_server(self.socket_path.clone())
            .await?;
        self.manager.start().await
    }

    pub async fn register_root(&self, root: SyncRoot) -> Result<()> {
        self.manager.sync_root(root).await
    }

    pub fn get_status(&self) -> SyncStatus {
        SyncStatus::Idle
    }
}
