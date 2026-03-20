//! HTTP handlers for notification operations.
//!
//! This module implements endpoints for managing persistent notifications,
//! including listing, marking as read, and deleting notifications.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rustshare_core::domain::Notification;
use rustshare_core::services::NotificationError;

use crate::AppState;
use super::{AuthenticatedUser, ErrorResponse};

// ============================================================================
// Request/Response DTOs
// ============================================================================

/// Query parameters for listing notifications.
#[derive(Debug, Deserialize)]
pub struct ListNotificationsQuery {
    /// Maximum number of notifications to return.
    #[serde(default = "default_limit")]
    pub limit: i64,
    /// Number of notifications to skip for pagination.
    #[serde(default)]
    pub offset: i64,
    /// Filter to only unread notifications.
    #[serde(default)]
    pub unread_only: bool,
}

fn default_limit() -> i64 {
    50
}

/// Response for a notification.
#[derive(Debug, Serialize)]
pub struct NotificationResponse {
    pub id: Uuid,
    pub notification_type: String,
    pub title: String,
    pub message: String,
    pub resource_id: Uuid,
    pub resource_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_url: Option<String>,
    pub read: bool,
    pub created_at: String,
}

impl From<Notification> for NotificationResponse {
    fn from(n: Notification) -> Self {
        Self {
            id: n.id,
            notification_type: format!("{:?}", n.notification_type).to_lowercase(),
            title: n.title,
            message: n.message,
            resource_id: n.resource_id,
            resource_type: format!("{:?}", n.resource_type).to_lowercase(),
            action_url: n.action_url,
            read: n.read,
            created_at: n.created_at.to_rfc3339(),
        }
    }
}

/// Response for listing notifications with metadata.
#[derive(Debug, Serialize)]
pub struct ListNotificationsResponse {
    pub notifications: Vec<NotificationResponse>,
    pub total: usize,
}

/// Response for unread notification count.
#[derive(Debug, Serialize)]
pub struct UnreadNotificationCountResponse {
    pub count: i64,
}

// ============================================================================
// Error Handler
// ============================================================================

/// Map NotificationError to HTTP response.
pub fn notification_error_response(err: NotificationError) -> Response {
    let (status, message) = match err {
        NotificationError::NotFound => (StatusCode::NOT_FOUND, err.to_string()),
        NotificationError::NotFoundById(_) => (StatusCode::NOT_FOUND, err.to_string()),
        NotificationError::NotOwned { .. } => (StatusCode::FORBIDDEN, err.to_string()),
        NotificationError::Database(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
        }
    };

    (status, Json(ErrorResponse::new(message))).into_response()
}

// ============================================================================
// 1. GET /api/notifications - List notifications (paginated, optional unread filter)
// ============================================================================

/// List notifications for the authenticated user.
///
/// GET /api/notifications?limit=50&offset=0&unread_only=false
///
/// Returns paginated list of notifications sorted by created_at descending.
/// Supports filtering by unread status.
pub async fn list_notifications(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<ListNotificationsQuery>,
) -> Result<Response, Response> {
    let notifications = state
        .notification_service
        .list_notifications(auth.user_id, query.unread_only, query.limit, query.offset)
        .await
        .map_err(notification_error_response)?;

    let total = state
        .notification_service
        .count_notifications(auth.user_id, query.unread_only)
        .await
        .map_err(notification_error_response)? as usize;
    let response_list: Vec<NotificationResponse> = notifications
        .into_iter()
        .map(NotificationResponse::from)
        .collect();

    let response = ListNotificationsResponse {
        notifications: response_list,
        total,
    };

    Ok(Json(response).into_response())
}

/// Count unread notifications for the authenticated user.
///
/// GET /api/notifications/unread-count
pub async fn count_unread_notifications(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Response, Response> {
    let count = state
        .notification_service
        .count_unread(auth.user_id)
        .await
        .map_err(notification_error_response)?;

    Ok(Json(UnreadNotificationCountResponse { count }).into_response())
}

// ============================================================================
// 2. PUT /api/notifications/{id}/read - Mark as read
// ============================================================================

/// Mark a notification as read.
///
/// PUT /api/notifications/{id}/read
///
/// Requires ownership of the notification.
/// Returns the updated notification with read=true.
pub async fn mark_notification_read(
    State(state): State<AppState>,
    Path(notification_id): Path<Uuid>,
    auth: AuthenticatedUser,
) -> Result<Response, Response> {
    let notification = state
        .notification_service
        .mark_as_read(notification_id, auth.user_id)
        .await
        .map_err(notification_error_response)?;

    let response = NotificationResponse::from(notification);

    Ok(Json(response).into_response())
}

// ============================================================================
// 3. DELETE /api/notifications/{id} - Delete notification
// ============================================================================

/// Delete a notification.
///
/// DELETE /api/notifications/{id}
///
/// Requires ownership of the notification.
/// Returns 204 No Content on success.
pub async fn delete_notification(
    State(state): State<AppState>,
    Path(notification_id): Path<Uuid>,
    auth: AuthenticatedUser,
) -> Result<Response, Response> {
    state
        .notification_service
        .delete_notification(notification_id, auth.user_id)
        .await
        .map_err(notification_error_response)?;

    Ok((StatusCode::NO_CONTENT, ()).into_response())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Full integration tests require database setup and axum_test.
    // These tests verify that the handler functions are correctly typed and compile.
    // Integration tests will be added when test infrastructure is set up.

    #[test]
    fn test_list_notifications_query_defaults() {
        let json = serde_json::json!({});
        let query: Result<ListNotificationsQuery, _> = serde_json::from_value(json);
        assert!(query.is_ok());
        let query = query.unwrap();
        assert_eq!(query.limit, 50);
        assert_eq!(query.offset, 0);
        assert!(!query.unread_only);
    }

    #[test]
    fn test_list_notifications_query_custom() {
        let json = serde_json::json!({
            "limit": 10,
            "offset": 20,
            "unread_only": true
        });
        let query: Result<ListNotificationsQuery, _> = serde_json::from_value(json);
        assert!(query.is_ok());
        let query = query.unwrap();
        assert_eq!(query.limit, 10);
        assert_eq!(query.offset, 20);
        assert!(query.unread_only);
    }

    #[test]
    fn test_notification_response_serialization() {
        let response = NotificationResponse {
            id: Uuid::new_v4(),
            notification_type: "share_received".to_string(),
            title: "File shared".to_string(),
            message: "Alice shared file.pdf with you".to_string(),
            resource_id: Uuid::new_v4(),
            resource_type: "file".to_string(),
            action_url: Some("/files/123".to_string()),
            read: false,
            created_at: "2024-01-15T12:00:00Z".to_string(),
        };

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["notification_type"], "share_received");
        assert_eq!(json["title"], "File shared");
        assert_eq!(json["read"], false);
        assert!(json["action_url"].is_string());
    }
}
