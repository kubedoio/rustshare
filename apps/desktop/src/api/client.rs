//! HTTP client for RustShare server API

use anyhow::Result;
use reqwest::{Client, Method, RequestBuilder, StatusCode};
use serde::Deserialize;
use std::time::Duration;

use uuid::Uuid;

use crate::config::Config;

/// API client for RustShare server
#[derive(Clone)]
pub struct ApiClient {
    client: Client,
    pub base_url: String,
    token: Option<String>,
}

/// API error types
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Authentication required")]
    Unauthorized,
    
    #[error("Access denied")]
    Forbidden,
    
    #[error("Resource not found")]
    NotFound,
    
    #[error("Conflict: {0}")]
    Conflict(String),
    
    #[error("Rate limit exceeded, retry after {0}s")]
    RateLimited(u64),
    
    #[error("Server error: {0}")]
    ServerError(String),
    
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Other error: {0}")]
    Other(String),
}

/// API response wrapper
#[derive(Debug, Deserialize)]
pub struct ApiResponse<T> {
    pub data: T,
}

/// File metadata response
#[derive(Debug, Deserialize, Clone)]
pub struct FileMetadata {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub size: i64,
    pub mime_type: String,
    pub content_hash: String,
    pub current_version: i32,
    pub parent_folder_id: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

/// Folder metadata response
#[derive(Debug, Deserialize, Clone)]
pub struct FolderMetadata {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub parent_id: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

/// Delta item from server
#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeltaItem {
    FileCreated {
        event_id: Uuid,
        timestamp: String,
        file_id: Uuid,
        name: String,
        path: String,
        parent_id: Option<Uuid>,
        size: i64,
        mime_type: String,
        content_hash: String,
        version_id: Uuid,
    },
    FileModified {
        event_id: Uuid,
        timestamp: String,
        file_id: Uuid,
        name: String,
        path: String,
        size: i64,
        mime_type: String,
        content_hash: String,
        version_id: Uuid,
        version_number: i32,
    },
    FileRenamed {
        event_id: Uuid,
        timestamp: String,
        file_id: Uuid,
        old_name: String,
        new_name: String,
        old_path: String,
        new_path: String,
    },
    FileMoved {
        event_id: Uuid,
        timestamp: String,
        file_id: Uuid,
        name: String,
        old_parent_id: Option<Uuid>,
        new_parent_id: Option<Uuid>,
        old_path: String,
        new_path: String,
    },
    FileDeleted {
        event_id: Uuid,
        timestamp: String,
        file_id: Uuid,
        name: String,
        path: String,
        parent_id: Option<Uuid>,
    },
    FileRestored {
        event_id: Uuid,
        timestamp: String,
        file_id: Uuid,
        name: String,
        path: String,
        parent_id: Option<Uuid>,
    },
    FolderCreated {
        event_id: Uuid,
        timestamp: String,
        folder_id: Uuid,
        name: String,
        path: String,
        parent_id: Option<Uuid>,
    },
    FolderRenamed {
        event_id: Uuid,
        timestamp: String,
        folder_id: Uuid,
        old_name: String,
        new_name: String,
        old_path: String,
        new_path: String,
    },
    FolderMoved {
        event_id: Uuid,
        timestamp: String,
        folder_id: Uuid,
        name: String,
        old_parent_id: Option<Uuid>,
        new_parent_id: Option<Uuid>,
        old_path: String,
        new_path: String,
    },
    FolderDeleted {
        event_id: Uuid,
        timestamp: String,
        folder_id: Uuid,
        name: String,
        path: String,
        parent_id: Option<Uuid>,
    },
    FolderRestored {
        event_id: Uuid,
        timestamp: String,
        folder_id: Uuid,
        name: String,
        path: String,
        parent_id: Option<Uuid>,
    },
}

/// Delta response from server
#[derive(Debug, Deserialize)]
pub struct DeltaResponse {
    pub items: Vec<DeltaItem>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

/// Upload URL response
#[derive(Debug, Deserialize)]
pub struct UploadUrlResponse {
    pub upload_url: String,
    pub file_id: Uuid,
}

impl ApiClient {
    /// Create a new API client from configuration
    pub fn new(config: &Config) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(10))
            .build()?;

        Ok(Self {
            client,
            base_url: config.server_url.clone(),
            token: None,
        })
    }

    /// Create client with authentication token
    pub fn with_token(config: &Config, token: String) -> Result<Self> {
        let mut client = Self::new(config)?;
        client.token = Some(token);
        Ok(client)
    }

    /// Set authentication token
    pub fn set_token(&mut self, token: String) {
        self.token = Some(token);
    }

    /// Check if client is authenticated
    pub fn is_authenticated(&self) -> bool {
        self.token.is_some()
    }

    // ====================================================================
    // HTTP Helpers
    // ====================================================================

    fn build_request(&self, method: Method, path: &str) -> RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.client.request(method, &url);
        
