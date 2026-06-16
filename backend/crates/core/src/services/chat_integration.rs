//! Chat integration service for RustShare.
//!
//! This service handles:
//! - Generating signed webhook events for file/share changes
//! - Verifying incoming chat integration requests
//! - Link unfurl requests with permission checking
//! - Dispatching events to registered chat webhook URLs

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info, warn};
use uuid::Uuid;

use rustshare_crypto::WebhookSigner;

use crate::domain::{File, Folder, Share, SharePermissions, UserId};
use crate::events::{AggregateType, Event, EventBroadcaster, EventType, ShareRevokedPayload};
use crate::services::ShareError;

/// Errors that can occur during chat integration operations.
#[derive(Debug, Error)]
pub enum ChatIntegrationError {
    /// Share not found.
    #[error("Share not found")]
    ShareNotFound,

    /// File not found.
    #[error("File not found")]
    FileNotFound,

    /// Folder not found.
    #[error("Folder not found")]
    FolderNotFound,

    /// Permission denied.
    #[error("Permission denied")]
    PermissionDenied,

    /// Invalid webhook URL.
    #[error("Invalid webhook URL: {0}")]
    InvalidWebhookUrl(String),

    /// Signature verification failed.
    #[error("Signature verification failed")]
    SignatureVerificationFailed,

    /// Webhook dispatch failed.
    #[error("Webhook dispatch failed: {0}")]
    DispatchFailed(String),

    /// Database operation failed.
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Serialization error.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Internal error.
    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

impl From<ShareError> for ChatIntegrationError {
    fn from(err: ShareError) -> Self {
        match err {
            ShareError::ShareNotFound(_) => ChatIntegrationError::ShareNotFound,
            ShareError::ShareNotFoundByToken(_) => ChatIntegrationError::ShareNotFound,
            ShareError::FileNotFound(_) => ChatIntegrationError::FileNotFound,
            ShareError::FolderNotFound(_) => ChatIntegrationError::ShareNotFound,
            ShareError::PermissionDenied { .. } => ChatIntegrationError::PermissionDenied,
            ShareError::Revoked => ChatIntegrationError::ShareNotFound,
            ShareError::Expired => ChatIntegrationError::ShareNotFound,
            _ => ChatIntegrationError::DispatchFailed(err.to_string()),
        }
    }
}

/// Types of chat events that can be dispatched.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChatEventType {
    /// A share has been revoked.
    ShareRevoked,
    /// A file has been updated (modified, renamed, moved).
    FileUpdated,
    /// A file has been deleted.
    FileDeleted,
}

/// Payload for chat events.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "type", content = "data")]
pub enum ChatEventPayload {
    ShareRevoked {
        share_id: Uuid,
        resource_id: Uuid,
        resource_type: String,
        revoked_by: Uuid,
        revoked_at: DateTime<Utc>,
    },
    FileUpdated {
        file_id: Uuid,
        name: String,
        mime_type: String,
        size: i64,
        updated_by: Uuid,
        updated_at: DateTime<Utc>,
    },
    FileDeleted {
        file_id: Uuid,
        name: String,
        deleted_by: Uuid,
        deleted_at: DateTime<Utc>,
    },
}

/// A signed chat event for webhook dispatch.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ChatEvent {
    pub event_type: ChatEventType,
    pub timestamp: DateTime<Utc>,
    pub signature: String,
    pub payload: ChatEventPayload,
}

impl ChatEvent {
    /// Create a new chat event with a signature.
    pub fn new(
        event_type: ChatEventType,
        payload: ChatEventPayload,
        signer: &WebhookSigner,
    ) -> Result<Self, ChatIntegrationError> {
        let timestamp = Utc::now();

        // Create canonical payload for signing
        let payload_bytes = serde_json::to_vec(&payload)
            .map_err(|e| ChatIntegrationError::Serialization(e.to_string()))?;

        // Sign with timestamp
        let signature = signer
            .sign_with_timestamp(timestamp.timestamp(), &payload_bytes)
            .map_err(|e| ChatIntegrationError::Serialization(e.to_string()))?;

        Ok(Self {
            event_type,
            timestamp,
            signature,
            payload,
        })
    }

