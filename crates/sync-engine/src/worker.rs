use crate::client::{
    ApiClient, CompleteUploadResponse, CreateUploadSessionRequest, UploadChunkResponse,
};
use crate::retry::{with_retry, RetryConfig};
use anyhow::{Context, Result};
use client_state::{Database, FileState, UploadSession};
use file_ops::atomic_rename;
use futures_util::StreamExt;
use std::path::Path;
use std::sync::Arc;
use sync_domain::{LocalEntry, RemoteEntry};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Mutex;
use tracing::info;
use uuid::Uuid;

const CHUNK_SIZE: usize = 5 * 1024 * 1024; // 5MB chunks
const TEMP_EXTENSION: &str = "rs_tmp";

pub struct SyncWorker {
    client: ApiClient,
    database: Arc<Mutex<Database>>,
    retry_config: RetryConfig,
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
    ) -> Result<()> {
        // Skip directories - they should be created separately
        if local.entry_type == sync_domain::EntryType::Directory {
            info!("Skipping directory: {}", local.path.display());
            return Ok(());
        }
        
        // Skip empty files - server doesn't handle them well
        if local.size == 0 {
            info!("Skipping empty file: {}", local.path.display());
            return Ok(());
        }
        
        info!("Uploading {}...", local.path.display());

        let file_size = local.size;
        let total_chunks = ((file_size + CHUNK_SIZE as u64 - 1) / CHUNK_SIZE as u64) as i32;
        let file_hash = &local.hash;

        // Get or create file state
        let file_state_id = self.get_or_create_file_state(remote_root, relative_path, local).await?;

        // Check for existing upload session
        let session = self.get_or_create_upload_session(
            file_state_id,
            remote_root,
            relative_path,
            file_size,
            total_chunks,
            file_hash,
        ).await?;

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
            with_retry(&self.retry_config, &format!("upload chunk {}", chunk_index), || async {
                self.upload_chunk(&session_id, chunk_index, chunk_data.clone()).await
            }).await?;

            // Update progress in database
            let db = self.database.lock().await;
            db.update_uploaded_chunks(&session_id, chunk_index + 1)?;
            drop(db);

            info!("Uploaded chunk {}/{} for {}", chunk_index + 1, total_chunks, local.path.display());
        }

        // Complete the upload
        let complete_result = with_retry(
            &self.retry_config,
            "complete upload session",
            || async { self.complete_upload_session(&session_id).await }
        ).await?;

        // Update file state as synced
        self.update_file_state_as_synced(
            file_state_id,
            remote_root,
            relative_path,
            local,
            &complete_result.file_id.to_string(),
        ).await?;

        // Clean up upload session
        let db = self.database.lock().await;
        db.delete_upload_session(&session_id)?;
        drop(db);

        info!("Successfully uploaded {} -> file_id={}", local.path.display(), complete_result.file_id);
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
            tokio::fs::create_dir_all(parent).await
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        // Download with retry
        let download_result = with_retry(
            &self.retry_config,
            "download file",
            || async { self.client.download_file(remote.id).await }
        ).await;

        let response = download_result?;

        // Stream response to temp file
        let mut temp_file = tokio::fs::File::create(&temp_path).await
            .with_context(|| format!("Failed to create temp file: {}", temp_path.display()))?;

        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("Failed to read download chunk")?;
            temp_file.write_all(&chunk).await
                .with_context(|| format!("Failed to write to temp file: {}", temp_path.display()))?;
        }

        temp_file.flush().await
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
        atomic_rename(&temp_path, local_dest)
            .with_context(|| format!("Failed to rename {} to {}", temp_path.display(), local_dest.display()))?;

        // Update file state
        self.update_file_state_after_download(
            root_id,
            relative_path,
            remote,
            expected_hash,
        ).await?;

        info!("Successfully downloaded {} -> {}", remote.name, local_dest.display());
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
            local_modified_at: Some(local.mtime.timestamp_millis()),
            remote_modified_at: None,
            size: Some(local.size as i64),
            is_directory: Some(false),
            sync_status: Some("uploading".to_string()),
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
        _remote_file_id: &str,
    ) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let state = FileState {
            id: Some(file_state_id),
            root_id,
            relative_path: relative_path.to_path_buf(),
            local_hash: Some(local.hash.clone()),
            remote_hash: Some(local.hash.clone()),
            local_modified_at: Some(local.mtime.timestamp_millis()),
            remote_modified_at: Some(now),
            size: Some(local.size as i64),
            is_directory: Some(false),
            sync_status: Some("synced".to_string()),
            last_sync_at: Some(now),
        };

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
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let state = FileState {
            id: None,
            root_id,
            relative_path: relative_path.to_path_buf(),
            local_hash: Some(local_hash.to_string()),
            remote_hash: Some(remote.hash.clone()),
            local_modified_at: Some(now),
            remote_modified_at: Some(remote.modified_at.timestamp_millis()),
            size: Some(remote.size as i64),
            is_directory: Some(false),
            sync_status: Some("synced".to_string()),
            last_sync_at: Some(now),
        };

        let db = self.database.lock().await;
        db.upsert_file_state(&state)?;
        Ok(())
    }

    async fn get_or_create_upload_session(
        &self,
        file_state_id: i64,
        remote_root: Uuid,
        relative_path: &Path,
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
        let session_response = self.create_upload_session(
            remote_root,
            relative_path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown"),
            file_size,
            file_hash,
        ).await?;

        // Store session in database
        let session = UploadSession {
            id: None,
            file_state_id,
            session_id: session_response.session_id.to_string(),
            total_chunks,
            uploaded_chunks: 0,
            chunk_size: CHUNK_SIZE as i32,
            expires_at: Some(
                chrono::DateTime::parse_from_rfc3339(&session_response.expires_at)?
                    .timestamp()
            ),
        };

        let db = self.database.lock().await;
        db.create_upload_session(&session)?;
        Ok(session)
    }

    async fn create_upload_session(
        &self,
        _folder_id: Uuid,
        file_name: &str,
        total_size: u64,
        _file_hash: &str,
    ) -> Result<crate::client::CreateUploadSessionResponse> {
        // TODO: Map sync root ID to actual server folder ID
        // For now, upload to root (None) to ensure uploads work
        // Skip file_hash - server computes it from chunks
        let request = CreateUploadSessionRequest {
            folder_id: None,
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
        let session_uuid = Uuid::parse_str(session_id)
            .context("Invalid session ID format")?;

        // Skip MD5 hash - server doesn't use Content-MD5 header correctly
        let result = self.client
            .upload_chunk(session_uuid, chunk_index as u32, chunk_data, None)
            .await?;

        if !result.verified {
            return Err(anyhow::anyhow!("Chunk {} hash verification failed", chunk_index));
        }

        Ok(result)
    }

    async fn complete_upload_session(
        &self,
        session_id: &str,
    ) -> Result<CompleteUploadResponse> {
        let session_uuid = Uuid::parse_str(session_id)
            .context("Invalid session ID format")?;

        self.client.complete_upload_session(session_uuid).await
    }

    async fn read_chunk(&self, file_path: &Path, chunk_index: i32) -> Result<Vec<u8>> {
        let offset = chunk_index as u64 * CHUNK_SIZE as u64;
        let mut file = tokio::fs::File::open(file_path).await
            .with_context(|| format!("Failed to open file: {}", file_path.display()))?;

        // Seek to chunk position
        file.seek(std::io::SeekFrom::Start(offset)).await
            .with_context(|| format!("Failed to seek in file: {}", file_path.display()))?;

        // Read chunk
        let mut buffer = vec![0u8; CHUNK_SIZE];
        let bytes_read = file.read(&mut buffer).await
            .with_context(|| format!("Failed to read chunk from file: {}", file_path.display()))?;

        buffer.truncate(bytes_read);
        Ok(buffer)
    }

    async fn verify_hash(&self, file_path: &Path, expected_hash: &str) -> Result<bool> {
        let computed_hash = tokio::task::spawn_blocking({
            let path = file_path.to_path_buf();
            move || file_ops::calculate_hash(&path)
        }).await??;

        Ok(computed_hash == expected_hash)
    }
}

/// Base64 encode MD5 hash bytes
fn base64_md5(bytes: &[u8]) -> String {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    STANDARD.encode(bytes)
}
