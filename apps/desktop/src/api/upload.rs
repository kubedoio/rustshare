//! Resumable file upload support
//!
//! Provides chunked upload with resume capability for reliable
//! transfer of large files over unreliable networks.

use anyhow::Result;
use bytes::Bytes;
use reqwest::{Client, StatusCode};
use std::path::Path;
use std::time::Duration;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tracing::{debug, error, info, warn};

use crate::api::client::ApiClient;
use crate::db::Database;

/// Default chunk size: 5MB
pub const DEFAULT_CHUNK_SIZE: usize = 5 * 1024 * 1024;

/// Minimum chunk size: 256KB
const MIN_CHUNK_SIZE: usize = 256 * 1024;

/// Maximum chunk size: 100MB
const MAX_CHUNK_SIZE: usize = 100 * 1024 * 1024;

/// Upload progress callback
pub type ProgressCallback = Box<dyn Fn(u64, u64) + Send + Sync>;

/// Resumable uploader
pub struct ResumableUpload {
    client: ApiClient,
    db: Database,
    chunk_size: usize,
    progress_callback: Option<ProgressCallback>,
}

/// Upload result
#[derive(Debug)]
pub struct UploadResult {
    pub file_id: uuid::Uuid,
    pub bytes_uploaded: u64,
    pub completed: bool,
}

/// Upload session
#[derive(Debug)]
struct UploadSession {
    #[allow(dead_code)]
    file_id: uuid::Uuid,
    upload_url: Option<String>,
    bytes_uploaded: u64,
    #[allow(dead_code)]
    total_bytes: u64,
    chunk_size: usize,
}

impl ResumableUpload {
    /// Create a new resumable uploader
    pub fn new(client: ApiClient, db: Database, chunk_size: usize) -> Self {
        let chunk_size = chunk_size.clamp(MIN_CHUNK_SIZE, MAX_CHUNK_SIZE);
        
        Self {
            client,
            db,
            chunk_size,
            progress_callback: None,
        }
    }

    /// Set progress callback
    pub fn with_progress_callback(mut self, callback: ProgressCallback) -> Self {
        self.progress_callback = Some(callback);
        self
    }

    /// Upload a file
    /// 
    /// If the file has been partially uploaded before, this will resume
    /// from the last checkpoint.
    pub async fn upload(
        &self,
        local_path: &Path,
        parent_folder_id: Option<uuid::Uuid>,
    ) -> Result<UploadResult> {
        let file_name = local_path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("Invalid file path"))?
            .to_string_lossy();

        // Get file size
        let metadata = tokio::fs::metadata(local_path).await?;
        let total_bytes = metadata.len();

        // Check for existing upload progress
        let file_id = self.get_or_create_file_id(local_path, parent_folder_id, &file_name).await?;
        
        let mut session = self.load_or_create_session(file_id, total_bytes).await?;

        // Open file and seek to resume position
        let mut file = File::open(local_path).await?;
        
        if session.bytes_uploaded > 0 {
            info!("Resuming upload of {} at {} bytes", file_name, session.bytes_uploaded);
            file.seek(std::io::SeekFrom::Start(session.bytes_uploaded)).await?;
        }

        // Upload chunks
        let mut buffer = vec![0u8; session.chunk_size];
        let completed;

        loop {
            let bytes_read = file.read(&mut buffer).await?;
            
            if bytes_read == 0 {
                // Upload complete
                completed = true;
                break;
            }

            let chunk = Bytes::copy_from_slice(&buffer[..bytes_read]);
            
            match self.upload_chunk(file_id, chunk, session.bytes_uploaded).await {
                Ok(_) => {
                    session.bytes_uploaded += bytes_read as u64;
                    
                    // Save progress
                    self.db.save_upload_progress(
                        file_id,
                        session.upload_url.as_deref(),
                        session.bytes_uploaded as i64,
                        total_bytes as i64,
                        session.chunk_size as i64,
                    )?;

                    // Report progress
                    if let Some(ref callback) = self.progress_callback {
                        callback(session.bytes_uploaded, total_bytes);
                    }
                }
                Err(e) => {
                    error!("Upload chunk failed: {}", e);
                    return Err(e);
                }
            }

            // Check if upload is complete
            if session.bytes_uploaded >= total_bytes {
                completed = true;
                break;
            }
        }

        // Clear progress on completion
        if session.bytes_uploaded >= total_bytes {
            self.db.clear_upload_progress(file_id)?;
            info!("Upload completed: {} ({} bytes)", file_name, session.bytes_uploaded);
        }