    /// Verify the event signature.
    pub fn verify(&self, signer: &WebhookSigner) -> Result<bool, ChatIntegrationError> {
        let payload_bytes = serde_json::to_vec(&self.payload)
            .map_err(|e| ChatIntegrationError::Serialization(e.to_string()))?;

        signer
            .verify(&self.signature, &payload_bytes)
            .map_err(|e| ChatIntegrationError::Serialization(e.to_string()))
    }
}

/// Metadata for link unfurl responses.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UnfurlMetadata {
    pub title: String,
    pub description: Option<String>,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub share_token: String,
    pub mime_type: Option<String>,
    pub size: Option<i64>,
    pub thumbnail_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub password_protected: bool,
    pub permissions: SharePermissions,
}

/// Request to unfurl a RustShare link.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UnfurlRequest {
    pub url: String,
}

/// Response from unfurling a link.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UnfurlResponse {
    pub metadata: UnfurlMetadata,
}

/// Incoming chat event from external chat system.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct IncomingChatEvent {
    pub event_type: String,
    pub timestamp: DateTime<Utc>,
    pub user_id: Option<String>,
    pub data: serde_json::Value,
}

/// Trait for metadata store operations needed by ChatIntegrationService.
#[allow(async_fn_in_trait)]
pub trait MetadataStoreOps: Send + Sync {
    /// Get a share by token.
    async fn get_share_by_token(&self, token: &str) -> anyhow::Result<Option<Share>>;

    /// Find a file by ID.
    async fn find_file_by_id(&self, id: Uuid, owner_id: Uuid) -> anyhow::Result<Option<File>>;

    /// Find a folder by ID.
    async fn find_folder_by_id(&self, id: Uuid, owner_id: Uuid) -> anyhow::Result<Option<Folder>>;

    /// Get user shares for a user.
    async fn get_user_shares(&self, user_id: UserId) -> anyhow::Result<Vec<Share>>;
}

/// Trait for event store operations needed by ChatIntegrationService.
#[allow(async_fn_in_trait)]
pub trait EventStoreOps: Send + Sync {
    /// Append an event to the event store.
    async fn append(&self, event: &Event, broadcaster: &EventBroadcaster) -> anyhow::Result<()>;
}

/// Trait for webhook dispatch operations.
#[async_trait::async_trait]
pub trait WebhookDispatcher: Send + Sync {
    /// Dispatch a webhook event to a URL.
    async fn dispatch(&self, url: &str, event: &ChatEvent) -> std::result::Result<(), String>;
}

/// Default HTTP webhook dispatcher.
#[derive(Debug, Clone)]
pub struct HttpWebhookDispatcher {
    client: reqwest::Client,
    timeout: Duration,
}

impl HttpWebhookDispatcher {
    /// Create a new HTTP webhook dispatcher.
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            timeout: Duration::from_secs(30),
        }
    }

    /// Create with custom timeout.
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            client: reqwest::Client::new(),
            timeout,
        }
    }
}

impl Default for HttpWebhookDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebhookDispatcher for HttpWebhookDispatcher {
    async fn dispatch(&self, url: &str, event: &ChatEvent) -> std::result::Result<(), String> {
        let response = self
            .client
            .post(url)
            .header("Content-Type", "application/json")
            .header("X-RustShare-Event", format!("{:?}", event.event_type))
            .header("X-RustShare-Signature", &event.signature)
            .timeout(self.timeout)
            .json(event)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read body>".to_string());
            Err(format!("HTTP {}: {}", status, body))
        }
    }
}

/// Chat integration service for managing webhook events and link unfurls.
pub struct ChatIntegrationService<M: MetadataStoreOps, E: EventStoreOps, W: WebhookDispatcher> {
    metadata_store: Arc<M>,
    event_store: Arc<E>,
    broadcaster: Arc<EventBroadcaster>,
    signer: WebhookSigner,
    dispatcher: Arc<W>,
    webhook_urls: Vec<String>,
}

