//! Chat integration HTTP handlers for RustShare.
//!
//! This module provides endpoints for:
//! - Link unfurling with permission checking
//! - Receiving webhook events from chat systems
//! - Dispatching internal events to registered chat webhooks

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use rustshare_core::services::{
    validate_chat_webhook_url, ChatIntegrationError, IncomingChatEvent, UnfurlRequest,
};

use crate::handlers::{AdminUser, AuthenticatedUser, ErrorResponse};
use crate::AppState;

const PUBLIC_UNFURL_TENANT_HEADER: &str = "X-Tenant-ID";

fn parse_unfurl_tenant_header(
    headers: &HeaderMap,
) -> Result<Uuid, (StatusCode, Json<ErrorResponse>)> {
    let header = headers.get(PUBLIC_UNFURL_TENANT_HEADER).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Missing X-Tenant-ID header")),
        )
    })?;
    let value = header.to_str().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Invalid X-Tenant-ID header")),
        )
    })?;
    Uuid::parse_str(value).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Invalid X-Tenant-ID header")),
        )
    })
}

fn parse_optional_unfurl_tenant_header(
    headers: &HeaderMap,
) -> Result<Option<Uuid>, (StatusCode, Json<ErrorResponse>)> {
    if headers.contains_key(PUBLIC_UNFURL_TENANT_HEADER) {
        parse_unfurl_tenant_header(headers).map(Some)
    } else {
        Ok(None)
    }
}

/// Request to unfurl a RustShare link.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UnfurlLinkRequest {
    pub url: String,
}

/// Response from unfurling a link.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct UnfurlLinkResponse {
    pub title: String,
    pub description: Option<String>,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub share_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub password_protected: bool,
    pub permissions: String,
}

/// POST /api/v1/integrations/chat/unfurl
///
/// Link unfurl endpoint that accepts a RustShare URL and returns preview metadata.
/// Requires authentication to verify the user has permission to view the shared resource.
#[utoipa::path(
    post,
    path = "/api/v1/integrations/chat/unfurl",
    tag = "Chat Integration",
    request_body = UnfurlLinkRequest,
    responses(
        (status = 200, description = "Unfurled link metadata", body = UnfurlLinkResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 403, description = "Permission denied", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Share or file not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn unfurl_link(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(req): Json<UnfurlLinkRequest>,
) -> Result<Json<UnfurlLinkResponse>, (StatusCode, Json<ErrorResponse>)> {
    debug!(
        "Unfurl request for URL: {} by user: {}",
        req.url, auth.user_id
    );

    let unfurl_req = UnfurlRequest { url: req.url };

    let response = state
        .chat_integration_service
        .unfurl_link(&unfurl_req, Some(auth.user_id), Some(auth.tenant_id))
        .await
        .map_err(map_chat_integration_error)?;

    let metadata = response.metadata;

    Ok(Json(UnfurlLinkResponse {
        title: metadata.title,
        description: metadata.description,
        resource_type: metadata.resource_type,
        resource_id: metadata.resource_id,
        share_token: metadata.share_token,
        mime_type: metadata.mime_type,
        size: metadata.size,
        thumbnail_url: metadata.thumbnail_url,
        created_at: metadata.created_at,
        expires_at: metadata.expires_at,
        password_protected: metadata.password_protected,
        permissions: format!("{:?}", metadata.permissions),
    }))
}

/// POST /api/v1/integrations/chat/unfurl/public
///
/// Public link unfurl endpoint that doesn't require authentication.
/// This is for chat systems that want to preview public shares.
#[utoipa::path(
    post,
    path = "/api/v1/integrations/chat/unfurl/public",
    tag = "Chat Integration",
    params(
        ("X-Tenant-ID" = Uuid, Header, description = "Tenant identifier for the public chat unfurl request"),
    ),
    request_body = UnfurlLinkRequest,
    responses(
        (status = 200, description = "Unfurled link metadata", body = UnfurlLinkResponse),
        (status = 404, description = "Share or file not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn unfurl_link_public(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<UnfurlLinkRequest>,
) -> Result<Json<UnfurlLinkResponse>, (StatusCode, Json<ErrorResponse>)> {
    debug!("Public unfurl request for URL: {}", req.url);

    let tenant_id = parse_optional_unfurl_tenant_header(&headers)?;
    let unfurl_req = UnfurlRequest { url: req.url };

    let response = state
        .chat_integration_service
        .unfurl_link(&unfurl_req, None, tenant_id)
        .await
        .map_err(map_chat_integration_error)?;

    let metadata = response.metadata;

    Ok(Json(UnfurlLinkResponse {
        title: metadata.title,
        description: metadata.description,
        resource_type: metadata.resource_type,
        resource_id: metadata.resource_id,
        share_token: metadata.share_token,
        mime_type: metadata.mime_type,
        size: metadata.size,
        thumbnail_url: metadata.thumbnail_url,
        created_at: metadata.created_at,
        expires_at: metadata.expires_at,
        password_protected: metadata.password_protected,
        permissions: format!("{:?}", metadata.permissions),
    }))
}

/// POST /api/v1/integrations/chat/events
///
/// Webhook receiver for chat events from external chat systems.
/// Verifies the event signature and processes the event.
#[utoipa::path(
    post,
    path = "/api/v1/integrations/chat/events",
    tag = "Chat Integration",
    request_body = IncomingChatEvent,
    responses(
        (status = 200, description = "Event processed"),
        (status = 401, description = "Missing or invalid signature", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn receive_chat_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // Extract signature from headers
    let signature = headers
        .get("X-RustShare-Signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if signature.is_empty() {
        warn!("Missing X-RustShare-Signature header");
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Missing signature".to_string(),
                details: Some("X-RustShare-Signature header is required".to_string()),
            }),
        );
    }

    // Process the event
    match state
        .chat_integration_service
        .process_incoming_event(&body, signature)
        .await
    {
        Ok(()) => {
            info!("Successfully processed chat event");
            (
                StatusCode::OK,
                Json(ErrorResponse {
                    error: "OK".to_string(),
                    details: None,
                }),
            )
        }
        Err(e) => {
            error!("Failed to process chat event: {}", e);
            let (status, response) = map_chat_integration_error_tuple(e);
            (status, Json(response))
        }
    }
}

/// Request to dispatch an event to registered chat webhooks.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct DispatchWebhookRequest {
    pub event_type: String,
    pub payload: serde_json::Value,
}

