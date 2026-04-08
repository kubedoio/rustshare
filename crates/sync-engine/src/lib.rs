pub mod manager;
pub mod client;
pub mod planner;
pub mod worker;
pub mod socket;
pub mod daemon;

pub use manager::SyncManager;
pub use client::ApiClient;
pub use sync_domain::{SyncStatus, SyncRoot};
pub use client_state::Database;
pub use socket::{SocketServer, SocketClient, RpcRequest, RpcResponse};
pub use daemon::{DaemonHandle, stop_daemon, wait_for_stop};
use anyhow::Result;
use std::path::PathBuf;

pub struct SyncCore {
    pub manager: SyncManager,
}

impl SyncCore {
    pub fn new(database: Database, client: ApiClient, workspace_root: PathBuf) -> Self {
        let manager = SyncManager::new(database, client, workspace_root);
        Self { manager }
    }

    pub async fn start(&self) -> Result<()> {
        self.manager.start_rpc_server(4242).await?;
        self.manager.start().await
    }

    pub async fn register_root(&self, root: SyncRoot) -> Result<()> {
        self.manager.sync_root(root).await
    }

    pub fn get_status(&self) -> SyncStatus {
        SyncStatus::Idle
    }
}