impl<M: MetadataStoreOps, E: EventStoreOps, W: WebhookDispatcher> ChatIntegrationService<M, E, W> {
    /// Create a new chat integration service.
    pub fn new(
        metadata_store: Arc<M>,
        event_store: Arc<E>,
        broadcaster: Arc<EventBroadcaster>,
        webhook_secret: impl AsRef<[u8]>,
        dispatcher: Arc<W>,
    ) -> Self {
        Self {
            metadata_store,
            event_store,
            broadcaster,
            signer: WebhookSigner::new(webhook_secret),
            dispatcher,
            webhook_urls: Vec::new(),
        }
    }

    /// Register a webhook URL for event dispatch.
    pub fn register_webhook(&mut self, url: String) {
        if !self.webhook_urls.contains(&url) {
            info!("Registering chat webhook: {}", url);
            self.webhook_urls.push(url);
        }
    }

    /// Unregister a webhook URL.
    pub fn unregister_webhook(&mut self, url: &str) {
        self.webhook_urls.retain(|u| u != url);
    }

    /// Get registered webhook URLs.
    pub fn get_webhook_urls(&self) -> &[String] {
        &self.webhook_urls
    }

    /// Create a signed chat event for a share revocation.
    pub fn create_share_revoked_event(
        &self,
        share: &Share,
        revoked_by: UserId,
    ) -> Result<ChatEvent, ChatIntegrationError> {
        let resource_id = share.file_id.or(share.folder_id).ok_or_else(|| {
            ChatIntegrationError::Serialization("Share has no resource".to_string())
        })?;

        let resource_type = if share.file_id.is_some() {
            "file"
        } else {
            "folder"
        }
        .to_string();

        let payload = ChatEventPayload::ShareRevoked {
            share_id: share.id,
            resource_id,
            resource_type,
            revoked_by,
            revoked_at: Utc::now(),
        };

        ChatEvent::new(ChatEventType::ShareRevoked, payload, &self.signer)
    }

    /// Create a signed chat event for a file update.
    pub fn create_file_updated_event(
        &self,
        file: &File,
        updated_by: UserId,
    ) -> Result<ChatEvent, ChatIntegrationError> {
        let payload = ChatEventPayload::FileUpdated {
            file_id: file.id,
            name: file.name.clone(),
            mime_type: file.mime_type.clone(),
            size: file.size,
            updated_by,
            updated_at: Utc::now(),
        };

        ChatEvent::new(ChatEventType::FileUpdated, payload, &self.signer)
    }

    /// Create a signed chat event for a file deletion.
    pub fn create_file_deleted_event(
        &self,
        file_id: Uuid,
        name: String,
        deleted_by: UserId,
    ) -> Result<ChatEvent, ChatIntegrationError> {
        let payload = ChatEventPayload::FileDeleted {
            file_id,
            name,
            deleted_by,
            deleted_at: Utc::now(),
        };

        ChatEvent::new(ChatEventType::FileDeleted, payload, &self.signer)
    }

    /// Dispatch a chat event to all registered webhooks.
    pub async fn dispatch_event(&self, event: &ChatEvent) -> Vec<(String, Result<(), String>)> {
        let mut results = Vec::new();

        for url in &self.webhook_urls {
            debug!("Dispatching {:?} event to {}", event.event_type, url);

            let result = self.dispatch_with_retry(url, event).await;

            if let Err(ref e) = result {
                warn!("Failed to dispatch to {}: {}", url, e);
            } else {
                info!("Successfully dispatched to {}", url);
            }

            results.push((url.clone(), result));
        }

        results
    }

