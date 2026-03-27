//! HTTP handlers for notification operations.
//!
//! TODO: This module needs to be rewritten to use the new NotificationRepository
//! for notification storage instead of PostgreSQL.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{AuthenticatedUser, ErrorResponse};
use crate::AppState;

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
// Handlers
// ============================================================================

/// List notifications for the authenticated user.
///
/// GET /api/notifications?limit=50&offset=0&unread_only=false
///
/// TODO: Implement using new NotificationRepository
pub async fn list_notifications(
    State(_state): State<AppState>,
    _auth: AuthenticatedUser,
    Query(_query): Query<ListNotificationsQuery>,
) -> Result<Response, Response> {
    tracing::warn!("Notification list not yet implemented in zero-PostgreSQL mode");
    
    // Return empty list for now
    let response = ListNotificationsResponse {
        notifications: vec![],
        total: 0,
    };
    
    Ok(Json(response).into_response())
}

/// Count unread notifications for the authenticated user.
///
/// GET /api/notifications/unread-count
///
/// TODO: Implement using new NotificationRepository
pub async fn count_unread_notifications(
    State(_state): State<AppState>,
    _auth: AuthenticatedUser,
) -> Result<Response, Response> {
    tracing::warn!("Unread notification count not yet implemented in zero-PostgreSQL mode");
    
    Ok(Json(UnreadNotificationCountResponse { count: 0 }).into_response())
}

/// Mark a notification as read.
///
/// PUT /api/notifications/{id}/read
///
/// TODO: Implement using new NotificationRepository
pub async fn mark_notification_read(
    State(_state): State<AppState>,
    Path(_notification_id): Path<Uuid>,
    _auth: AuthenticatedUser,
) -> Result<Response, Response> {
    tracing::warn!("Mark notification read not yet implemented in zero-PostgreSQL mode");
    
    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(ErrorResponse::new("Not implemented")),
    )
        .into_response())
}

/// Delete a notification.
///
/// DELETE /api/notifications/{id}
///
/// TODO: Implement using new NotificationRepository
pub async fn delete_notification(
    State(_state): State<AppState>,
    Path(_notification_id): Path<Uuid>,
    _auth: AuthenticatedUser,
) -> Result<Response, Response> {
    tracing::warn!("Delete notification not yet implemented in zero-PostgreSQL mode");
    
    Ok((StatusCode::NO_CONTENT, ()).into_response())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

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
}
