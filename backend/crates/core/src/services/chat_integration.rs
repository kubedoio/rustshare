//! Chat integration service for RustShare.
//!
//! This service handles:
//! - Generating signed webhook events for file/share changes
//! - Verifying incoming chat integration requests
//! - Link unfurl requests with permission checking
//! - Dispatching events to registered chat webhook URLs

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
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
    #[error("Invalid webhook URL")]
    InvalidWebhookUrl,

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
    /// Get a share by token scoped to a tenant.
    async fn get_share_by_token(
        &self,
        token: &str,
        tenant_id: Uuid,
    ) -> anyhow::Result<Option<Share>>;

    /// Get a share by globally unique public token without tenant scoping.
    async fn get_share_by_token_unscoped(&self, token: &str) -> anyhow::Result<Option<Share>>;

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

/// Returns true if the IPv4 address is unspecified, loopback, private, link-local,
/// multicast, or part of the CGNAT range (100.64.0.0/10).
fn is_internal_ipv4(v4: &Ipv4Addr) -> bool {
    let octets = v4.octets();
    // CGNAT 100.64.0.0/10
    if octets[0] == 100 && (64..=127).contains(&octets[1]) {
        return true;
    }
    v4.is_unspecified()
        || v4.is_loopback()
        || v4.is_private()
        || v4.is_link_local()
        || v4.is_multicast()
}

/// Returns true if the IP address is unspecified, loopback, private, link-local,
/// multicast, unique-local, or an IPv4-mapped/compatible IPv6 address that
/// resolves to an internal IPv4 address.
fn is_internal_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_internal_ipv4(v4),
        IpAddr::V6(v6) => {
            if let Some(mapped_v4) = v6.to_ipv4() {
                return is_internal_ipv4(&mapped_v4);
            }
            v6.is_unspecified()
                || v6.is_loopback()
                || v6.is_unicast_link_local()
                || v6.is_multicast()
                || v6.is_unique_local()
        }
    }
}

/// Validate a chat webhook URL for SSRF safety.
///
/// Rejects non-HTTPS URLs (unless `allow_http` is true) and any URL whose host
/// resolves to an internal/private IP address. DNS resolution is bounded by a
/// 5-second timeout. DNS failures are logged server-side and surfaced to the
/// caller as a generic invalid URL error.
pub async fn validate_chat_webhook_url(
    url: &str,
    allow_http: bool,
) -> Result<(), ChatIntegrationError> {
    checked_webhook_socket_addrs(url, allow_http).await?;
    Ok(())
}

async fn checked_webhook_socket_addrs(
    url: &str,
    allow_http: bool,
) -> Result<(String, Vec<SocketAddr>), ChatIntegrationError> {
    let parsed = url::Url::parse(url).map_err(|_| ChatIntegrationError::InvalidWebhookUrl)?;

    // Scheme check.
    match parsed.scheme() {
        "https" => {}
        "http" if allow_http => {}
        _ => return Err(ChatIntegrationError::InvalidWebhookUrl),
    }

    let host = parsed
        .host_str()
        .ok_or(ChatIntegrationError::InvalidWebhookUrl)?;

    if host.eq_ignore_ascii_case("localhost") {
        return Err(ChatIntegrationError::InvalidWebhookUrl);
    }

    let port = parsed.port_or_known_default().unwrap_or(80);

    // Check IP literals first; these can bypass DNS-based defences.
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_internal_ip(&ip) {
            return Err(ChatIntegrationError::InvalidWebhookUrl);
        }
        return Ok((host.to_string(), vec![SocketAddr::new(ip, port)]));
    }

    // Resolve the hostname and verify none of the addresses are internal.
    // Cap DNS lookup at 5 seconds to avoid hanging on slow/unresponsive resolvers.
    let lookup = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::net::lookup_host((host, port)),
    )
    .await
    .map_err(|_| ChatIntegrationError::InvalidWebhookUrl)?;

    match lookup {
        Ok(addrs) => {
            let addrs: Vec<SocketAddr> = addrs.collect();
            if addrs.is_empty() {
                return Err(ChatIntegrationError::InvalidWebhookUrl);
            }
            for addr in &addrs {
                if is_internal_ip(&addr.ip()) {
                    return Err(ChatIntegrationError::InvalidWebhookUrl);
                }
            }
            Ok((host.to_string(), addrs))
        }
        Err(e) => {
            warn!(url = %url, error = %e, "DNS lookup failed for webhook URL");
            Err(ChatIntegrationError::InvalidWebhookUrl)
        }
    }
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
    timeout: Duration,
}