    /// Dispatch with exponential backoff retry.
    async fn dispatch_with_retry(&self, url: &str, event: &ChatEvent) -> Result<(), String> {
        let max_retries = 3;
        let base_delay_ms = 1000;

        for attempt in 0..max_retries {
            match self.dispatcher.dispatch(url, event).await {
                Ok(()) => return Ok(()),
                Err(e) if attempt < max_retries - 1 => {
                    let delay = Duration::from_millis(base_delay_ms * 2_u64.pow(attempt));
                    warn!(
                        "Webhook dispatch failed (attempt {}/{}), retrying in {:?}: {}",
                        attempt + 1,
                        max_retries,
                        delay,
                        e
                    );
                    tokio::time::sleep(delay).await;
                }
                Err(e) => return Err(e),
            }
        }

        Err("Max retries exceeded".to_string())
    }

    /// Handle a share revocation by dispatching events to chat integrations.
    pub async fn handle_share_revoked(
        &self,
        share: &Share,
        revoked_by: UserId,
    ) -> Result<(), ChatIntegrationError> {
        let event = self.create_share_revoked_event(share, revoked_by)?;

        // Also emit to internal event store for audit trail
        let internal_event = self.create_internal_share_revoked_event(share, revoked_by)?;
        self.event_store
            .append(&internal_event, &self.broadcaster)
            .await
            .map_err(|e| ChatIntegrationError::DispatchFailed(e.to_string()))?;

        // Dispatch to external webhooks
        self.dispatch_event(&event).await;

        Ok(())
    }

    /// Unfurl a RustShare link with permission checking.
    pub async fn unfurl_link(
        &self,
        request: &UnfurlRequest,
        requesting_user_id: Option<UserId>,
    ) -> Result<UnfurlResponse, ChatIntegrationError> {
        // Parse the URL to extract share token
        let share_token = self.extract_share_token(&request.url)?;

        // Get the share
        let share = self
            .metadata_store
            .get_share_by_token(&share_token)
            .await
            .map_err(|e| ChatIntegrationError::DispatchFailed(e.to_string()))?
            .ok_or(ChatIntegrationError::ShareNotFound)?;

        // Check if share is revoked
        if share.revoked_at.is_some() {
            return Err(ChatIntegrationError::ShareNotFound);
        }

        // Check if share is expired
        if share.is_expired() {
            return Err(ChatIntegrationError::ShareNotFound);
        }

        // For private/user shares, check if requesting user has permission
        if let Some(user_id) = requesting_user_id {
            if share.is_user_share() {
                // Check if user is the recipient
                let user_shares = self
                    .metadata_store
                    .get_user_shares(user_id)
                    .await
                    .map_err(ChatIntegrationError::Internal)?;

                let has_access = user_shares.iter().any(|s| s.id == share.id);

                if !has_access {
                    return Err(ChatIntegrationError::PermissionDenied);
                }
            }
            // For public shares, anyone can view (but may need password)
        }

        // Get resource metadata
        let metadata = if let Some(file_id) = share.file_id {
            let file = self
                .metadata_store
                .find_file_by_id(file_id, share.created_by)
                .await
                .map_err(|e| ChatIntegrationError::DispatchFailed(e.to_string()))?
                .ok_or(ChatIntegrationError::FileNotFound)?;

            UnfurlMetadata {
                title: file.name.clone(),
                description: Some(format!("Shared file ({}", format_bytes(file.size))),
                resource_type: "file".to_string(),
                resource_id: file.id,
                share_token: share_token.clone(),
                mime_type: Some(file.mime_type.clone()),
                size: Some(file.size),
                thumbnail_url: None, // Could generate thumbnail URL here
                created_at: file.created_at,
                expires_at: share.expires_at,
                password_protected: share.password_hash.is_some(),
                permissions: share.permissions,
            }
        } else if let Some(folder_id) = share.folder_id {
            let folder = self
                .metadata_store
                .find_folder_by_id(folder_id, share.created_by)
                .await
                .map_err(|e| ChatIntegrationError::DispatchFailed(e.to_string()))?
                .ok_or(ChatIntegrationError::FolderNotFound)?;

            UnfurlMetadata {
                title: folder.name.clone(),
                description: Some("Shared folder".to_string()),
                resource_type: "folder".to_string(),
                resource_id: folder.id,
                share_token: share_token.clone(),
                mime_type: None,
                size: None,
                thumbnail_url: None,
                created_at: folder.created_at,
                expires_at: share.expires_at,
                password_protected: share.password_hash.is_some(),
                permissions: share.permissions,
            }
        } else {
            return Err(ChatIntegrationError::Serialization(
                "Share has no resource".to_string(),
            ));
        };

        Ok(UnfurlResponse { metadata })
    }

