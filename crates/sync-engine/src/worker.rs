use crate::client::ApiClient;
use crate::retry::{with_retry, with_retry_sync, RetryConfig};
use anyhow::{Context, Result};
use client_state::{Database, FileState, UploadSession};
use file_ops::atomic_rename;
use filetime::FileTime;
use futures_util::StreamExt;
use std::io;
use std::path::Path;
use std::sync::Arc;
use sync_domain::{LocalEntry, RemoteEntry, SyncError};
use sync_protocol::{CompleteUploadResponse, CreateUploadSessionRequest, UploadChunkResponse};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Mutex;
use tracing::info;
use uuid::Uuid;

const CHUNK_SIZE: usize = 5 * 1024 * 1024; // 5MB chunks
const TEMP_EXTENSION: &str = "rs_tmp";

fn total_chunks_for_size(file_size: u64) -> i32 {
    if file_size == 0 {
        1
    } else {
        file_size.div_ceil(CHUNK_SIZE as u64) as i32
    }
}

pub struct SyncWorker {
    client: ApiClient,
    database: Arc<Mutex<Database>>,
    retry_config: RetryConfig,
}

fn uploaded_file_state(
    file_state_id: i64,
    root_id: Uuid,
    relative_path: &Path,
    local: &LocalEntry,
    remote_hash: &str,
    remote_file_id: Uuid,
) -> FileState {
    let synced_at = local.mtime.timestamp();

    FileState {
        id: Some(file_state_id),
        root_id,
        relative_path: relative_path.to_path_buf(),
        local_hash: Some(local.hash.clone()),
        remote_hash: Some(remote_hash.to_string()),
        remote_file_id: Some(remote_file_id),
        local_modified_at: Some(synced_at),
        remote_modified_at: Some(synced_at),
        size: Some(local.size as i64),
        is_directory: Some(false),
        sync_status: Some("synced".to_string()),
        tombstone_side: None,
        tombstone_at: None,
        last_sync_at: Some(synced_at),
    }
}

fn downloaded_file_state(
    root_id: Uuid,
    relative_path: &Path,
    remote: &RemoteEntry,
    local_hash: &str,
) -> FileState {
    let synced_at = remote.modified_at.timestamp();

    FileState {
        id: None,
        root_id,
        relative_path: relative_path.to_path_buf(),
        local_hash: Some(local_hash.to_string()),
        remote_hash: Some(remote.hash.clone()),
        remote_file_id: Some(remote.id),
        local_modified_at: Some(synced_at),
        remote_modified_at: Some(synced_at),
        size: Some(remote.size as i64),
        is_directory: Some(false),
        sync_status: Some("synced".to_string()),
        tombstone_side: None,
        tombstone_at: None,
        last_sync_at: Some(synced_at),
    }
}

