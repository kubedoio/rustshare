//! HTTP handlers for notification operations.
//!
//! This module implements endpoints for managing persistent notifications,
//! including listing, marking as read, and deleting notifications.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use rustshare_core::domain::{Notification, SharePermissions};
use rustshare_core::events::{AggregateType, EventType};

use super::AuthenticatedUser;
use crate::handlers::AppError;
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

impl From<Notification> for NotificationResponse {
    fn from(n: Notification) -> Self {
        Self {
            id: n.id,
            notification_type: n.notification_type.to_string(),
            title: n.title,
            message: n.message,
            resource_id: n.resource_id,
            resource_type: n.resource_type.to_string(),
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
// Activity Feed DTOs
// ============================================================================

/// Query parameters for the activity feed.
#[derive(Debug, Deserialize)]
pub struct ListActivityQuery {
    /// Maximum number of activity items to return.
    #[serde(default = "default_limit")]
    pub limit: i64,
    /// Cursor: return events before this timestamp.
    #[serde(default)]
    pub before_timestamp: Option<DateTime<Utc>>,
    /// Cursor: return events before this event ID (for tie-breaking).
    #[serde(default)]
    pub before_id: Option<Uuid>,
}

/// A single activity feed item.
#[derive(Debug, Serialize)]
pub struct ActivityItemResponse {
    pub id: Uuid,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_name: Option<String>,
    pub actor_id: Uuid,
    pub timestamp: String,
}

/// Response for the activity feed endpoint.
#[derive(Debug, Serialize)]
pub struct ListActivityResponse {
    pub items: Vec<ActivityItemResponse>,
    pub next_cursor: Option<ActivityCursor>,
}

/// Cursor for paginating the activity feed.
#[derive(Debug, Serialize)]
pub struct ActivityCursor {
    pub before_timestamp: String,
    pub before_id: Uuid,
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
) -> Result<axum::response::Response, AppError> {
    let notifications = state
        .notification_service
        .list_notifications(auth.user_id, query.unread_only, query.limit, query.offset)
        .await?;

    let total = state
        .notification_service
        .count_notifications(auth.user_id, query.unread_only)
        .await? as usize;
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
) -> Result<axum::response::Response, AppError> {
    let count = state
        .notification_service
        .count_unread(auth.user_id)
        .await?;

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
) -> Result<axum::response::Response, AppError> {
    let notification = state
        .notification_service
        .mark_as_read(notification_id, auth.user_id)
        .await?;

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
) -> Result<axum::response::Response, AppError> {
    state
        .notification_service
        .delete_notification(notification_id, auth.user_id)
        .await?;

    Ok((StatusCode::NO_CONTENT, ()).into_response())
}

// ============================================================================
// 4. GET /api/activity - Durable activity feed (server-sourced, permission-filtered)
// ============================================================================

/// List recent activity for the authenticated user.
///
/// GET /api/activity?limit=50&before_timestamp=...&before_id=...
///
/// Queries the event store for recent file/module/share mutations,
/// filters out resources the user cannot currently access, and returns
/// a paginated activity feed.
pub async fn list_activity(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<ListActivityQuery>,
) -> Result<axum::response::Response, AppError> {
    // Clamp limit to a reasonable range.
    let limit = query.limit.clamp(1, 200);

    // Fetch recent mutation events for this tenant.
    let events = state
        .event_store
        .query_recent_events(auth.tenant_id, query.before_timestamp, query.before_id, limit)
        .await?;

    // Build the permission-filtered activity list.
    let mut items = Vec::with_capacity(events.len());
    let mut last_timestamp = None;
    let mut last_id = None;

    for event in &events {
        // Determine whether the user can currently access the resource.
        let can_access = match event.aggregate_type {
            AggregateType::File => {
                state
                    .permission_resolver
                    .check_file_permission(auth.user_id, event.aggregate_id, SharePermissions::View)
                    .await
                    .unwrap_or(false)
            }
            AggregateType::Folder => {
                state
                    .permission_resolver
                    .check_folder_permission(
                        auth.user_id,
                        event.aggregate_id,
                        SharePermissions::View,
                    )
                    .await
                    .unwrap_or(false)
            }
            AggregateType::Share => {
                // Look up the share to find its underlying resource, then check
                // permission on that resource.
                let row = sqlx::query("SELECT file_id, folder_id FROM shares WHERE id = $1")
                    .bind(event.aggregate_id)
                    .fetch_optional(state.event_store.pool())
                    .await;

                match row {
                    Ok(Some(row)) => {
                        let file_id: Option<Uuid> = row.try_get("file_id").unwrap_or(None);
                        let folder_id: Option<Uuid> = row.try_get("folder_id").unwrap_or(None);
                        if let Some(fid) = file_id {
                            state
                                .permission_resolver
                                .check_file_permission(auth.user_id, fid, SharePermissions::View)
                                .await
                                .unwrap_or(false)
                        } else if let Some(fid) = folder_id {
                            state
                                .permission_resolver
                                .check_folder_permission(
                                    auth.user_id,
                                    fid,
                                    SharePermissions::View,
                                )
                                .await
                                .unwrap_or(false)
                        } else {
                            false
                        }
                    }
                    _ => false,
                }
            }
            AggregateType::User => false,
        };

        if !can_access {
            continue;
        }

        let resource_name = extract_resource_name(&event.event_type, &event.payload);

        items.push(ActivityItemResponse {
            id: event.id,
            action: event_type_to_action(&event.event_type).to_string(),
            resource_type: aggregate_type_to_resource_type(
                &event.aggregate_type,
                &event.event_type,
            )
            .to_string(),
            resource_id: event.aggregate_id,
            resource_name,
            actor_id: event.user_id,
            timestamp: event.timestamp.to_rfc3339(),
        });

        last_timestamp = Some(event.timestamp);
        last_id = Some(event.id);
    }

    let next_cursor = if events.len() >= limit as usize {
        // Use the last event from the raw query as the cursor so the next
        // page starts after it, even if it was filtered out.
        if let (Some(ts), Some(id)) = (last_timestamp, last_id) {
            Some(ActivityCursor {
                before_timestamp: ts.to_rfc3339(),
                before_id: id,
            })
        } else if let Some(last_event) = events.last() {
            Some(ActivityCursor {
                before_timestamp: last_event.timestamp.to_rfc3339(),
                before_id: last_event.id,
            })
        } else {
            None
        }
    } else {
        None
    };

    Ok(Json(ListActivityResponse {
        items,
        next_cursor,
    })
    .into_response())
}

/// Map an EventType to a stable snake_case action name.
fn event_type_to_action(event_type: &EventType) -> &'static str {
    match event_type {
        EventType::FileUploaded => "file_uploaded",
        EventType::FileModified => "file_modified",
        EventType::FileRenamed => "file_renamed",
        EventType::FileMoved => "file_moved",
        EventType::FileDeleted => "file_deleted",
        EventType::FileRestored => "file_restored",
        EventType::FolderCreated => "folder_created",
        EventType::FolderRenamed => "folder_renamed",
        EventType::FolderMoved => "folder_moved",
        EventType::FolderDeleted => "folder_deleted",
        EventType::ShareCreated => "share_created",
        EventType::ShareRevoked => "share_revoked",
        EventType::ShareUpdated => "share_updated",
        EventType::ShareReceivedByUser => "share_received",
        EventType::SharePermissionChanged => "share_permission_changed",
        EventType::ShareRevokedFromUser => "share_revoked_from_user",
        EventType::BrainstormBoardModified => "brainstorm_board_modified",
        EventType::MeetingNoteModified => "meeting_note_modified",
        EventType::DecisionModified => "decision_modified",
        EventType::StandupModified => "standup_modified",
        EventType::KanbanModified => "kanban_modified",
        EventType::NoteModified => "note_modified",
        _ => "unknown",
    }
}

/// Map aggregate/event info to a resource type string.
fn aggregate_type_to_resource_type(
    aggregate_type: &AggregateType,
    event_type: &EventType,
) -> &'static str {
    // Module events are stored with AggregateType::File but represent modules.
    match event_type {
        EventType::BrainstormBoardModified
        | EventType::MeetingNoteModified
        | EventType::DecisionModified
        | EventType::StandupModified
        | EventType::KanbanModified
        | EventType::NoteModified => "module",
        _ => match aggregate_type {
            AggregateType::File => "file",
            AggregateType::Folder => "folder",
            AggregateType::Share => "share",
            AggregateType::User => "user",
        },
    }
}

/// Extract a display name from the event payload when available.
fn extract_resource_name(event_type: &EventType, payload: &serde_json::Value) -> Option<String> {
    match event_type {
        EventType::FileUploaded => payload.get("name")?.as_str().map(String::from),
        EventType::FileDeleted => payload.get("file_name")?.as_str().map(String::from),
        EventType::FileRenamed => payload.get("new_name")?.as_str().map(String::from),
        EventType::FolderCreated | EventType::FolderDeleted => {
            payload.get("name")?.as_str().map(String::from)
        }
        EventType::FolderRenamed => payload.get("new_name")?.as_str().map(String::from),
        EventType::BrainstormBoardModified
        | EventType::MeetingNoteModified
        | EventType::DecisionModified
        | EventType::StandupModified
        | EventType::NoteModified => payload.get("title")?.as_str().map(String::from),
        _ => None,
    }
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

    #[test]
    fn test_activity_query_defaults() {
        let json = serde_json::json!({});
        let query: Result<ListActivityQuery, _> = serde_json::from_value(json);
        assert!(query.is_ok());
        let query = query.unwrap();
        assert_eq!(query.limit, 50);
        assert!(query.before_timestamp.is_none());
        assert!(query.before_id.is_none());
    }

    #[test]
    fn test_event_type_to_action_mapping() {
        assert_eq!(event_type_to_action(&EventType::FileUploaded), "file_uploaded");
        assert_eq!(event_type_to_action(&EventType::ShareCreated), "share_created");
        assert_eq!(
            event_type_to_action(&EventType::BrainstormBoardModified),
            "brainstorm_board_modified"
        );
    }

    #[test]
    fn test_extract_resource_name() {
        let payload = serde_json::json!({"name": "test.txt"});
        assert_eq!(
            extract_resource_name(&EventType::FileUploaded, &payload),
            Some("test.txt".to_string())
        );

        let payload = serde_json::json!({"file_name": "old.txt"});
        assert_eq!(
            extract_resource_name(&EventType::FileDeleted, &payload),
            Some("old.txt".to_string())
        );

        let payload = serde_json::json!({"title": "My Decision"});
        assert_eq!(
            extract_resource_name(&EventType::DecisionModified, &payload),
            Some("My Decision".to_string())
        );
    }
}