    /// Verify an incoming chat event signature.
    pub fn verify_incoming_event(&self, event: &ChatEvent) -> Result<bool, ChatIntegrationError> {
        event.verify(&self.signer)
    }

    /// Process an incoming chat event.
    ///
    /// Verifies the `X-RustShare-Signature` over the raw request body before
    /// deserializing the event. Returns [`ChatIntegrationError::SignatureVerificationFailed`]
    /// when the signature is missing or invalid.
    pub async fn process_incoming_event(
        &self,
        body: &[u8],
        signature: &str,
    ) -> Result<(), ChatIntegrationError> {
        if signature.is_empty() {
            return Err(ChatIntegrationError::SignatureVerificationFailed);
        }

        if !self
            .signer
            .verify(signature, body)
            .map_err(|e| ChatIntegrationError::Serialization(e.to_string()))?
        {
            return Err(ChatIntegrationError::SignatureVerificationFailed);
        }

        let event: IncomingChatEvent = serde_json::from_slice(body)
            .map_err(|e| ChatIntegrationError::Serialization(e.to_string()))?;

        info!(
            "Processing incoming chat event: {} from {}",
            event.event_type,
            event.user_id.as_deref().unwrap_or("anonymous")
        );

        debug!("Received chat event: {:?}", event);

        Ok(())
    }

    /// Extract share token from a RustShare URL.
    fn extract_share_token(&self, url: &str) -> Result<String, ChatIntegrationError> {
        // Expected formats:
        // - https://example.com/share/TOKEN
        // - https://example.com/s/TOKEN
        // - https://example.com/public/share/TOKEN

        let parsed = url::Url::parse(url)
            .map_err(|_| ChatIntegrationError::InvalidWebhookUrl(url.to_string()))?;

        let path_segments: Vec<&str> = parsed
            .path_segments()
            .map(|s| s.collect())
            .unwrap_or_default();

        // Look for share token in path
        if let Some(pos) = path_segments.iter().position(|&s| s == "share" || s == "s") {
            if let Some(token) = path_segments.get(pos + 1) {
                return Ok(token.to_string());
            }
        }

        Err(ChatIntegrationError::InvalidWebhookUrl(
            "No share token found in URL".to_string(),
        ))
    }

    fn create_internal_share_revoked_event(
        &self,
        share: &Share,
        revoked_by: UserId,
    ) -> Result<Event, ChatIntegrationError> {
        let payload = ShareRevokedPayload {
            share_id: share.id,
            file_id: share.resource_id().unwrap_or(share.id),
            revoked_by,
        };

        Ok(Event::new(
            EventType::ShareRevoked,
            share.id,
            AggregateType::Share,
            serde_json::to_value(&payload)
                .map_err(|e| ChatIntegrationError::Serialization(e.to_string()))?,
            revoked_by,
        ))
    }
}