/// Response from dispatching webhooks.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DispatchWebhookResponse {
    pub dispatched: Vec<WebhookDispatchResult>,
}

/// Result of a single webhook dispatch.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct WebhookDispatchResult {
    pub url: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// POST /api/v1/integrations/webhooks/dispatch
///
/// Internal endpoint to dispatch events to registered chat webhooks.
/// This is called internally when shares are revoked or files change.
#[utoipa::path(
    post,
    path = "/api/v1/integrations/webhooks/dispatch",
    tag = "Chat Integration",
    request_body = DispatchWebhookRequest,
    responses(
        (status = 200, description = "Dispatch results", body = DispatchWebhookResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn dispatch_webhooks(
    State(_state): State<AppState>,
    _admin: AuthenticatedUser, // Only authenticated users can trigger dispatches
    Json(req): Json<DispatchWebhookRequest>,
) -> Result<Json<DispatchWebhookResponse>, (StatusCode, Json<ErrorResponse>)> {
    debug!("Dispatching webhooks for event: {}", req.event_type);

    // For now, return empty dispatch results
    // In a full implementation, we would:
    // 1. Parse the event type and payload
    // 2. Create the appropriate ChatEvent
    // 3. Dispatch to all registered webhooks
    // 4. Return results

    let dispatched = Vec::new();

    Ok(Json(DispatchWebhookResponse { dispatched }))
}

/// Request to register a new webhook URL.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RegisterWebhookRequest {
    pub url: String,
}

/// POST /api/v1/admin/integrations/chat/webhooks
///
/// Register a new chat webhook URL for event dispatch.
/// Requires admin privileges.
#[utoipa::path(
    post,
    path = "/api/v1/admin/integrations/chat/webhooks",
    tag = "Chat Integration",
    request_body = RegisterWebhookRequest,
    responses(
        (status = 201, description = "Webhook registered"),
        (status = 400, description = "Invalid URL", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn register_chat_webhook(
    State(_state): State<AppState>,
    _admin: AdminUser,
    Json(req): Json<RegisterWebhookRequest>,
) -> impl IntoResponse {
    info!("Registering chat webhook: {}", req.url);

    // Reject non-HTTPS webhook URLs in production. HTTP is permitted in debug
    // builds or when the RUSTSHARE_ALLOW_HTTP_WEBHOOKS environment variable is
    // set to "true" or "1" (case-insensitive). Internal/private addresses are
    // always rejected to mitigate SSRF.
    let allow_http = http_webhooks_allowed();

    if let Err(e) = validate_chat_webhook_url(&req.url, allow_http).await {
        warn!(url = %req.url, error = %e, "Rejected invalid webhook URL registration");
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid webhook URL".to_string(),
                details: Some("Invalid webhook URL".to_string()),
            }),
        );
    }

    // Register the webhook
    // Note: This is a simplified implementation. In production, webhooks
    // would be stored in the database with associated metadata.

    (
        StatusCode::CREATED,
        Json(ErrorResponse {
            error: "Webhook registered".to_string(),
            details: Some(req.url),
        }),
    )
}

