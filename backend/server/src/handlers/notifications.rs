//! HTTP handlers for notification operations.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{handlers::AuthenticatedUser, AppState};

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
    pub id: String,
    pub notification_type: String,
    pub title: String,
    pub message: String,
    pub resource_id: String,
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
    pub count: u32,
}

// ============================================================================
// Handlers
// ============================================================================

/// List notifications for the authenticated user.
///
/// GET /api/notifications?limit=50&offset=0&unread_only=false
pub async fn list_notifications(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<ListNotificationsQuery>,
) -> Result<Response, Response> {
    let offset = query.offset.max(0) as usize;
    let limit = query.limit.clamp(1, 100) as usize;
    
    let notifications = state.notification_repo
        .list(auth.user_id.into(), query.unread_only, offset, limit)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list notifications: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to list notifications" })),
            )
                .into_response()
        })?;

    // Fetch full notification documents to get title, message, etc.
    let mut notification_responses = Vec::new();
    for n in notifications {
        // Try to fetch the full notification document
        let (title, message, action_url) = match state.notification_repo
            .get(auth.user_id.into(), n.notification_id)
            .await 
        {
            Ok(Some(doc)) => (doc.title, doc.message, None), // action_url not stored in NotificationDocument
            Ok(None) => {
                tracing::warn!("Notification {} not found in document store", n.notification_id);
                ("Notification".to_string(), "You have a new notification".to_string(), None)
            }
            Err(e) => {
                tracing::warn!("Failed to fetch notification {}: {}", n.notification_id, e);
                ("Notification".to_string(), "You have a new notification".to_string(), None)
            }
        };
        
        notification_responses.push(NotificationResponse {
            id: n.notification_id.to_string(),
            notification_type: format!("{:?}", n.notification_type).to_lowercase(),
            title,
            message,
            resource_id: n.resource_id.to_string(),
            resource_type: n.resource_type.clone(),
            action_url,
            read: n.read,
            created_at: n.created_at.to_rfc3339(),
        });
    }

    let response = ListNotificationsResponse {
        total: notification_responses.len(),
        notifications: notification_responses,
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
    let count = state.notification_repo
        .count_unread(auth.user_id.into())
        .await
        .map_err(|e| {
            tracing::error!("Failed to count unread notifications: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to count notifications" })),
            )
                .into_response()
        })?;

    Ok(Json(UnreadNotificationCountResponse { count }).into_response())
}

/// Mark a notification as read.
///
/// PUT /api/notifications/{id}/read
pub async fn mark_notification_read(
    State(state): State<AppState>,
    Path(notification_id): Path<Uuid>,
    auth: AuthenticatedUser,
) -> Result<Response, Response> {
    state.notification_repo
        .mark_read(auth.user_id.into(), notification_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to mark notification as read: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to mark notification as read" })),
            )
                .into_response()
        })?;

    Ok((StatusCode::OK, Json(serde_json::json!({ "success": true }))).into_response())
}

/// Delete a notification.
///
/// DELETE /api/notifications/{id}
pub async fn delete_notification(
    State(state): State<AppState>,
    Path(notification_id): Path<Uuid>,
    auth: AuthenticatedUser,
) -> Result<Response, Response> {
    state.notification_repo
        .delete(auth.user_id.into(), notification_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete notification: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to delete notification" })),
            )
                .into_response()
        })?;

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