impl HttpWebhookDispatcher {
    /// Create a new HTTP webhook dispatcher.
    pub fn new() -> Self {
        Self {
            timeout: Duration::from_secs(30),
        }
    }

    /// Create with custom timeout.
    pub fn with_timeout(timeout: Duration) -> Self {
        Self { timeout }
    }
}

fn webhook_client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder().redirect(reqwest::redirect::Policy::none())
}

/// Build a reqwest client that does not follow redirects, preventing SSRF
/// payloads from redirecting to internal addresses after validation.
#[cfg(test)]
fn build_webhook_client() -> reqwest::Client {
    webhook_client_builder().build().unwrap_or_else(|e| {
        warn!(error = %e, "Failed to build no-redirect webhook client; falling back to default");
        reqwest::Client::new()
    })
}

/// Build a webhook client whose DNS resolution is pinned to the socket
/// addresses that passed SSRF validation. This prevents a separate client DNS
/// lookup from resolving the same hostname to an internal address.
fn build_pinned_webhook_client(
    host: &str,
    addrs: &[SocketAddr],
) -> Result<reqwest::Client, String> {
    webhook_client_builder()
        .resolve_to_addrs(host, addrs)
        .build()
        .map_err(|e| format!("Failed to build webhook client: {e}"))
}

impl Default for HttpWebhookDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebhookDispatcher for HttpWebhookDispatcher {
    async fn dispatch(&self, url: &str, event: &ChatEvent) -> std::result::Result<(), String> {
        // Re-validate and pin the vetted socket addresses at dispatch time.
        // HTTP is never allowed at dispatch time regardless of debug configuration.
        let (host, addrs) = checked_webhook_socket_addrs(url, false).await.map_err(|e| {
            warn!(url = %url, error = %e, "Webhook URL failed SSRF validation at dispatch time");
            "Invalid webhook URL".to_string()
        })?;
        let client = build_pinned_webhook_client(&host, &addrs)?;

        let response = client
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

/// Default maximum age for incoming webhook events (seconds).
const DEFAULT_WEBHOOK_MAX_AGE_SECONDS: i64 = 300;

fn parse_webhook_max_age_seconds(value: Option<&str>) -> i64 {
    value
        .map(|s| {
            s.parse::<i64>()
                .ok()
                .filter(|&v| v >= 1)
                .unwrap_or_else(|| {
                    warn!(
                        value = %s,
                        "RUSTSHARE_WEBHOOK_MAX_AGE_SECONDS must be a positive integer; using default"
                    );
                    DEFAULT_WEBHOOK_MAX_AGE_SECONDS
                })
        })
        .unwrap_or(DEFAULT_WEBHOOK_MAX_AGE_SECONDS)
}

/// Chat integration service for managing webhook events and link unfurls.
pub struct ChatIntegrationService<M: MetadataStoreOps, E: EventStoreOps, W: WebhookDispatcher> {
    metadata_store: Arc<M>,
    event_store: Arc<E>,
    broadcaster: Arc<EventBroadcaster>,
    signer: WebhookSigner,
    dispatcher: Arc<W>,
    webhook_urls: Vec<String>,
    webhook_max_age_seconds: i64,
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
        let webhook_max_age_seconds = parse_webhook_max_age_seconds(
            std::env::var("RUSTSHARE_WEBHOOK_MAX_AGE_SECONDS")
                .ok()
                .as_deref(),
        );

        Self {
            metadata_store,
            event_store,
            broadcaster,
            signer: WebhookSigner::new(webhook_secret),
            dispatcher,
            webhook_urls: Vec::new(),
            webhook_max_age_seconds,
        }
    }

    /// Create a new chat integration service with an explicit max age (tests only).
    #[cfg(test)]
    pub fn new_with_max_age(
        metadata_store: Arc<M>,
        event_store: Arc<E>,
        broadcaster: Arc<EventBroadcaster>,
        webhook_secret: impl AsRef<[u8]>,
        dispatcher: Arc<W>,
        webhook_max_age_seconds: i64,
    ) -> Self {
        Self {
            metadata_store,
            event_store,
            broadcaster,
            signer: WebhookSigner::new(webhook_secret),
            dispatcher,
            webhook_urls: Vec::new(),
            webhook_max_age_seconds,
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
        tenant_id: Option<Uuid>,
    ) -> Result<UnfurlResponse, ChatIntegrationError> {
        // Parse the URL to extract share token
        let share_token = self.extract_share_token(&request.url)?;

        let share = if let Some(tenant_id) = tenant_id {
            self.metadata_store
                .get_share_by_token(&share_token, tenant_id)
                .await
        } else {
            self.metadata_store
                .get_share_by_token_unscoped(&share_token)
                .await
        }
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

        if share.is_user_share() {
            let Some(user_id) = requesting_user_id else {
                return Err(ChatIntegrationError::PermissionDenied);
            };

            if tenant_id != Some(share.tenant_id) || share.recipient_user_id != Some(user_id) {
                return Err(ChatIntegrationError::PermissionDenied);
            }
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
    /// Verifies the `X-RustShare-Signature` over the raw request body, deserializes
    /// the event, and enforces a maximum age on the event timestamp to prevent
    /// replay attacks. Returns [`ChatIntegrationError::SignatureVerificationFailed`]
    /// when the signature is missing, invalid, or when the event timestamp is
    /// outside the allowed window.
    pub async fn process_incoming_event(
        &self,
        body: &[u8],
        signature: &str,
    ) -> Result<(), ChatIntegrationError> {
        if signature.is_empty() {
            return Err(ChatIntegrationError::SignatureVerificationFailed);
        }

        let verified = self
            .signer
            .verify(signature, body)
            .map_err(|_| ChatIntegrationError::SignatureVerificationFailed)?;
        if !verified {
            return Err(ChatIntegrationError::SignatureVerificationFailed);
        }

        let event: IncomingChatEvent = serde_json::from_slice(body)
            .map_err(|e| ChatIntegrationError::Serialization(e.to_string()))?;

        // Enforce maximum age to prevent replay attacks.
        // Return SignatureVerificationFailed to avoid leaking that the failure
        // was due to replay.
        let now = Utc::now();
        let age_seconds = (now - event.timestamp).num_seconds().abs();
        if event.timestamp > now || age_seconds > self.webhook_max_age_seconds {
            return Err(ChatIntegrationError::SignatureVerificationFailed);
        }

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

        let parsed = url::Url::parse(url).map_err(|_| ChatIntegrationError::InvalidWebhookUrl)?;

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

        Err(ChatIntegrationError::InvalidWebhookUrl)
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
        async fn get_share_by_token(
            &self,
            token: &str,
            tenant_id: Uuid,
        ) -> anyhow::Result<Option<Share>> {
            Ok(self
                .shares
                .lock()
                .unwrap()
                .iter()
                .find(|s| s.share_token.as_deref() == Some(token) && s.tenant_id == tenant_id)
                .cloned())
        }

        async fn get_share_by_token_unscoped(&self, token: &str) -> anyhow::Result<Option<Share>> {
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

    fn test_chat_service(
        metadata: Arc<MockMetadataStore>,
    ) -> ChatIntegrationService<MockMetadataStore, MockEventStore, MockWebhookDispatcher> {
        ChatIntegrationService::new(
            metadata,
            Arc::new(MockEventStore::new()),
            Arc::new(EventBroadcaster::new(100)),
            "test_secret",
            Arc::new(MockWebhookDispatcher::new()),
        )
    }

    #[tokio::test]
    async fn test_unfurl_user_share_requires_recipient() {
        let metadata = Arc::new(MockMetadataStore::new());
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let recipient_id = Uuid::new_v4();
        let other_user_id = Uuid::new_v4();
        let file = File::new(
            "private.pdf".to_string(),
            "/private.pdf".to_string(),
            "hash".to_string(),
            42,
            "application/pdf".to_string(),
            None,
            owner_id,
            tenant_id,
        );
        let share = Share {
            id: Uuid::new_v4(),
            file_id: Some(file.id),
            folder_id: None,
            share_token: Some("private-token".to_string()),
            permissions: SharePermissions::View,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: Some(recipient_id),
            recipient_group_id: None,
            created_by: owner_id,
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id,
        };
        metadata.files.lock().unwrap().push(file);
        metadata.shares.lock().unwrap().push(share);
        let service = test_chat_service(metadata);
        let request = UnfurlRequest {
            url: "https://rustshare.example.com/share/private-token".to_string(),
        };

        let result = service
            .unfurl_link(&request, Some(other_user_id), Some(tenant_id))
            .await;

        assert!(matches!(
            result,
            Err(ChatIntegrationError::PermissionDenied)
        ));
    }

    #[tokio::test]
    async fn test_unfurl_public_share_resolves_without_tenant() {
        let metadata = Arc::new(MockMetadataStore::new());
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let file = File::new(
            "public.pdf".to_string(),
            "/public.pdf".to_string(),
            "hash".to_string(),
            42,
            "application/pdf".to_string(),
            None,
            owner_id,
            tenant_id,
        );
        let share = Share {
            id: Uuid::new_v4(),
            file_id: Some(file.id),
            folder_id: None,
            share_token: Some("public-token".to_string()),
            permissions: SharePermissions::View,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: None,
            recipient_group_id: None,
            created_by: owner_id,
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id,
        };
        metadata.files.lock().unwrap().push(file);
        metadata.shares.lock().unwrap().push(share);
        let service = test_chat_service(metadata);
        let request = UnfurlRequest {
            url: "https://rustshare.example.com/share/public-token".to_string(),
        };

        let response = service.unfurl_link(&request, None, None).await.unwrap();

        assert_eq!(response.metadata.title, "public.pdf");
    }

    #[tokio::test]
    async fn test_unfurl_user_share_allows_recipient() {
        let metadata = Arc::new(MockMetadataStore::new());
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let recipient_id = Uuid::new_v4();
        let file = File::new(
            "private.pdf".to_string(),
            "/private.pdf".to_string(),
            "hash".to_string(),
            42,
            "application/pdf".to_string(),
            None,
            owner_id,
            tenant_id,
        );
        let share = Share {
            id: Uuid::new_v4(),
            file_id: Some(file.id),
            folder_id: None,
            share_token: Some("recipient-token".to_string()),
            permissions: SharePermissions::View,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: Some(recipient_id),
            recipient_group_id: None,
            created_by: owner_id,
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id,
        };
        metadata.files.lock().unwrap().push(file);
        metadata.shares.lock().unwrap().push(share);
        let service = test_chat_service(metadata);
        let request = UnfurlRequest {
            url: "https://rustshare.example.com/share/recipient-token".to_string(),
        };

        let response = service
            .unfurl_link(&request, Some(recipient_id), Some(tenant_id))
            .await
            .unwrap();

        assert_eq!(response.metadata.title, "private.pdf");
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

    #[test]
    fn test_parse_webhook_max_age_invalid_values_use_default() {
        assert_eq!(parse_webhook_max_age_seconds(Some("0")), 300);
        assert_eq!(parse_webhook_max_age_seconds(Some("-1")), 300);
        assert_eq!(parse_webhook_max_age_seconds(Some("not-a-number")), 300);
    }

    #[test]
    fn test_parse_webhook_max_age_positive_value_is_used() {
        assert_eq!(parse_webhook_max_age_seconds(Some("60")), 60);
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

    fn test_service_with_max_age(
        max_age_seconds: i64,
    ) -> ChatIntegrationService<MockMetadataStore, MockEventStore, MockWebhookDispatcher> {
        ChatIntegrationService::new_with_max_age(
            Arc::new(MockMetadataStore::new()),
            Arc::new(MockEventStore::new()),
            Arc::new(EventBroadcaster::new(100)),
            "test_secret",
            Arc::new(MockWebhookDispatcher::new()),
            max_age_seconds,
        )
    }

    fn sign_incoming_event(event: &IncomingChatEvent) -> (Vec<u8>, String) {
        let body = serde_json::to_vec(event).unwrap();
        let signer = WebhookSigner::new("test_secret");
        let signature = signer
            .sign_with_timestamp(event.timestamp.timestamp(), &body)
            .unwrap();
        (body, signature)
    }

    #[tokio::test]
    async fn test_process_incoming_event_within_age_accepted() {
        let service = test_service_with_max_age(300);
        let mut event = sample_incoming_event();
        event.timestamp = Utc::now() - chrono::Duration::seconds(10);
        let (body, signature) = sign_incoming_event(&event);

        assert!(service
            .process_incoming_event(&body, &signature)
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn test_process_incoming_event_exact_boundary_accepted() {
        let max_age = 300;
        let service = test_service_with_max_age(max_age);
        let mut event = sample_incoming_event();
        event.timestamp = Utc::now() - chrono::Duration::seconds(max_age);
        let (body, signature) = sign_incoming_event(&event);

        assert!(service
            .process_incoming_event(&body, &signature)
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn test_process_incoming_event_too_old_rejected() {
        let service = test_service_with_max_age(300);
        let mut event = sample_incoming_event();
        event.timestamp = Utc::now() - chrono::Duration::seconds(301);
        let (body, signature) = sign_incoming_event(&event);

        let result = service.process_incoming_event(&body, &signature).await;
        assert!(matches!(
            result,
            Err(ChatIntegrationError::SignatureVerificationFailed)
        ));
    }

    #[tokio::test]
    async fn test_process_incoming_event_future_timestamp_rejected() {
        let service = test_service_with_max_age(300);
        let mut event = sample_incoming_event();
        event.timestamp = Utc::now() + chrono::Duration::seconds(10);
        let (body, signature) = sign_incoming_event(&event);

        let result = service.process_incoming_event(&body, &signature).await;
        assert!(matches!(
            result,
            Err(ChatIntegrationError::SignatureVerificationFailed)
        ));
    }

    #[tokio::test]
    async fn test_validate_chat_webhook_url_public_ip_accepted() {
        assert!(validate_chat_webhook_url("https://1.1.1.1/webhook", false)
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn test_checked_webhook_socket_addrs_pins_public_ip_literal() {
        let (host, addrs) = checked_webhook_socket_addrs("https://1.1.1.1/webhook", false)
            .await
            .expect("public IP literal should pass validation");

        assert_eq!(host, "1.1.1.1");
        assert_eq!(addrs, vec![SocketAddr::from(([1, 1, 1, 1], 443))]);
    }

    #[tokio::test]
    async fn test_pinned_webhook_client_uses_validated_addresses() {
        let addrs = [SocketAddr::from(([1, 1, 1, 1], 443))];
        build_pinned_webhook_client("example.com", &addrs)
            .expect("pinned webhook client should build");
    }

    #[tokio::test]
    async fn test_validate_chat_webhook_url_http_rejected_without_allow_http() {
        assert!(validate_chat_webhook_url("http://1.1.1.1/webhook", false)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_validate_chat_webhook_url_http_accepted_with_allow_http() {
        assert!(validate_chat_webhook_url("http://1.1.1.1/webhook", true)
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn test_validate_chat_webhook_url_localhost_rejected() {
        assert!(
            validate_chat_webhook_url("https://localhost/webhook", false)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_validate_chat_webhook_url_private_ipv4_rejected() {
        assert!(validate_chat_webhook_url("https://10.0.0.1/webhook", false)
            .await
            .is_err());
        assert!(
            validate_chat_webhook_url("https://172.16.0.1/webhook", false)
                .await
                .is_err()
        );
        assert!(
            validate_chat_webhook_url("https://192.168.1.1/webhook", false)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_validate_chat_webhook_url_link_local_ipv4_rejected() {
        assert!(
            validate_chat_webhook_url("https://169.254.1.1/webhook", false)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_validate_chat_webhook_url_loopback_ipv4_rejected() {
        assert!(
            validate_chat_webhook_url("https://127.0.0.1/webhook", false)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_validate_chat_webhook_url_unspecified_addresses_rejected() {
        assert!(validate_chat_webhook_url("https://0.0.0.0/webhook", false)
            .await
            .is_err());
        assert!(validate_chat_webhook_url("https://[::]/webhook", false)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_validate_chat_webhook_url_multicast_ipv4_rejected() {
        assert!(
            validate_chat_webhook_url("https://224.0.0.1/webhook", false)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_validate_chat_webhook_url_cgnat_rejected() {
        assert!(
            validate_chat_webhook_url("https://100.64.0.1/webhook", false)
                .await
                .is_err()
        );
        assert!(
            validate_chat_webhook_url("https://100.127.255.255/webhook", false)
                .await
                .is_err()
        );
        assert!(
            validate_chat_webhook_url("https://100.63.0.1/webhook", false)
                .await
                .is_ok()
        );
        assert!(
            validate_chat_webhook_url("https://100.128.0.1/webhook", false)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn test_validate_chat_webhook_url_ipv6_loopback_rejected() {
        assert!(validate_chat_webhook_url("https://[::1]/webhook", false)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_validate_chat_webhook_url_ipv6_link_local_rejected() {
        assert!(
            validate_chat_webhook_url("https://[fe80::1]/webhook", false)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_validate_chat_webhook_url_ipv6_multicast_rejected() {
        assert!(
            validate_chat_webhook_url("https://[ff02::1]/webhook", false)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_validate_chat_webhook_url_ipv6_unique_local_rejected() {
        assert!(
            validate_chat_webhook_url("https://[fc00::1]/webhook", false)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_validate_chat_webhook_url_ipv4_mapped_ipv6_rejected() {
        assert!(
            validate_chat_webhook_url("https://[::ffff:127.0.0.1]/webhook", false)
                .await
                .is_err()
        );
        assert!(
            validate_chat_webhook_url("https://[::ffff:10.0.0.1]/webhook", false)
                .await
                .is_err()
        );
        assert!(
            validate_chat_webhook_url("https://[::ffff:192.168.1.1]/webhook", false)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_validate_chat_webhook_url_ipv4_compatible_ipv6_rejected() {
        assert!(
            validate_chat_webhook_url("https://[::127.0.0.1]/webhook", false)
                .await
                .is_err()
        );
        assert!(
            validate_chat_webhook_url("https://[::10.0.0.1]/webhook", false)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_webhook_client_does_not_follow_redirects() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
            Ok(listener) => listener,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
            Err(error) => panic!("failed to bind redirect test listener: {error}"),
        };
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 1024];
            let _ = socket.read(&mut buffer).await.unwrap();
            socket
                .write_all(
                    b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1/internal\r\nContent-Length: 0\r\n\r\n",
                )
                .await
                .unwrap();
        });

        let response = build_webhook_client()
            .post(format!("http://{addr}/webhook"))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), reqwest::StatusCode::FOUND);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn test_validate_chat_webhook_url_dns_failure_is_generic_error() {
        // This invalid TLD should not resolve, producing a generic invalid URL error.
        let result =
            validate_chat_webhook_url("https://invalid-tld-for-test.invalid/webhook", false).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            !err.contains("DNS"),
            "DNS failure should not be exposed: {}",
            err
        );
        assert!(
            !err.contains("internal IP"),
            "Internal reason should not be exposed: {}",
            err
        );
    }
}
