use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use sync_protocol::{
    DeltaRequest, DeltaResponse, DeviceRegistrationRequest, DeviceRegistrationResponse,
};
use chrono::{DateTime, Utc};
use url::Url;
use uuid::Uuid;

#[derive(Clone)]
pub struct ApiClient {
    base_url: Url,
    client: reqwest::Client,
    token: Option<String>,
}

impl ApiClient {
    pub fn new(base_url: &str) -> Result<Self> {
        let base_url = Url::parse(base_url)?;
        let client = reqwest::Client::new();
        Ok(Self {
            base_url,
            client,
            token: None,
        })
    }

    pub fn set_token(&mut self, token: String) {
        self.token = Some(token);
    }

    fn headers(&self) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        if let Some(token) = &self.token {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", token))
                    .context("Invalid token format")?,
            );
        }
        Ok(headers)
    }

    pub async fn register_device(
        &self,
        request: DeviceRegistrationRequest,
    ) -> Result<DeviceRegistrationResponse> {
        let url = self.base_url.join("/api/v1/devices/register")?;
        let response = self
            .client
            .post(url)
            .headers(self.headers()?)
            .json(&request)
            .send()
            .await?
            .error_for_status()?;

        Ok(response.json().await?)
    }

    pub async fn fetch_deltas(&self, request: DeltaRequest) -> Result<DeltaResponse> {
        let url = self.base_url.join("/api/v1/sync/delta")?;
        let response = self
            .client
            .get(url)
            .headers(self.headers()?)
            .query(&[("cursor", &request.cursor)])
            .send()
            .await?
            .error_for_status()?;

        Ok(response.json().await?)
    }

    pub async fn download_file(&self, file_id: uuid::Uuid) -> Result<reqwest::Response> {
        let url = self
            .base_url
            .join(&format!("/api/v1/files/{}/content", file_id))?;
        let response = self
            .client
            .get(url)
            .headers(self.headers()?)
            .send()
            .await?
            .error_for_status()?;

        Ok(response)
    }

    pub async fn delete_file(&self, file_id: Uuid) -> Result<()> {
        let url = self.base_url.join(&format!("/api/v1/files/{}", file_id))?;
        self.client
            .delete(url)
            .headers(self.headers()?)
            .send()
            .await
            .context("Failed to delete file")?
            .error_for_status()
            .context("Server returned error when deleting file")?;

        Ok(())
    }

    // ============================================================================
    // Upload Session APIs
    // ============================================================================

    pub async fn create_upload_session(
        &self,
        request: CreateUploadSessionRequest,
    ) -> Result<CreateUploadSessionResponse> {
        let url = self.base_url.join("/api/v1/uploads/sessions")?;
        let response = self
            .client
            .post(url)
            .headers(self.headers()?)
            .json(&request)
            .send()
            .await
            .context("Failed to create upload session")?
            .error_for_status()
            .context("Server returned error when creating upload session")?;

        Ok(response.json().await
            .context("Failed to parse create session response")?)
    }

    pub async fn upload_chunk(
        &self,
        session_id: Uuid,
        chunk_index: u32,
        chunk_data: Vec<u8>,
        md5_hash: Option<String>,
    ) -> Result<UploadChunkResponse> {
        let url = self.base_url
            .join(&format!("/api/v1/uploads/sessions/{}/chunks/{}", session_id, chunk_index))?;

        let mut request = self
            .client
            .put(url)
            .headers(self.headers()?)
            .header(CONTENT_TYPE, "application/octet-stream")
            .body(chunk_data);

        if let Some(md5) = md5_hash {
            let content_md5 = HeaderName::from_static("content-md5");
            request = request.header(content_md5, md5);
        }

        let response = request
            .send()
            .await
            .with_context(|| format!("Failed to upload chunk {}", chunk_index))?
            .error_for_status()
            .with_context(|| format!("Server returned error for chunk {}", chunk_index))?;

        Ok(response.json().await
            .with_context(|| format!("Failed to parse chunk {} response", chunk_index))?)
    }

    pub async fn complete_upload_session(
        &self,
        session_id: Uuid,
    ) -> Result<CompleteUploadResponse> {
        let url = self.base_url
            .join(&format!("/api/v1/uploads/sessions/{}/complete", session_id))?;

        let response = self
            .client
            .post(url)
            .headers(self.headers()?)
            .send()
            .await
            .context("Failed to complete upload session")?
            .error_for_status()
            .context("Server returned error when completing upload")?;

        Ok(response.json().await
            .context("Failed to parse complete upload response")?)
    }

    /// List files from the server
    /// 
    /// Uses /api/v1/files endpoint to get all files (works around broken delta endpoint)
    pub async fn list_files(&self) -> Result<Vec<RemoteFile>> {
        let url = self.base_url.join("/api/v1/files")?;
        let response = self
            .client
            .get(url)
            .headers(self.headers()?)
            .send()
            .await
            .context("Failed to list files")?
            .error_for_status()
            .context("Server returned error when listing files")?;
        
        let files: Vec<RemoteFile> = response.json().await
            .context("Failed to parse files list response")?;
        
        Ok(files)
    }

    pub async fn get_folder_tree(&self) -> Result<RemoteFolderTree> {
        let url = self.base_url.join("/api/v1/folders/tree")?;
        let response = self
            .client
            .get(url)
            .headers(self.headers()?)
            .send()
            .await
            .context("Failed to fetch folder tree")?
            .error_for_status()
            .context("Server returned error when fetching folder tree")?;

        Ok(response
            .json()
            .await
            .context("Failed to parse folder tree response")?)
    }

    pub async fn create_folder(
        &self,
        name: &str,
        parent_folder_id: Option<Uuid>,
    ) -> Result<RemoteFolder> {
        let url = self.base_url.join("/api/v1/folders")?;
        let response = self
            .client
            .post(url)
            .headers(self.headers()?)
            .json(&CreateFolderRequest {
                name: name.to_string(),
                parent_folder_id,
            })
            .send()
            .await
            .context("Failed to create folder")?
            .error_for_status()
            .context("Server returned error when creating folder")?;

        Ok(response
            .json()
            .await
            .context("Failed to parse create folder response")?)
    }

    pub async fn delete_folder(&self, folder_id: Uuid) -> Result<()> {
        let url = self
            .base_url
            .join(&format!("/api/v1/folders/{}", folder_id))?;
        self.client
            .delete(url)
            .headers(self.headers()?)
            .send()
            .await
            .context("Failed to delete folder")?
            .error_for_status()
            .context("Server returned error when deleting folder")?;

        Ok(())
    }

    // ============================================================================
    // Internal Helpers
    // ============================================================================

    pub fn http_client(&self) -> &reqwest::Client {
        &self.client
    }

    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    pub fn get_token(&self) -> Option<&str> {
        self.token.as_deref()
    }
}

