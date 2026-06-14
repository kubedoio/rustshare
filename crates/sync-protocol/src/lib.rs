use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sync_domain::{RemoteEntry, SyncRoot};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceRegistrationRequest {
    pub name: String,
    pub os: String,
    pub device_type: String,
    pub version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceRegistrationResponse {
    pub device_id: uuid::Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeltaRequest {
    pub cursor: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeltaResponse {
    pub cursor: String,
    pub changes: Vec<DeltaChange>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum DeltaChange {
    Upsert(RemoteEntry),
    Delete(uuid::Uuid),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncRootsResponse {
    pub roots: Vec<SyncRoot>,
}

#[derive(Debug, Serialize)]
pub struct CreateUploadSessionRequest {
    pub folder_id: Option<Uuid>,
    pub file_name: String,
    pub mime_type: String,
    pub total_size: u64,
    #[serde(default = "default_chunk_size")]
    pub chunk_size: u64,
    pub file_hash: Option<String>,
}

pub fn default_chunk_size() -> u64 {
    5 * 1024 * 1024 // 5MB default
}

#[derive(Debug, Deserialize)]
pub struct CreateUploadSessionResponse {
    pub session_id: Uuid,
    pub total_chunks: u32,
    #[serde(default = "default_chunk_size")]
    pub chunk_size: u64,
    pub expires_at: String,
}

#[derive(Debug, Deserialize)]
pub struct UploadChunkResponse {
    pub session_id: Uuid,
    pub chunk_index: u32,
    pub verified: bool,
    pub progress_percent: u8,
    pub is_complete: bool,
}

#[derive(Debug, Deserialize)]
pub struct CompleteUploadResponse {
    pub session_id: Uuid,
    pub file_id: Uuid,
    pub file_name: String,
    pub file_size: u64,
    #[serde(default)]
    pub content_hash: Option<String>,
}

/// File info from the files list endpoint
#[derive(Debug, Deserialize)]
pub struct RemoteFile {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    /// SHA-256 hash of the file contents. Older/lighter server responses may
    /// omit this field; in that case `current_version` is used as a change
    /// token instead.
    #[serde(default)]
    pub content_hash: Option<String>,
    /// Monotonically increasing file version. Used as a fallback change token
    /// when the server does not provide a content hash.
    #[serde(default)]
    pub current_version: i32,
    pub size: u64,
    pub modified_at: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RemoteFolderTree {
    pub folder: RemoteFolderNode,
    #[serde(default)]
    pub subfolders: Vec<RemoteFolderTree>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RemoteFolderNode {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub parent_folder_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RemoteFolder {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub parent_folder_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct CreateFolderRequest {
    pub name: String,
    pub parent_folder_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct ListFilesResponse {
    pub files: Vec<RemoteFile>,
}