/// GET /api/v1/admin/integrations/chat/webhooks
///
/// List registered chat webhook URLs.
/// Requires admin privileges.
#[utoipa::path(
    get,
    path = "/api/v1/admin/integrations/chat/webhooks",
    tag = "Chat Integration",
    responses(
        (status = 200, description = "Registered chat webhooks", body = WebhookListResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_chat_webhooks(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Json<WebhookListResponse> {
    let urls: Vec<String> = state
        .chat_integration_service
        .get_webhook_urls()
        .iter()
        .map(|s| s.to_string())
        .collect();

    Json(WebhookListResponse { webhooks: urls })
}

/// Response containing registered webhooks.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct WebhookListResponse {
    pub webhooks: Vec<String>,
}

/// Determine whether HTTP webhook URLs are allowed.
///
/// HTTP is always permitted in debug builds. In release builds it is only
/// permitted when `RUSTSHARE_ALLOW_HTTP_WEBHOOKS` is set to `"true"` or `"1"`
/// (case-insensitive). Any other value, including `"false"` or an empty string,
/// is treated as disabled.
fn http_webhooks_allowed() -> bool {
    cfg!(debug_assertions)
        || parse_allow_http_webhooks(
            std::env::var("RUSTSHARE_ALLOW_HTTP_WEBHOOKS")
                .ok()
                .as_deref(),
        )
}

/// Parse the `RUSTSHARE_ALLOW_HTTP_WEBHOOKS` value.
///
/// Returns `true` only for `"true"` or `"1"` (case-insensitive).
fn parse_allow_http_webhooks(value: Option<&str>) -> bool {
    value
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false)
}

/// Map ChatIntegrationError to HTTP response.
fn map_chat_integration_error(err: ChatIntegrationError) -> (StatusCode, Json<ErrorResponse>) {
    let (status, message) = match err {
        ChatIntegrationError::ShareNotFound => {
            (StatusCode::NOT_FOUND, "Share not found".to_string())
        }
        ChatIntegrationError::FileNotFound => (StatusCode::NOT_FOUND, "File not found".to_string()),
        ChatIntegrationError::FolderNotFound => {
            (StatusCode::NOT_FOUND, "Folder not found".to_string())
        }
        ChatIntegrationError::PermissionDenied => {
            (StatusCode::FORBIDDEN, "Permission denied".to_string())
        }
        ChatIntegrationError::InvalidWebhookUrl => {
            (StatusCode::BAD_REQUEST, "Invalid webhook URL".to_string())
        }
        ChatIntegrationError::SignatureVerificationFailed => (
            StatusCode::UNAUTHORIZED,
            "Signature verification failed".to_string(),
        ),
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        ),
    };

    (
        status,
        Json(ErrorResponse {
            error: message.clone(),
            details: Some(message),
        }),
    )
}