fn to_sync_error(error: anyhow::Error) -> SyncError {
    let message = error.to_string();

    if let Some(reqwest_error) = error.downcast_ref::<reqwest::Error>() {
        if let Some(status) = reqwest_error.status() {
            return match status {
                reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
                    SyncError::Auth(message)
                }
                reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::GONE => {
                    SyncError::Io(io::Error::new(io::ErrorKind::NotFound, message))
                }
                status if status.is_server_error() => SyncError::Network(message),
                status if status.is_client_error() => {
                    SyncError::Io(io::Error::new(io::ErrorKind::InvalidInput, message))
                }
                _ => SyncError::Other(message),
            };
        }

        if reqwest_error.is_timeout() || reqwest_error.is_connect() {
            return SyncError::Network(message);
        }
    }

    SyncError::Other(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Bytes,
        extract::{Path as AxumPath, State},
        routing::{get, post, put},
        Json, Router,
    };
    use chrono::{TimeZone, Utc};
    use serde::{Deserialize, Serialize};
    use std::{net::SocketAddr, sync::Arc};
    use tempfile::TempDir;
    use tokio::net::TcpListener;
    use uuid::Uuid;

    #[derive(Clone, Default)]
    struct UploadTestState {
        create_requests: Arc<tokio::sync::Mutex<Vec<TestCreateUploadSessionRequest>>>,
        uploaded_chunks: Arc<tokio::sync::Mutex<Vec<(Uuid, u32, Vec<u8>)>>>,
        complete_requests: Arc<tokio::sync::Mutex<Vec<Uuid>>>,
    }

    #[derive(Debug, Deserialize)]
    struct TestCreateUploadSessionRequest {
        folder_id: Option<Uuid>,
        file_name: String,
        mime_type: String,
        total_size: u64,
        chunk_size: u64,
        file_hash: Option<String>,
    }

    #[derive(Debug, Serialize)]
    struct TestCreateUploadSessionResponse {
        session_id: Uuid,
        total_chunks: u32,
        chunk_size: u64,
        expires_at: String,
    }

    #[derive(Debug, Serialize)]
    struct TestUploadChunkResponse {
        session_id: Uuid,
        chunk_index: u32,
        verified: bool,
        progress_percent: u8,
        is_complete: bool,
    }

    #[derive(Debug, Serialize)]
    struct TestCompleteUploadResponse {
        session_id: Uuid,
        file_id: Uuid,
        file_name: String,
        file_size: u64,
        content_hash: String,
    }

    async fn start_upload_test_server(state: UploadTestState) -> SocketAddr {
        async fn create_session(
            State(state): State<UploadTestState>,
            Json(request): Json<TestCreateUploadSessionRequest>,
        ) -> Json<TestCreateUploadSessionResponse> {
            state.create_requests.lock().await.push(request);
            Json(TestCreateUploadSessionResponse {
                session_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
                total_chunks: 1,
                chunk_size: CHUNK_SIZE as u64,
                expires_at: "2030-01-01T00:00:00Z".to_string(),
            })
        }

        async fn upload_chunk(
            State(state): State<UploadTestState>,
            AxumPath((session_id, chunk_index)): AxumPath<(Uuid, u32)>,
            body: Bytes,
        ) -> Json<TestUploadChunkResponse> {
            state
                .uploaded_chunks
                .lock()
                .await
                .push((session_id, chunk_index, body.to_vec()));
            Json(TestUploadChunkResponse {
                session_id,
                chunk_index,
                verified: true,
                progress_percent: 100,
                is_complete: true,
            })
        }

        async fn complete_session(
            State(state): State<UploadTestState>,
            AxumPath(session_id): AxumPath<Uuid>,
        ) -> Json<TestCompleteUploadResponse> {
            state.complete_requests.lock().await.push(session_id);
            Json(TestCompleteUploadResponse {
                session_id,
                file_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap(),
                file_name: "empty.txt".to_string(),
                file_size: 0,
                content_hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                    .to_string(),
            })
        }

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/api/v1/uploads/sessions", post(create_session))
            .route(
                "/api/v1/uploads/sessions/{session_id}/chunks/{chunk_index}",
                put(upload_chunk),
            )
            .route(
                "/api/v1/uploads/sessions/{session_id}/complete",
                post(complete_session),
            )
            .with_state(state);

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        addr
    }

    async fn start_download_test_server(body: Vec<u8>) -> SocketAddr {
        async fn download_file_content(
            AxumPath(_file_id): AxumPath<Uuid>,
            State(body): State<Arc<Vec<u8>>>,
        ) -> (axum::http::StatusCode, Bytes) {
            (
                axum::http::StatusCode::OK,
                Bytes::from(body.as_ref().clone()),
            )
        }

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route(
                "/api/v1/files/{file_id}/content",
                get(download_file_content),
            )
            .with_state(Arc::new(body));

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        addr
    }

    #[test]
    fn uploaded_state_uses_second_precision_for_both_sides() {
        let root_id = Uuid::new_v4();
        let local = LocalEntry {
            path: "/tmp/example.txt".into(),
            entry_type: sync_domain::EntryType::File,
            size: 42,
            hash: "hash123".to_string(),
            mtime: Utc.timestamp_opt(1_700_000_123, 987_000_000).unwrap(),
            last_synced_version: None,
            hydration_state: sync_domain::HydrationState::Materialized,
        };

        let state = uploaded_file_state(
            7,
            root_id,
            Path::new("example.txt"),
            &local,
            "hash123",
            Uuid::nil(),
        );

        assert_eq!(state.local_modified_at, Some(1_700_000_123));
        assert_eq!(state.remote_modified_at, Some(1_700_000_123));
        assert_eq!(state.remote_hash.as_deref(), Some("hash123"));
        assert_eq!(state.remote_file_id, Some(Uuid::nil()));
    }

    #[test]
    fn downloaded_state_uses_second_precision_for_both_sides() {
        let root_id = Uuid::new_v4();
        let remote = RemoteEntry {
            id: Uuid::new_v4(),
            parent_id: None,
            name: "example.txt".to_string(),
            entry_type: sync_domain::EntryType::File,
            size: 42,
            hash: "remote-hash".to_string(),
            version: "1".to_string(),
            modified_at: Utc.timestamp_opt(1_800_000_456, 654_000_000).unwrap(),
        };

        let state = downloaded_file_state(root_id, Path::new("example.txt"), &remote, "local-hash");

        assert_eq!(state.local_modified_at, Some(1_800_000_456));
        assert_eq!(state.remote_modified_at, Some(1_800_000_456));
    }

    #[tokio::test]
    async fn upload_zero_byte_file_creates_one_empty_chunk_and_sync_state() {
        let tempdir = TempDir::new().unwrap();
        let empty_file = tempdir.path().join("empty.txt");
        tokio::fs::write(&empty_file, b"").await.unwrap();

        let db = Database::open(&tempdir.path().join("state.db")).unwrap();
        let database = Arc::new(Mutex::new(db));
        let state = UploadTestState::default();
        let addr = start_upload_test_server(state.clone()).await;
        let client = ApiClient::new(&format!("http://{}", addr)).unwrap();
        let worker = SyncWorker::new(client, database.clone());
        let root_id = Uuid::new_v4();
        let relative_path = Path::new("empty.txt");
        let local = LocalEntry {
            path: empty_file.clone(),
            entry_type: sync_domain::EntryType::File,
            size: 0,
            hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
            mtime: Utc::now(),
            last_synced_version: None,
            hydration_state: sync_domain::HydrationState::Materialized,
        };

        worker
            .upload(&local, root_id, relative_path, None)
            .await
            .unwrap();

        let create_requests = state.create_requests.lock().await;
        assert_eq!(create_requests.len(), 1);
        assert_eq!(create_requests[0].file_name, "empty.txt");
        assert_eq!(create_requests[0].mime_type, "application/octet-stream");
        assert_eq!(create_requests[0].total_size, 0);
        assert_eq!(create_requests[0].chunk_size, CHUNK_SIZE as u64);
        assert_eq!(create_requests[0].folder_id, None);
        assert_eq!(create_requests[0].file_hash, None);
        drop(create_requests);

        let uploaded_chunks = state.uploaded_chunks.lock().await;
        assert_eq!(uploaded_chunks.len(), 1);
        assert_eq!(uploaded_chunks[0].1, 0);
        assert!(uploaded_chunks[0].2.is_empty());
        drop(uploaded_chunks);

        let complete_requests = state.complete_requests.lock().await;
        assert_eq!(complete_requests.len(), 1);
        drop(complete_requests);

        let db = database.lock().await;
        let file_state = db.get_file_state(root_id, relative_path).unwrap().unwrap();
        assert_eq!(file_state.sync_status.as_deref(), Some("synced"));
        assert_eq!(file_state.size, Some(0));
        assert_eq!(
            file_state.remote_file_id,
            Some(Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap())
        );
        assert_eq!(
            file_state.remote_hash.as_deref(),
            Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        );
    }

    #[tokio::test]
    async fn download_preserves_remote_modified_time() {
        let tempdir = TempDir::new().unwrap();
        let download_path = tempdir.path().join("note.md");
        let db = Database::open(&tempdir.path().join("state.db")).unwrap();
        let database = Arc::new(Mutex::new(db));
        let body = b"hello from remote".to_vec();
        let addr = start_download_test_server(body.clone()).await;
        let client = ApiClient::new(&format!("http://{}", addr)).unwrap();
        let worker = SyncWorker::new(client, database);
        let root_id = Uuid::new_v4();
        let modified_at = Utc.timestamp_opt(1_800_000_123, 0).unwrap();
        let remote = RemoteEntry {
            id: Uuid::new_v4(),
            parent_id: None,
            name: "note.md".to_string(),
            entry_type: sync_domain::EntryType::File,
            size: body.len() as u64,
            hash: "5c90a3b922e9376c872e234f4c56f14612549d59c8c83f9afdfab7f9221e8d07".to_string(),
            version: "1".to_string(),
            modified_at,
        };

        worker
            .download(
                &remote,
                &download_path,
                &remote.hash,
                root_id,
                Path::new("note.md"),
            )
            .await
            .unwrap();

        let metadata = tokio::fs::metadata(&download_path).await.unwrap();
        let local_mtime = metadata
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        assert_eq!(tokio::fs::read(&download_path).await.unwrap(), body);
        assert_eq!(local_mtime, modified_at.timestamp());
    }
}