        if let Some(token) = &self.token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }
        
        req.header("User-Agent", format!("rustshare-desktop/{}" , crate::VERSION))
    }

    async fn handle_response<T: for<'de> Deserialize<'de>>(
        &self,
        response: reqwest::Response,
    ) -> std::result::Result<T, ApiError> {
        let status = response.status();
        
        match status {
            StatusCode::OK | StatusCode::CREATED | StatusCode::NO_CONTENT => {
                if status == StatusCode::NO_CONTENT {
                    // Parse empty response as empty JSON
                    serde_json::from_str("{}").map_err(ApiError::Serialization)
                } else {
                    response.json().await.map_err(ApiError::Network)
                }
            }
            StatusCode::UNAUTHORIZED => Err(ApiError::Unauthorized),
            StatusCode::FORBIDDEN => Err(ApiError::Forbidden),
            StatusCode::NOT_FOUND => Err(ApiError::NotFound),
            StatusCode::CONFLICT => {
                let text = response.text().await.unwrap_or_default();
                Err(ApiError::Conflict(text))
            }
            StatusCode::TOO_MANY_REQUESTS => {
                let retry_after = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(60);
                Err(ApiError::RateLimited(retry_after))
            }
            _ if status.is_server_error() => {
                let text = response.text().await.unwrap_or_default();
                Err(ApiError::ServerError(text))
            }
            _ => {
                let text = response.text().await.unwrap_or_default();
                Err(ApiError::Other(format!("HTTP {}: {}", status, text)))
            }
        }
    }

    // ====================================================================
    // File Operations
    // ====================================================================

    /// Get file metadata
    pub async fn get_file(&self, file_id: Uuid) -> std::result::Result<FileMetadata, ApiError> {
        let response = self
            .build_request(Method::GET, &format!("/api/files/{}", file_id))
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Download file content
    pub async fn download_file(&self, file_id: Uuid) -> std::result::Result<bytes::Bytes, ApiError> {
        let response = self
            .build_request(Method::GET, &format!("/api/files/{}/download", file_id))
            .send()
            .await?;

        match response.status() {
            StatusCode::OK => response.bytes().await.map_err(ApiError::Network),
            StatusCode::UNAUTHORIZED => Err(ApiError::Unauthorized),
            StatusCode::FORBIDDEN => Err(ApiError::Forbidden),
            StatusCode::NOT_FOUND => Err(ApiError::NotFound),
            status if status.is_server_error() => {
                let text = response.text().await.unwrap_or_default();
                Err(ApiError::ServerError(text))
            }
            status => {
                let text = response.text().await.unwrap_or_default();
                Err(ApiError::Other(format!("HTTP {}: {}", status, text)))
            }
        }
    }

    /// Delete a file
    pub async fn delete_file(&self, file_id: Uuid) -> std::result::Result<(), ApiError> {
        let response = self
            .build_request(Method::DELETE, &format!("/api/files/{}", file_id))
            .send()
            .await?;

        self.handle_response::<serde_json::Value>(response).await?;
        Ok(())
    }

    /// Rename a file
    pub async fn rename_file(
        &self,
        file_id: Uuid,
        new_name: &str,
    ) -> std::result::Result<FileMetadata, ApiError> {
        let response = self
            .build_request(Method::PUT, &format!("/api/files/{}", file_id))
            .json(&serde_json::json!({ "name": new_name }))
            .send()
            .await?;

        self.handle_response(response).await
    }

    // ====================================================================
    // Folder Operations
    // ====================================================================

    /// Get folder metadata
    pub async fn get_folder(&self, folder_id: Uuid) -> std::result::Result<FolderMetadata, ApiError> {
        let response = self
            .build_request(Method::GET, &format!("/api/folders/{}", folder_id))
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// List folder contents
    pub async fn list_folder(
        &self,
        folder_id: Option<Uuid>,
    ) -> std::result::Result<(Vec<FolderMetadata>, Vec<FileMetadata>), ApiError> {
        let path = match folder_id {
            Some(id) => format!("/api/folders/{}/list", id),
            None => "/api/folders/root/list".to_string(),
        };

        let response = self
            .build_request(Method::GET, &path)
            .send()
            .await?;

        #[derive(Deserialize)]
        struct ListResponse {
            folders: Vec<FolderMetadata>,
            files: Vec<FileMetadata>,
        }

        let resp: ListResponse = self.handle_response(response).await?;
        Ok((resp.folders, resp.files))
    }

    /// Create a folder
    pub async fn create_folder(
        &self,
        name: &str,
        parent_id: Option<Uuid>,
    ) -> std::result::Result<FolderMetadata, ApiError> {
        let response = self
            .build_request(Method::POST, "/api/folders")
            .json(&serde_json::json!({
                "name": name,
                "parent_folder_id": parent_id,
            }))
            .send()
            .await?;

        self.handle_response(response).await
    }

    // ====================================================================
    // Sync Operations
    // ====================================================================

    /// Get delta changes since cursor
    pub async fn get_delta(
        &self,
        cursor: &str,
        limit: Option<usize>,
    ) -> std::result::Result<DeltaResponse, ApiError> {
        let mut path = format!("/api/sync/delta?cursor={}", urlencoding::encode(cursor));
        
        if let Some(limit) = limit {
            path.push_str(&format!("&limit={}", limit));
        }

        let response = self
            .build_request(Method::GET, &path)
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Update cursor position
    pub async fn update_cursor(
        &self,
        cursor: &str,
        last_event_id: Uuid,
    ) -> std::result::Result<(), ApiError> {
        let response = self
            .build_request(Method::POST, "/api/sync/cursor")
            .json(&serde_json::json!({
                "cursor": cursor,
                "last_event_id": last_event_id,
            }))
            .send()
            .await?;

        self.handle_response::<serde_json::Value>(response).await?;
        Ok(())
    }

    // ====================================================================
    // User Operations
    // ====================================================================

    /// Get current user info
    pub async fn get_current_user(&self) -> std::result::Result<UserInfo, ApiError> {
        let response = self
            .build_request(Method::GET, "/api/users/me")
            .send()
            .await?;

        self.handle_response(response).await
    }
}

/// User information
#[derive(Debug, Deserialize)]
pub struct UserInfo {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_error_display() {
        let err = ApiError::Unauthorized;
        assert_eq!(err.to_string(), "Authentication required");
        
        let err = ApiError::RateLimited(60);
        assert_eq!(err.to_string(), "Rate limit exceeded, retry after 60s");
    }
}
