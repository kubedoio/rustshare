use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use sync_protocol::{
    DeltaRequest, DeltaResponse, DeviceRegistrationRequest, DeviceRegistrationResponse,
};
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
        let url = self.base_url.join("/api/v1/sync/deltas")?;
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
            .join(&format!("/api/v1/sync/download/{}", file_id))?;
        let response = self
            .client
            .get(url)
            .headers(self.headers()?)
            .send()
            .await?
            .error_for_status()?;

        Ok(response)
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