impl SyncWorker {
    pub fn new(client: ApiClient, database: Arc<Mutex<Database>>) -> Self {
        Self {
            client,
            database,
            retry_config: RetryConfig::default(),
        }
    }

    pub fn with_retry_config(mut self, config: RetryConfig) -> Self {
        self.retry_config = config;
        self
    }

    // ============================================================================
    // Upload with Resumable Chunks
    // ============================================================================

    pub async fn upload(
        &self,
        local: &LocalEntry,
        remote_root: Uuid,
        relative_path: &Path,
        parent_folder_id: Option<Uuid>,
    ) -> Result<()> {
        // Skip directories - they should be created separately
        if local.entry_type == sync_domain::EntryType::Directory {
            info!("Skipping directory: {}", local.path.display());
            return Ok(());
        }

        info!("Uploading {}...", local.path.display());

        let file_size = local.size;
        let total_chunks = total_chunks_for_size(file_size);
        let file_hash = &local.hash;

        // Get or create file state
        let file_state_id = self
            .get_or_create_file_state(remote_root, relative_path, local)
            .await?;

        // Check for existing upload session
        let session = self
            .get_or_create_upload_session(
                file_state_id,
                parent_folder_id,
                relative_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown"),
                file_size,
                total_chunks,
                file_hash,
            )
            .await?;

        let session_id = session.session_id.clone();
        let uploaded_chunks = session.uploaded_chunks;

        info!(
            "Upload session {}: chunk {}/{} already uploaded",
            session_id, uploaded_chunks, total_chunks
        );

        // Upload remaining chunks
        for chunk_index in uploaded_chunks..total_chunks {
            let chunk_data = self.read_chunk(&local.path, chunk_index).await?;

            // Upload chunk with retry
            with_retry(
                &self.retry_config,
                &format!("upload chunk {}", chunk_index),
                || async {
                    self.upload_chunk(&session_id, chunk_index, chunk_data.clone())
                        .await
                },
            )
            .await?;

            // Update progress in database
            let db = self.database.lock().await;
            db.update_uploaded_chunks(&session_id, chunk_index + 1)?;
            drop(db);

            info!(
                "Uploaded chunk {}/{} for {}",
                chunk_index + 1,
                total_chunks,
                local.path.display()
            );
        }

        // Complete the upload
        let complete_result =
            with_retry_sync(&self.retry_config, "complete upload session", || async {
                self.complete_upload_session(&session_id)
                    .await
                    .map_err(to_sync_error)
            })
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        // Update file state as synced
        self.update_file_state_as_synced(
            file_state_id,
            remote_root,
            relative_path,
            local,
            &complete_result.content_hash,
            complete_result.file_id,
        )
        .await?;

        // Clean up upload session
        let db = self.database.lock().await;
        db.delete_upload_session(&session_id)?;
        drop(db);

        info!(
            "Successfully uploaded {} -> file_id={}",
            local.path.display(),
            complete_result.file_id
        );
        Ok(())
    }

