use axum::{routing::post, Router, Json, extract::State};
use crate::client::ApiClient;
use client_state::Database;
use file_ops::FsWatcher;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::info;
use sync_domain::SyncRoot;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: serde_json::Value,
    pub id: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<serde_json::Value>,
    pub id: Option<serde_json::Value>,
}

pub struct SyncManager {
    database: Arc<Mutex<Database>>,
    _client: ApiClient,
    workspace_root: PathBuf,
    rpc_token: String,
}

impl SyncManager {
    pub fn new(database: Database, client: ApiClient, workspace_root: PathBuf) -> Self {
        let rpc_token = Uuid::new_v4().to_string();
        Self {
            database: Arc::new(Mutex::new(database)),
            _client: client,
            workspace_root,
            rpc_token,
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

    pub async fn start_rpc_server(&self, port: u16) -> anyhow::Result<()> {
        let app_state = Arc::new(RpcState {
            db: self.database.clone(),
            token: self.rpc_token.clone(),
        });
        
        let app = Router::new()
            .route("/rpc", post(handle_rpc))
            .with_state(app_state);

        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        info!("RPC server listening on {}", addr);
        
        let listener = tokio::net::TcpListener::bind(addr).await?;
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        Ok(())
    }
}

pub struct RpcState {
    pub db: Arc<Mutex<Database>>,
    pub token: String,
}

async fn handle_rpc(
    State(state): State<Arc<RpcState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<RpcRequest>,
) -> Json<RpcResponse> {
    // Basic token validation in header
    let provided_token = headers.get("X-RustShare-Token")
        .and_then(|h| h.to_str().ok());

    if provided_token != Some(&state.token) {
        return Json(RpcResponse {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(serde_json::json!({"code": -32000, "message": "Unauthorized"})),
            id: req.id,
        });
    }

    info!("Received RPC (Authenticated): {}", req.method);
    
    let result = match req.method.as_str() {
        "sync.request" => {
            // Trigger sync for path in params
            Some(serde_json::json!({"status": "queued"}))
        }
        "sync.status" => {
            // Query status for path in params
            Some(serde_json::json!({"status": "synced"}))
        }
        _ => None,
    };

    let error = if result.is_none() { 
        Some(serde_json::json!({"code": -32601, "message": "Method not found"})) 
    } else { 
        None 
    };

    Json(RpcResponse {
        jsonrpc: "2.0".to_string(),
        result,
        error,
        id: req.id,
    })
}