/// Format bytes to human-readable string.
fn format_bytes(bytes: i64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    format!("{:.1} {}", size, UNITS[unit_idx])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct MockMetadataStore {
        shares: Mutex<Vec<Share>>,
        files: Mutex<Vec<File>>,
        folders: Mutex<Vec<Folder>>,
    }

    impl MockMetadataStore {
        fn new() -> Self {
            Self {
                shares: Mutex::new(Vec::new()),
                files: Mutex::new(Vec::new()),
                folders: Mutex::new(Vec::new()),
            }
        }
    }

    impl MetadataStoreOps for MockMetadataStore {
        async fn get_share_by_token(&self, token: &str) -> anyhow::Result<Option<Share>> {
            Ok(self
                .shares
                .lock()
                .unwrap()
                .iter()
                .find(|s| s.share_token.as_deref() == Some(token))
                .cloned())
        }

        async fn find_file_by_id(&self, id: Uuid, _owner_id: Uuid) -> anyhow::Result<Option<File>> {
            Ok(self
                .files
                .lock()
                .unwrap()
                .iter()
                .find(|f| f.id == id)
                .cloned())
        }

        async fn find_folder_by_id(
            &self,
            id: Uuid,
            _owner_id: Uuid,
        ) -> anyhow::Result<Option<Folder>> {
            Ok(self
                .folders
                .lock()
                .unwrap()
                .iter()
                .find(|f| f.id == id)
                .cloned())
        }

        async fn get_user_shares(&self, _user_id: UserId) -> anyhow::Result<Vec<Share>> {
            Ok(self.shares.lock().unwrap().clone())
        }
    }

    struct MockEventStore {
        events: Mutex<Vec<Event>>,
    }

    impl MockEventStore {
        fn new() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
            }
        }
    }

    impl EventStoreOps for MockEventStore {
        async fn append(
            &self,
            event: &Event,
            _broadcaster: &EventBroadcaster,
        ) -> anyhow::Result<()> {
            self.events.lock().unwrap().push(event.clone());
            Ok(())
        }
    }

    struct MockWebhookDispatcher {
        calls: Mutex<Vec<(String, ChatEvent)>>,
        should_fail: bool,
    }

    impl MockWebhookDispatcher {
        fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                should_fail: false,
            }
        }

        #[allow(dead_code)]
        fn with_failure() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                should_fail: true,
            }
        }
    }

    #[async_trait::async_trait]
    impl WebhookDispatcher for MockWebhookDispatcher {
        async fn dispatch(&self, url: &str, event: &ChatEvent) -> std::result::Result<(), String> {
            if self.should_fail {
                return Err("Mock dispatch failure".to_string());
            }
            self.calls
                .lock()
                .unwrap()
                .push((url.to_string(), event.clone()));
            Ok(())
        }
    }

    #[test]
    fn test_chat_event_creation() {
        let signer = WebhookSigner::new("test_secret");
        let payload = ChatEventPayload::ShareRevoked {
            share_id: Uuid::new_v4(),
            resource_id: Uuid::new_v4(),
            resource_type: "file".to_string(),
            revoked_by: Uuid::new_v4(),
            revoked_at: Utc::now(),
        };

        let event = ChatEvent::new(ChatEventType::ShareRevoked, payload, &signer).unwrap();

        assert_eq!(event.event_type, ChatEventType::ShareRevoked);
        assert!(!event.signature.is_empty());
        assert!(event.verify(&signer).unwrap());
    }

    #[test]
    fn test_extract_share_token() {
        let metadata = Arc::new(MockMetadataStore::new());
        let events = Arc::new(MockEventStore::new());
        let broadcaster = Arc::new(EventBroadcaster::new(100));
        let dispatcher = Arc::new(MockWebhookDispatcher::new());

        let service =
            ChatIntegrationService::new(metadata, events, broadcaster, "test_secret", dispatcher);

        assert_eq!(
            service
                .extract_share_token("https://example.com/share/abc123")
                .unwrap(),
            "abc123"
        );
        assert_eq!(
            service
                .extract_share_token("https://example.com/s/xyz789")
                .unwrap(),
            "xyz789"
        );
        assert_eq!(
            service
                .extract_share_token("https://example.com/public/share/token456")
                .unwrap(),
            "token456"
        );
    }

    #[test]
    fn test_extract_share_token_invalid() {
        let metadata = Arc::new(MockMetadataStore::new());
        let events = Arc::new(MockEventStore::new());
        let broadcaster = Arc::new(EventBroadcaster::new(100));
        let dispatcher = Arc::new(MockWebhookDispatcher::new());

        let service =
            ChatIntegrationService::new(metadata, events, broadcaster, "test_secret", dispatcher);

        assert!(service
            .extract_share_token("https://example.com/other/path")
            .is_err());
        assert!(service.extract_share_token("not_a_url").is_err());
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500.0 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GB");
    }

    #[test]
    fn test_webhook_registration() {
        let metadata = Arc::new(MockMetadataStore::new());
        let events = Arc::new(MockEventStore::new());
        let broadcaster = Arc::new(EventBroadcaster::new(100));
        let dispatcher = Arc::new(MockWebhookDispatcher::new());

        let mut service =
            ChatIntegrationService::new(metadata, events, broadcaster, "test_secret", dispatcher);

        service.register_webhook("https://chat.example.com/webhook".to_string());
        service.register_webhook("https://chat2.example.com/webhook".to_string());

        assert_eq!(service.get_webhook_urls().len(), 2);

        // Duplicate registration should not add
        service.register_webhook("https://chat.example.com/webhook".to_string());
        assert_eq!(service.get_webhook_urls().len(), 2);

        service.unregister_webhook("https://chat.example.com/webhook");
        assert_eq!(service.get_webhook_urls().len(), 1);
    }

    fn sample_incoming_event() -> IncomingChatEvent {
        IncomingChatEvent {
            event_type: "share.revoked".to_string(),
            timestamp: Utc::now(),
            user_id: Some(Uuid::new_v4().to_string()),
            data: serde_json::json!({"share_token": "abc123"}),
        }
    }

    #[tokio::test]
    async fn chat_webhook_signature_missing() {
        let metadata = Arc::new(MockMetadataStore::new());
        let events = Arc::new(MockEventStore::new());
        let broadcaster = Arc::new(EventBroadcaster::new(100));
        let dispatcher = Arc::new(MockWebhookDispatcher::new());
        let service =
            ChatIntegrationService::new(metadata, events, broadcaster, "test_secret", dispatcher);

        let event = sample_incoming_event();
        let body = serde_json::to_vec(&event).unwrap();

        let result = service.process_incoming_event(&body, "").await;
        assert!(
            matches!(
                result,
                Err(ChatIntegrationError::SignatureVerificationFailed)
            ),
            "Missing signature must be rejected"
        );
    }

    #[tokio::test]
    async fn chat_webhook_signature_invalid() {
        let metadata = Arc::new(MockMetadataStore::new());
        let events = Arc::new(MockEventStore::new());
        let broadcaster = Arc::new(EventBroadcaster::new(100));
        let dispatcher = Arc::new(MockWebhookDispatcher::new());
        let service =
            ChatIntegrationService::new(metadata, events, broadcaster, "test_secret", dispatcher);

        let event = sample_incoming_event();
        let body = serde_json::to_vec(&event).unwrap();

        let signer = WebhookSigner::new("wrong-secret");
        let signature = signer.sign(&body).unwrap();

        let result = service.process_incoming_event(&body, &signature).await;
        assert!(
            matches!(
                result,
                Err(ChatIntegrationError::SignatureVerificationFailed)
            ),
            "Invalid signature must be rejected"
        );
    }

    #[tokio::test]
    async fn chat_webhook_signature_valid() {
        let metadata = Arc::new(MockMetadataStore::new());
        let events = Arc::new(MockEventStore::new());
        let broadcaster = Arc::new(EventBroadcaster::new(100));
        let dispatcher = Arc::new(MockWebhookDispatcher::new());
        let service =
            ChatIntegrationService::new(metadata, events, broadcaster, "test_secret", dispatcher);

        let event = sample_incoming_event();
        let body = serde_json::to_vec(&event).unwrap();

        let signer = WebhookSigner::new("test_secret");
        let signature = signer.sign(&body).unwrap();

        let result = service.process_incoming_event(&body, &signature).await;
        assert!(
            result.is_ok(),
            "Valid signature must be accepted: {:?}",
            result
        );
    }
}