    // ============================================================================
    // Download with Verification
    // ============================================================================

    pub async fn download(
        &self,
        remote: &RemoteEntry,
        local_dest: &Path,
        expected_hash: &str,
        root_id: Uuid,
        relative_path: &Path,
    ) -> Result<()> {
        info!("Downloading {} -> {}...", remote.name, local_dest.display());

        // Create temp file path
        let mut temp_path = local_dest.to_path_buf();
        temp_path.set_extension(TEMP_EXTENSION);

        // Ensure parent directory exists
        if let Some(parent) = temp_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        // Download with retry
        let download_result = with_retry_sync(&self.retry_config, "download file", || async {
            self.client
                .download_file(remote.id)
                .await
                .map_err(to_sync_error)
        })
        .await;

        let response = download_result.map_err(|e| anyhow::anyhow!(e.to_string()))?;

        // Stream response to temp file
        let mut temp_file = tokio::fs::File::create(&temp_path)
            .await
            .with_context(|| format!("Failed to create temp file: {}", temp_path.display()))?;

        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("Failed to read download chunk")?;
            temp_file.write_all(&chunk).await.with_context(|| {
                format!("Failed to write to temp file: {}", temp_path.display())
            })?;
        }

        temp_file
            .flush()
            .await
            .with_context(|| format!("Failed to flush temp file: {}", temp_path.display()))?;
        drop(temp_file);

