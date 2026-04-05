use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde::{Deserialize, Serialize};
use sync_protocol::{DeltaRequest, DeltaResponse, DeviceRegistrationRequest, DeviceRegistrationResponse};
use url::Url;

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

    pub async fn register_device(&self, request: DeviceRegistrationRequest) -> Result<DeviceRegistrationResponse> {
        let url = self.base_url.join("/api/v1/devices/register")?;
        let response = self.client.post(url)
            .headers(self.headers()?)
            .json(&request)
            .send().await?
            .error_for_status()?;
        
        Ok(response.json().await?)
    }

    pub async fn fetch_deltas(&self, request: DeltaRequest) -> Result<DeltaResponse> {
        let url = self.base_url.join("/api/v1/sync/deltas")?;
        let response = self.client.get(url)
            .headers(self.headers()?)
            .query(&[("cursor", request.cursor)])
            .send().await?
            .error_for_status()?;
        
        Ok(response.json().await?)
    }

    pub async fn download_file(&self, file_id: uuid::Uuid) -> Result<reqwest::Response> {
        let url = self.base_url.join(&format!("/api/v1/sync/download/{}", file_id))?;
        let response = self.client.get(url)
            .headers(self.headers()?)
            .send().await?
            .error_for_status()?;
        
        Ok(response)
    }
}