/// Map ChatIntegrationError to tuple for use in non-Json responses.
fn map_chat_integration_error_tuple(err: ChatIntegrationError) -> (StatusCode, ErrorResponse) {
    let (status, message) = match err {
        ChatIntegrationError::ShareNotFound => {
            (StatusCode::NOT_FOUND, "Share not found".to_string())
        }
        ChatIntegrationError::FileNotFound => (StatusCode::NOT_FOUND, "File not found".to_string()),
        ChatIntegrationError::FolderNotFound => {
            (StatusCode::NOT_FOUND, "Folder not found".to_string())
        }
        ChatIntegrationError::PermissionDenied => {
            (StatusCode::FORBIDDEN, "Permission denied".to_string())
        }
        ChatIntegrationError::InvalidWebhookUrl => {
            (StatusCode::BAD_REQUEST, "Invalid webhook URL".to_string())
        }
        ChatIntegrationError::SignatureVerificationFailed => (
            StatusCode::UNAUTHORIZED,
            "Signature verification failed".to_string(),
        ),
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        ),
    };

    (
        status,
        ErrorResponse {
            error: message.clone(),
            details: Some(message),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;

    #[tokio::test]
    async fn chat_webhook_http_rejection() {
        assert!(
            validate_chat_webhook_url("http://1.1.1.1/webhook", false)
                .await
                .is_err(),
            "HTTP webhook URLs must be rejected in production"
        );
        assert!(
            validate_chat_webhook_url("https://1.1.1.1/webhook", false)
                .await
                .is_ok(),
            "HTTPS webhook URLs must be accepted"
        );
        assert!(
            validate_chat_webhook_url("http://1.1.1.1/webhook", true)
                .await
                .is_ok(),
            "HTTP webhook URLs may be allowed when explicitly enabled"
        );
        assert!(
            validate_chat_webhook_url("ftp://1.1.1.1/webhook", false)
                .await
                .is_err(),
            "Non-HTTP(S) schemes must be rejected"
        );
        assert!(
            validate_chat_webhook_url("not-a-url", false).await.is_err(),
            "Malformed URLs must be rejected"
        );
    }

    #[tokio::test]
    async fn test_valid_chat_webhook_url_https_public_ip_accepted() {
        assert!(validate_chat_webhook_url("https://1.1.1.1/webhook", false)
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn test_valid_chat_webhook_url_http_public_ip_rejected_without_allow_http() {
        assert!(validate_chat_webhook_url("http://1.1.1.1/webhook", false)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_valid_chat_webhook_url_http_public_ip_accepted_with_allow_http() {
        assert!(validate_chat_webhook_url("http://1.1.1.1/webhook", true)
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn test_valid_chat_webhook_url_localhost_rejected() {
        assert!(
            validate_chat_webhook_url("https://localhost/webhook", false)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_valid_chat_webhook_url_192_168_rejected() {
        assert!(
            validate_chat_webhook_url("https://192.168.1.1/webhook", false)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_valid_chat_webhook_url_10_rejected() {
        assert!(validate_chat_webhook_url("https://10.0.0.1/webhook", false)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_valid_chat_webhook_url_172_16_rejected() {
        assert!(
            validate_chat_webhook_url("https://172.16.0.1/webhook", false)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_valid_chat_webhook_url_172_31_rejected() {
        assert!(
            validate_chat_webhook_url("https://172.31.0.1/webhook", false)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_valid_chat_webhook_url_link_local_rejected() {
        assert!(
            validate_chat_webhook_url("https://169.254.1.1/webhook", false)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_valid_chat_webhook_url_ipv6_loopback_rejected() {
        assert!(validate_chat_webhook_url("https://[::1]/webhook", false)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_valid_chat_webhook_url_ipv6_link_local_rejected() {
        assert!(
            validate_chat_webhook_url("https://[fe80::1]/webhook", false)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_valid_chat_webhook_url_ipv6_multicast_rejected() {
        assert!(
            validate_chat_webhook_url("https://[ff02::1]/webhook", false)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_valid_chat_webhook_url_ipv6_unique_local_rejected() {
        assert!(
            validate_chat_webhook_url("https://[fc00::1]/webhook", false)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_valid_chat_webhook_url_ipv4_mapped_ipv6_loopback_rejected() {
        assert!(
            validate_chat_webhook_url("https://[::ffff:127.0.0.1]/webhook", false)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_valid_chat_webhook_url_ipv4_mapped_ipv6_private_rejected() {
        assert!(
            validate_chat_webhook_url("https://[::ffff:10.0.0.1]/webhook", false)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_valid_chat_webhook_url_cgnat_rejected() {
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
    }

    #[test]
    fn parse_allow_http_webhooks_is_value_sensitive() {
        assert!(
            parse_allow_http_webhooks(Some("true")),
            "lowercase 'true' must enable HTTP webhooks"
        );
        assert!(
            parse_allow_http_webhooks(Some("TRUE")),
            "uppercase 'TRUE' must enable HTTP webhooks"
        );
        assert!(
            parse_allow_http_webhooks(Some("True")),
            "mixed-case 'True' must enable HTTP webhooks"
        );
        assert!(
            parse_allow_http_webhooks(Some("1")),
            "'1' must enable HTTP webhooks"
        );
        assert!(
            !parse_allow_http_webhooks(Some("false")),
            "'false' must reject HTTP webhooks"
        );
        assert!(
            !parse_allow_http_webhooks(Some("FALSE")),
            "uppercase 'FALSE' must reject HTTP webhooks"
        );
        assert!(
            !parse_allow_http_webhooks(Some("0")),
            "'0' must reject HTTP webhooks"
        );
        assert!(
            !parse_allow_http_webhooks(Some("")),
            "empty value must reject HTTP webhooks"
        );
        assert!(
            !parse_allow_http_webhooks(Some("yes")),
            "arbitrary truthy words must reject HTTP webhooks"
        );
        assert!(
            !parse_allow_http_webhooks(None),
            "missing value must reject HTTP webhooks"
        );
    }

    #[test]
    fn chat_integration_admin_authorization() {
        fn assert_list_requires_admin<H, Fut>(_handler: H)
        where
            H: Fn(State<AppState>, AdminUser) -> Fut,
            Fut: Future,
        {
        }
        assert_list_requires_admin(list_chat_webhooks);

        fn assert_register_requires_admin<H, Fut>(_handler: H)
        where
            H: Fn(State<AppState>, AdminUser, Json<RegisterWebhookRequest>) -> Fut,
            Fut: Future,
        {
        }
        assert_register_requires_admin(register_chat_webhook);
    }
}