        // Verify hash
        if !self.verify_hash(&temp_path, expected_hash).await? {
            tokio::fs::remove_file(&temp_path).await.ok();
            return Err(anyhow::anyhow!(
                "Hash verification failed for downloaded file: expected {}, file: {}",
                expected_hash,
                local_dest.display()
            ));
        }

        // Atomic rename
        atomic_rename(&temp_path, local_dest).with_context(|| {
            format!(
                "Failed to rename {} to {}",
                temp_path.display(),
                local_dest.display()
            )
        })?;

        // Preserve the remote timestamp locally so the next scan does not treat a fresh
        // download as a brand-new local edit.
        let remote_mtime = FileTime::from_unix_time(remote.modified_at.timestamp(), 0);
        filetime::set_file_mtime(local_dest, remote_mtime)
            .with_context(|| format!("Failed to set modified time for {}", local_dest.display()))?;

        // Update file state
        self.update_file_state_after_download(root_id, relative_path, remote, expected_hash)
            .await?;

        info!(
            "Successfully downloaded {} -> {}",
            remote.name,
            local_dest.display()
        );
        Ok(())
    }

    // ============================================================================
    // Helper Methods
    // ============================================================================

    async fn get_or_create_file_state(
        &self,
        root_id: Uuid,
        relative_path: &Path,
        local: &LocalEntry,
    ) -> Result<i64> {
        let db = self.database.lock().await;

        if let Some(existing) = db.get_file_state(root_id, relative_path)? {
            drop(db);
            return Ok(existing.id.unwrap_or(0));
        }
        drop(db);

        // Create new file state
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let state = FileState {
            id: None,
            root_id,
            relative_path: relative_path.to_path_buf(),
            local_hash: Some(local.hash.clone()),
            remote_hash: None,
            remote_file_id: None,
            local_modified_at: Some(local.mtime.timestamp()),
            remote_modified_at: None,
            size: Some(local.size as i64),
            is_directory: Some(false),
            sync_status: Some("uploading".to_string()),
            tombstone_side: None,
            tombstone_at: None,
            last_sync_at: Some(now),
        };

        let db = self.database.lock().await;
        let id = db.upsert_file_state(&state)?;
        Ok(id)
    }

    async fn update_file_state_as_synced(
        &self,
        file_state_id: i64,
        root_id: Uuid,
        relative_path: &Path,
        local: &LocalEntry,
        remote_hash: &str,
        remote_file_id: Uuid,
    ) -> Result<()> {
        let state = uploaded_file_state(
            file_state_id,
            root_id,
            relative_path,
            local,
            remote_hash,
            remote_file_id,
        );

        let db = self.database.lock().await;
        db.upsert_file_state(&state)?;
        Ok(())
    }

    async fn update_file_state_after_download(
        &self,
        root_id: Uuid,
        relative_path: &Path,
        remote: &RemoteEntry,
        local_hash: &str,
    ) -> Result<()> {
        let state = downloaded_file_state(root_id, relative_path, remote, local_hash);

        let db = self.database.lock().await;
        db.upsert_file_state(&state)?;
        Ok(())
    }

    async fn get_or_create_upload_session(
        &self,
        file_state_id: i64,
        parent_folder_id: Option<Uuid>,
        file_name: &str,
        file_size: u64,
        total_chunks: i32,
        file_hash: &str,
    ) -> Result<UploadSession> {
        let db = self.database.lock().await;

        // Check for existing session
        if let Some(existing) = db.get_upload_session(file_state_id)? {
            // Check if session is expired
            if !db.is_upload_session_expired(&existing) {
                drop(db);
                return Ok(existing);
            }
            // Session expired, delete it
            db.delete_upload_session(&existing.session_id)?;
        }
        drop(db);

        // Create new session via API
        let session_response = self
            .create_upload_session(parent_folder_id, file_size, file_hash, file_name)
            .await?;

        // Store session in database
        let session = UploadSession {
            id: None,
            file_state_id,
            session_id: session_response.session_id.to_string(),
            total_chunks,
            uploaded_chunks: 0,
            chunk_size: CHUNK_SIZE as i32,
            expires_at: Some(
                chrono::DateTime::parse_from_rfc3339(&session_response.expires_at)?.timestamp(),
            ),
        };

        let db = self.database.lock().await;
        db.create_upload_session(&session)?;
        Ok(session)
    }

    async fn create_upload_session(
        &self,
        folder_id: Option<Uuid>,
        total_size: u64,
        _file_hash: &str,
        file_name: &str,
    ) -> Result<sync_protocol::CreateUploadSessionResponse> {
        let request = CreateUploadSessionRequest {
            folder_id,
            file_name: file_name.to_string(),
            mime_type: "application/octet-stream".to_string(),
            total_size,
            chunk_size: CHUNK_SIZE as u64,
            file_hash: None,
        };

        self.client.create_upload_session(request).await
    }

    async fn upload_chunk(
        &self,
        session_id: &str,
        chunk_index: i32,
        chunk_data: Vec<u8>,
    ) -> Result<UploadChunkResponse> {
        let session_uuid = Uuid::parse_str(session_id).context("Invalid session ID format")?;

        // Skip MD5 hash - server doesn't use Content-MD5 header correctly
        let result = self
            .client
            .upload_chunk(session_uuid, chunk_index as u32, chunk_data, None)
            .await?;

        if !result.verified {
            return Err(anyhow::anyhow!(
                "Chunk {} hash verification failed",
                chunk_index
            ));
        }

        Ok(result)
    }

    async fn complete_upload_session(&self, session_id: &str) -> Result<CompleteUploadResponse> {
        let session_uuid = Uuid::parse_str(session_id).context("Invalid session ID format")?;

        self.client.complete_upload_session(session_uuid).await
    }

    async fn read_chunk(&self, file_path: &Path, chunk_index: i32) -> Result<Vec<u8>> {
        let offset = chunk_index as u64 * CHUNK_SIZE as u64;
        let mut file = tokio::fs::File::open(file_path)
            .await
            .with_context(|| format!("Failed to open file: {}", file_path.display()))?;

        // Seek to chunk position
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .with_context(|| format!("Failed to seek in file: {}", file_path.display()))?;

        // Read chunk
        let mut buffer = vec![0u8; CHUNK_SIZE];
        let bytes_read = file
            .read(&mut buffer)
            .await
            .with_context(|| format!("Failed to read chunk from file: {}", file_path.display()))?;

        buffer.truncate(bytes_read);
        Ok(buffer)
    }

    async fn verify_hash(&self, file_path: &Path, expected_hash: &str) -> Result<bool> {
        let computed_hash = tokio::task::spawn_blocking({
            let path = file_path.to_path_buf();
            move || file_ops::calculate_hash(&path)
        })
        .await??;

        Ok(computed_hash == expected_hash)
    }
}