        Ok(UploadResult {
            file_id,
            bytes_uploaded: session.bytes_uploaded,
            completed,
        })
    }

    /// Get existing file ID or create new upload
    async fn get_or_create_file_id(
        &self,
        _local_path: &Path,
        _parent_folder_id: Option<uuid::Uuid>,
        file_name: &str,
    ) -> Result<uuid::Uuid> {
        // Check for existing upload progress
        // For now, we generate a new file ID for each upload
        // In a full implementation, we'd check if this file already exists on server
        
        let file_id = uuid::Uuid::new_v4();
        debug!("Generated new file ID: {} for {}", file_id, file_name);
        
        Ok(file_id)
    }

    /// Load existing upload session or create new one
    async fn load_or_create_session(
        &self,
        file_id: uuid::Uuid,
        total_bytes: u64,
    ) -> Result<UploadSession> {
        // Check database for existing progress
        if let Some((upload_url, bytes_uploaded, _, chunk_size)) = 
            self.db.get_upload_progress(file_id)? {
            info!("Found existing upload session: {} bytes uploaded", bytes_uploaded);
            return Ok(UploadSession {
                file_id,
                upload_url,
                bytes_uploaded: bytes_uploaded as u64,
                total_bytes,
                chunk_size: chunk_size as usize,
            });
        }

        // Create new session
        Ok(UploadSession {
            file_id,
            upload_url: None,
            bytes_uploaded: 0,
            total_bytes,
            chunk_size: self.chunk_size,
        })
    }

    /// Upload a single chunk
    async fn upload_chunk(
        &self,
        file_id: uuid::Uuid,
        chunk: Bytes,
        offset: u64,
    ) -> Result<()> {
        // For now, this is a simplified implementation
        // In production, this would use the server's resumable upload API
        
        let client = Client::new();
        let url = format!("{}/api/files/{}/chunks", self.client.base_url, file_id);
        
        let response = client
            .put(&url)
            .header("Content-Range", format!("bytes {}-{}/{}", 
                offset, 
                offset + chunk.len() as u64 - 1,
                offset + chunk.len() as u64))
            .body(chunk)
            .timeout(Duration::from_secs(300))
            .send()
            .await?;

        match response.status() {
            StatusCode::OK | StatusCode::CREATED | StatusCode::PERMANENT_REDIRECT => Ok(()),
            StatusCode::CONFLICT => {
                warn!("Chunk already uploaded at offset {}", offset);
                Ok(())
            }
            _ => {
                let text = response.text().await.unwrap_or_default();
                anyhow::bail!("Chunk upload failed: {}", text);
            }
        }
    }

    /// Cancel an in-progress upload
    pub async fn cancel_upload(&self, file_id: uuid::Uuid) -> Result<()> {
        self.db.clear_upload_progress(file_id)?;
        
        // Optionally notify server to clean up
        debug!("Cancelled upload for file {}", file_id);
        
        Ok(())
    }
}

/// Simple non-resumable upload for small files
pub async fn upload_simple(
    client: &ApiClient,
    local_path: &Path,
    parent_folder_id: Option<uuid::Uuid>,
) -> Result<uuid::Uuid> {
    let file_name = local_path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("Invalid file path"))?
        .to_string_lossy();

    // Read file content
    let content = tokio::fs::read(local_path).await?;
    
    // Detect MIME type
    let mime_type = mime_guess::from_path(local_path)
        .first_or_octet_stream()
        .to_string();

    // Build multipart form
    let form = reqwest::multipart::Form::new()
        .text("name", file_name.to_string())
        .part(
            "file",
            reqwest::multipart::Part::bytes(content)
                .file_name(file_name.to_string())
                .mime_str(&mime_type)?,
        );

    // Add parent folder ID if specified
    let form = if let Some(folder_id) = parent_folder_id {
        form.text("parent_folder_id", folder_id.to_string())
    } else {
        form
    };

    // Send upload request using the ApiClient's base_url
    let url = format!("{}/api/files/upload", client.base_url);
    
    let response = reqwest::Client::new()
        .post(&url)
        .multipart(form)
        .timeout(Duration::from_secs(300))
        .send()
        .await?;

    if !response.status().is_success() {
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("Upload failed: {}", text);
    }

    #[derive(Deserialize)]
    struct UploadResponse {
        id: uuid::Uuid,
    }

    let result: UploadResponse = response.json().await?;
    
    info!("Uploaded {} as file {}", file_name, result.id);
    
    Ok(result.id)
}

// Extension trait to access base_url from ApiClient
#[allow(dead_code)]
trait ApiClientExt {
    fn base_url(&self) -> &str;
}

use serde::Deserialize;