// ============================================================================
// Request/Response Types
// ============================================================================

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

fn default_chunk_size() -> u64 {
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
    pub content_hash: String,
}

/// File info from the files list endpoint
#[derive(Debug, Deserialize)]
pub struct RemoteFile {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub content_hash: String,
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        extract::{Path, State},
        http::{header::AUTHORIZATION, HeaderMap, StatusCode},
        routing::get,
        Router,
    };
    use std::{net::SocketAddr, sync::Arc};
    use tokio::net::TcpListener;

    #[derive(Clone, Default)]
    struct DownloadRouteState {
        hits: Arc<tokio::sync::Mutex<Vec<Uuid>>>,
    }

    async fn start_download_test_server(state: DownloadRouteState) -> SocketAddr {
        async fn download_file_content(
            State(state): State<DownloadRouteState>,
            Path(file_id): Path<Uuid>,
            headers: HeaderMap,
        ) -> (StatusCode, Body) {
            assert_eq!(
                headers.get(AUTHORIZATION).and_then(|value| value.to_str().ok()),
                Some("Bearer test-token")
            );
            state.hits.lock().await.push(file_id);
            (StatusCode::OK, Body::from("note body"))
        }

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/api/v1/files/{id}/content", get(download_file_content))
            .with_state(state);

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        addr
    }

    #[tokio::test]
    async fn download_file_uses_file_content_endpoint() {
        let state = DownloadRouteState::default();
        let addr = start_download_test_server(state.clone()).await;
        let mut client = ApiClient::new(&format!("http://{}", addr)).unwrap();
        client.set_token("test-token".to_string());

        let file_id = Uuid::new_v4();
        let response = client.download_file(file_id).await.unwrap();
        let body = response.bytes().await.unwrap();

        assert_eq!(body.as_ref(), b"note body");
        assert_eq!(state.hits.lock().await.as_slice(), &[file_id]);
    }
}
