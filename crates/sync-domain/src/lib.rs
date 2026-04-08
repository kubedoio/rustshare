use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRoot {
    pub id: Uuid,
    pub remote_path: String,
    pub local_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryType {
    File,
    Directory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteEntry {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub entry_type: EntryType,
    pub size: u64,
    pub hash: String,
    pub version: String,
    pub modified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalEntry {
    pub path: PathBuf,
    pub entry_type: EntryType,
    pub size: u64,
    pub hash: String,
    pub mtime: DateTime<Utc>,
    pub last_synced_version: Option<String>,
    pub hydration_state: HydrationState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HydrationState {
    Placeholder,
    Materialized,
    Pinned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncStatus {
    Idle,
    Scanning,
    SyncingUp,
    SyncingDown,
    Paused,
    Conflicted,
    Degraded,
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncEvent {
    LocalCreated(LocalEntry),
    LocalUpdated(LocalEntry),
    LocalDeleted(PathBuf),
    RemoteCreated(RemoteEntry),
    RemoteUpdated(RemoteEntry),
    RemoteDeleted(Uuid),
    Conflict(Uuid, PathBuf),
}

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Database error: {0}")]
    Database(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("Auth error: {0}")]
    Auth(String),
    #[error("Conflict: {0}")]
    Conflict(PathBuf),
    #[error("Other: {0}")]
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub server_url: String,
    pub workspace_root: PathBuf,
    pub poll_interval_seconds: u64,
}
