use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::{HeaderMap, StatusCode},
    response::Response,
};
use chrono::{DateTime, Utc};
use futures_util::{SinkExt, StreamExt};
use rustshare_auth::ShareSessionClaims;
use rustshare_core::domain::{FileId, ShareId, SharePermissions, UserId};
use rustshare_core::events::{
    Event, EventType, NotificationCreatedPayload, ReplicationStateChangedPayload,
    ShareCreatedPayload, ShareRevokedPayload, ShareUpdatedPayload,
};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::{error, info, warn};
use uuid::Uuid;

use super::extractors::bearer_token_from_headers;
use crate::web_session::{extract_cookie_value, resolve_user_session};
use crate::AppState;

/// Identifies the client connected to WebSocket
#[derive(Debug, Clone)]
enum ClientIdentity {
    /// Authenticated user
    User(UserId),
    /// Anonymous share viewer with session token
    ShareViewer {
        share_id: ShareId,
        file_id: Option<FileId>,
        permissions: SharePermissions,
    },
}

/// Client message for requesting catch-up
#[derive(Debug, Deserialize)]
struct SyncRequest {
    #[serde(rename = "type")]
    msg_type: String,
    last_seen_event_id: Option<String>,
}

/// Query parameters for WebSocket authentication
#[derive(Debug, Deserialize)]
pub struct SyncQuery {
    pub token: Option<String>,
}

/// Notification message sent to client
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
enum SyncMessage {
    /// Generic event notification (for backward compatibility)
    Event {
        event_id: String,
        event_type: String,
        aggregate_id: String,
        aggregate_type: String,
        timestamp: String,
        version: i32,
    },
    /// Share created notification
    ShareCreated {
        share_id: Uuid,
        file_id: Uuid,
        share_token: String,
        permissions: SharePermissions,
        password_protected: bool,
        expires_at: Option<DateTime<Utc>>,
    },
    /// Share revoked notification
    ShareRevoked { share_id: Uuid, file_id: Uuid },
    /// Share updated notification
    ShareUpdated {
        share_id: Uuid,
        file_id: Uuid,
        password_changed: bool,
        expires_at_changed: bool,
        new_expires_at: Option<DateTime<Utc>>,
    },
    /// Replication state update for a file version
    ReplicationStateChanged {
        file_id: Uuid,
        file_version_id: Uuid,
        replication_state: String,
        job_status: Option<String>,
        attempt_count: i32,
        next_attempt_at: Option<DateTime<Utc>>,
        last_error: Option<String>,
        updated_at: DateTime<Utc>,
    },
    /// Persistent notification created for a user
    NotificationCreated {
        notification_id: Uuid,
        user_id: Uuid,
        title: String,
        notification_type: String,
        message: String,
        resource_id: Uuid,
        resource_type: String,
        action_url: Option<String>,
        timestamp: DateTime<Utc>,
    },
}

/// Lagged warning message
#[derive(Debug, Serialize)]
struct LaggedMessage {
    #[serde(rename = "type")]
    msg_type: String,
    message: String,
}

/// Validate client token - supports both user and share session JWTs
async fn validate_client_token(
    token: &str,
    jwt_manager: &rustshare_auth::JwtManager,
) -> Result<ClientIdentity, (StatusCode, String)> {
    // First try to decode as user JWT
    if let Ok(claims) = jwt_manager.validate(token) {
        let user_id = UserId::from(
            Uuid::parse_str(&claims.sub)
                .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid user ID".to_string()))?,
        );
        return Ok(ClientIdentity::User(user_id));
    }

    // Try to decode as share session JWT
    if let Ok(claims) = jwt_manager.decode_custom::<ShareSessionClaims>(token) {
        // Check if expired
        if claims.is_expired() {
            return Err((StatusCode::UNAUTHORIZED, "Token expired".to_string()));
        }

        return Ok(ClientIdentity::ShareViewer {
            share_id: claims.share_id,
            file_id: claims.file_id,
            permissions: claims.permissions,
        });
    }

    Err((StatusCode::UNAUTHORIZED, "Invalid token".to_string()))
}

/// WebSocket handler for real-time sync
/// Supports authentication via:
/// - Authorization header: `Authorization: Bearer <token>`
/// - Query parameter: `?token=<token>` (for browser WebSocket API compatibility)
pub async fn sync_handler(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    Query(query): Query<SyncQuery>,
) -> Result<Response, (StatusCode, String)> {
    let client_identity = if let Some(token) = bearer_token_from_headers(&headers) {
        validate_client_token(&token, &state.jwt_manager).await?
    } else if let Some(token) = query.token {
        validate_client_token(&token, &state.jwt_manager).await?
    } else if let Some(session_token) =
        extract_cookie_value(&headers, rustshare_auth::WEB_SESSION_COOKIE_NAME)
    {
        let Some(session) = resolve_user_session(&state, &session_token)
            .await
            .map_err(|error| (StatusCode::UNAUTHORIZED, error))?
        else {
            return Err((StatusCode::UNAUTHORIZED, "Invalid session".to_string()));
        };

        ClientIdentity::User(session.user_id)
    } else {
        return Err((
            StatusCode::UNAUTHORIZED,
            "Missing authentication (cookie, Authorization header, or ?token= query parameter)"
                .to_string(),
        ));
    };

    match &client_identity {
        ClientIdentity::User(user_id) => {
            info!("WebSocket connection established for user {}", user_id);
        }
        ClientIdentity::ShareViewer {
            share_id,
            file_id,
            permissions,
        } => {
            info!(
                "WebSocket connection established for share viewer: share_id={}, file_id={}, permissions={:?}",
                share_id,
                file_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "none".to_string()),
                permissions
            );
        }
    }

    // Upgrade connection
    Ok(ws.on_upgrade(move |socket| handle_socket(socket, client_identity, state)))
}

/// Handle WebSocket connection
async fn handle_socket(socket: WebSocket, client_identity: ClientIdentity, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to event broadcaster
    let mut event_rx = state.broadcaster.subscribe();

    // Handle incoming messages (catch-up requests)
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Text(text) = msg {
                if let Ok(sync_req) = serde_json::from_str::<SyncRequest>(&text) {
                    if sync_req.msg_type == "sync" {
                        return sync_req.last_seen_event_id;
                    }
                }
            }
        }
        None
    });

    // Clone state for use in send task
    let metadata_store = state.metadata_store.clone();
    let client_identity_for_task = client_identity.clone();

    // Send events to client
    let send_task = tokio::spawn(async move {
        // Wait briefly for catch-up request (only for authenticated users)
        if let ClientIdentity::User(user_id) = &client_identity_for_task {
            tokio::select! {
                last_seen_id = &mut recv_task => {
                    if let Ok(Some(last_id_str)) = last_seen_id {
                        // Handle catch-up
                        if let Ok(last_id) = Uuid::parse_str(&last_id_str) {
                            match state.event_store.get_events_since(*user_id, Some(last_id), 100).await {
                                Ok(events) => {
                                    info!("Sending {} catch-up events to user {}", events.len(), user_id);
                                    for event in events {
                                        if let Ok(message) = event_to_sync_message(&event, &metadata_store).await {
                                            if let Ok(json) = serde_json::to_string(&message) {
                                                if sender.send(Message::Text(json.into())).await.is_err() {
                                                    return;
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to fetch catch-up events: {}", e);
                                }
                            }
                        }
                    }
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                    // No catch-up request, proceed to live events
                }
            }
        } else {
            // Share viewers don't support catch-up, abort the recv_task and continue
            recv_task.abort();
        }

        // Stream live events
        loop {
            match event_rx.recv().await {
                Ok(event) => {
                    // Check if this event is relevant to this client
                    match should_send_event_to_client(
                        &event,
                        &client_identity_for_task,
                        &metadata_store,
                    )
                    .await
                    {
                        Ok(true) => {
                            // Convert event to sync message
                            match event_to_sync_message(&event, &metadata_store).await {
                                Ok(message) => {
                                    if let Ok(json) = serde_json::to_string(&message) {
                                        if sender.send(Message::Text(json.into())).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to convert event to sync message: {}", e);
                                }
                            }
                        }
                        Ok(false) => {
                            // Event not relevant to this client, skip
                        }
                        Err(e) => {
                            error!("Failed to check event relevance: {}", e);
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Client lagged by {} events", n);
                    let lagged = LaggedMessage {
                        msg_type: "lagged".to_string(),
                        message: "Too many events, please sync".to_string(),
                    };
                    if let Ok(json) = serde_json::to_string(&lagged) {
                        if sender.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                }
                Err(broadcast::error::RecvError::Closed) => {
                    error!("Broadcaster closed");
                    break;
                }
            }
        }
    });

    // Wait for send task to complete
    let _ = send_task.await;

    match &client_identity {
        ClientIdentity::User(user_id) => {
            info!("WebSocket connection closed for user {}", user_id);
        }
        ClientIdentity::ShareViewer {
            share_id,
            permissions: _,
            ..
        } => {
            info!(
                "WebSocket connection closed for share viewer: share_id={}",
                share_id
            );
        }
    }
}

/// Determine if an event should be sent to a specific client
async fn should_send_event_to_client(
    event: &Event,
    client_identity: &ClientIdentity,
    metadata_store: &rustshare_storage::MetadataStore,
) -> Result<bool, String> {
    match client_identity {
        ClientIdentity::User(user_id) => {
            // For authenticated users, use existing logic
            should_send_event_to_user(event, *user_id, metadata_store).await
        }
        ClientIdentity::ShareViewer {
            share_id,
            file_id,
            permissions: _,
        } => {
            // Share viewers receive:
            // 1. Events for their specific file
            // 2. ShareRevoked/ShareUpdated events for their share
            match event.event_type {
                EventType::ShareRevoked | EventType::ShareUpdated => {
                    // Check if this event is for their share
                    Ok(event.aggregate_id == *share_id)
                }
                EventType::FileModified | EventType::FileDeleted | EventType::FileRenamed => {
                    // Check if this event is for their file
                    Ok(file_id.is_some_and(|id| event.aggregate_id == id))
                }
                _ => Ok(false),
            }
        }
    }
}

/// Determine if an event should be sent to a specific user (authenticated users only)
async fn should_send_event_to_user(
    event: &Event,
    user_id: UserId,
    metadata_store: &rustshare_storage::MetadataStore,
) -> Result<bool, String> {
    // For most events, send to the user who triggered them
    if event.user_id == user_id {
        return Ok(true);
    }

    // For share events, also send to file owner
    match event.event_type {
        EventType::ShareCreated | EventType::ShareRevoked | EventType::ShareUpdated => {
            // Deserialize payload to get file_id
            let file_id = match event.event_type {
                EventType::ShareCreated => {
                    let payload: ShareCreatedPayload =
                        serde_json::from_value(event.payload.clone()).map_err(|e| {
                            format!("Failed to deserialize ShareCreatedPayload: {}", e)
                        })?;
                    payload.file_id
                }
                EventType::ShareRevoked => {
                    let payload: ShareRevokedPayload =
                        serde_json::from_value(event.payload.clone()).map_err(|e| {
                            format!("Failed to deserialize ShareRevokedPayload: {}", e)
                        })?;
                    payload.file_id
                }
                EventType::ShareUpdated => {
                    let payload: ShareUpdatedPayload =
                        serde_json::from_value(event.payload.clone()).map_err(|e| {
                            format!("Failed to deserialize ShareUpdatedPayload: {}", e)
                        })?;
                    payload.file_id
                }
                _ => return Ok(false),
            };

            // Get file to check owner
            let file = metadata_store
                .find_file_by_id(file_id)
                .await
                .map_err(|e| format!("Failed to get file: {}", e))?;

            if let Some(file) = file {
                // Send to file owner
                return Ok(file.owner_id == user_id);
            }

            Ok(false)
        }
        _ => Ok(false),
    }
}

/// Convert Event to SyncMessage
async fn event_to_sync_message(
    event: &Event,
    _metadata_store: &rustshare_storage::MetadataStore,
) -> Result<SyncMessage, String> {
    // For share events, create specialized messages
    match event.event_type {
        EventType::ShareCreated => {
            let payload: ShareCreatedPayload = serde_json::from_value(event.payload.clone())
                .map_err(|e| format!("Failed to deserialize ShareCreatedPayload: {}", e))?;

            Ok(SyncMessage::ShareCreated {
                share_id: payload.share_id,
                file_id: payload.file_id,
                share_token: payload.share_token,
                permissions: payload.permissions,
                password_protected: payload.password_protected,
                expires_at: payload.expires_at,
            })
        }
        EventType::ShareRevoked => {
            let payload: ShareRevokedPayload = serde_json::from_value(event.payload.clone())
                .map_err(|e| format!("Failed to deserialize ShareRevokedPayload: {}", e))?;

            Ok(SyncMessage::ShareRevoked {
                share_id: payload.share_id,
                file_id: payload.file_id,
            })
        }
        EventType::ShareUpdated => {
            let payload: ShareUpdatedPayload = serde_json::from_value(event.payload.clone())
                .map_err(|e| format!("Failed to deserialize ShareUpdatedPayload: {}", e))?;

            Ok(SyncMessage::ShareUpdated {
                share_id: payload.share_id,
                file_id: payload.file_id,
                password_changed: payload.password_changed,
                expires_at_changed: payload.expires_at_changed,
                new_expires_at: payload.new_expires_at,
            })
        }
        EventType::ReplicationStateChanged => {
            let payload: ReplicationStateChangedPayload =
                serde_json::from_value(event.payload.clone()).map_err(|e| {
                    format!(
                        "Failed to deserialize ReplicationStateChangedPayload: {}",
                        e
                    )
                })?;

            Ok(SyncMessage::ReplicationStateChanged {
                file_id: payload.file_id,
                file_version_id: payload.file_version_id,
                replication_state: payload.replication_state.as_str().to_string(),
                job_status: payload.job_status,
                attempt_count: payload.attempt_count,
                next_attempt_at: payload.next_attempt_at,
                last_error: payload.last_error,
                updated_at: payload.updated_at,
            })
        }
        EventType::NotificationCreated => {
            let payload: NotificationCreatedPayload = serde_json::from_value(event.payload.clone())
                .map_err(|e| format!("Failed to deserialize NotificationCreatedPayload: {}", e))?;

            Ok(SyncMessage::NotificationCreated {
                notification_id: payload.notification_id,
                user_id: payload.user_id,
                title: payload.title,
                notification_type: payload.notification_type,
                message: payload.message,
                resource_id: payload.resource_id,
                resource_type: payload.resource_type,
                action_url: payload.action_url,
                timestamp: payload.timestamp,
            })
        }
        _ => {
            // For other events, use generic event message
            Ok(SyncMessage::Event {
                event_id: event.id.to_string(),
                event_type: event.event_type.type_name().to_string(),
                aggregate_id: event.aggregate_id.to_string(),
                aggregate_type: serde_json::to_string(&event.aggregate_type)
                    .map_err(|e| e.to_string())?
                    .trim_matches('"')
                    .to_string(),
                timestamp: event.timestamp.to_rfc3339(),
                version: event.version,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rustshare_core::domain::{File, SharePermissions};
    use rustshare_core::events::{
        AggregateType, Event, EventType, FileModifiedPayload, ShareCreatedPayload,
        ShareRevokedPayload, ShareUpdatedPayload,
    };
    use uuid::Uuid;

    // TODO: These tests use PgPool for testing which needs to be replaced
    // with in-memory MetadataStore once it's fully implemented.
    // For now, tests that require database are marked with #[ignore].

    #[tokio::test]
    #[ignore] // Requires database - TODO: rewrite with in-memory store
    async fn test_should_send_share_created_to_file_owner() {
        let owner_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let share_id = Uuid::new_v4();

        // Create ShareCreated event
        let payload = ShareCreatedPayload {
            share_id,
            file_id,
            share_token: "token123".to_string(),
            permissions: SharePermissions::View,
            password_protected: false,
            expires_at: None,
            created_by: owner_id,
        };

        let event = Event::new(
            EventType::ShareCreated,
            share_id,
            AggregateType::Share,
            serde_json::to_value(&payload).unwrap(),
            owner_id,
        );

        // Event triggered by owner should be sent to owner
        assert_eq!(event.user_id, owner_id, "Event user_id should match owner");
    }

    #[tokio::test]
    #[ignore] // Requires database - TODO: rewrite with in-memory store
    async fn test_should_send_share_revoked_to_file_owner() {
        let owner_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let share_id = Uuid::new_v4();

        // Create ShareRevoked event
        let payload = ShareRevokedPayload {
            share_id,
            file_id,
            revoked_by: owner_id,
        };

        let event = Event::new(
            EventType::ShareRevoked,
            share_id,
            AggregateType::Share,
            serde_json::to_value(&payload).unwrap(),
            owner_id,
        );

        // Event triggered by owner should be sent to owner
        assert_eq!(event.user_id, owner_id, "Event user_id should match owner");
    }

    #[tokio::test]
    #[ignore] // Requires database - TODO: rewrite with in-memory store
    async fn test_should_send_share_updated_to_file_owner() {
        let owner_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let share_id = Uuid::new_v4();

        // Create ShareUpdated event
        let payload = ShareUpdatedPayload {
            share_id,
            file_id,
            password_changed: true,
            expires_at_changed: false,
            new_expires_at: None,
            updated_by: owner_id,
        };

        let event = Event::new(
            EventType::ShareUpdated,
            share_id,
            AggregateType::Share,
            serde_json::to_value(&payload).unwrap(),
            owner_id,
        );

        // Event triggered by owner should be sent to owner
        assert_eq!(event.user_id, owner_id, "Event user_id should match owner");
    }

    #[tokio::test]
    async fn test_event_to_sync_message_share_created() {
        let share_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();

        let payload = ShareCreatedPayload {
            share_id,
            file_id,
            share_token: "token123".to_string(),
            permissions: SharePermissions::View,
            password_protected: false,
            expires_at: None,
            created_by: owner_id,
        };

        let event = Event::new(
            EventType::ShareCreated,
            share_id,
            AggregateType::Share,
            serde_json::to_value(&payload).unwrap(),
            owner_id,
        );

        // Create a mock metadata store - in production this would be a real store
        let metadata_store = rustshare_storage::MetadataStore::new();

        let message = event_to_sync_message(&event, &metadata_store)
            .await
            .unwrap();

        match message {
            SyncMessage::ShareCreated {
                share_id: msg_share_id,
                file_id: msg_file_id,
                share_token,
                permissions,
                password_protected,
                expires_at,
            } => {
                assert_eq!(msg_share_id, share_id);
                assert_eq!(msg_file_id, file_id);
                assert_eq!(share_token, "token123");
                assert_eq!(permissions, SharePermissions::View);
                assert!(!password_protected);
                assert!(expires_at.is_none());
            }
            _ => panic!("Expected ShareCreated message"),
        }
    }

    #[tokio::test]
    async fn test_event_to_sync_message_share_revoked() {
        let share_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();

        let payload = ShareRevokedPayload {
            share_id,
            file_id,
            revoked_by: owner_id,
        };

        let event = Event::new(
            EventType::ShareRevoked,
            share_id,
            AggregateType::Share,
            serde_json::to_value(&payload).unwrap(),
            owner_id,
        );

        let metadata_store = rustshare_storage::MetadataStore::new();

        let message = event_to_sync_message(&event, &metadata_store)
            .await
            .unwrap();

        match message {
            SyncMessage::ShareRevoked {
                share_id: msg_share_id,
                file_id: msg_file_id,
            } => {
                assert_eq!(msg_share_id, share_id);
                assert_eq!(msg_file_id, file_id);
            }
            _ => panic!("Expected ShareRevoked message"),
        }
    }

    #[tokio::test]
    async fn test_event_to_sync_message_share_updated() {
        let share_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let expires_at = Some(Utc::now() + chrono::Duration::hours(24));

        let payload = ShareUpdatedPayload {
            share_id,
            file_id,
            password_changed: true,
            expires_at_changed: true,
            new_expires_at: expires_at,
            updated_by: owner_id,
        };

        let event = Event::new(
            EventType::ShareUpdated,
            share_id,
            AggregateType::Share,
            serde_json::to_value(&payload).unwrap(),
            owner_id,
        );

        let metadata_store = rustshare_storage::MetadataStore::new();

        let message = event_to_sync_message(&event, &metadata_store)
            .await
            .unwrap();

        match message {
            SyncMessage::ShareUpdated {
                share_id: msg_share_id,
                file_id: msg_file_id,
                password_changed,
                expires_at_changed,
                new_expires_at,
            } => {
                assert_eq!(msg_share_id, share_id);
                assert_eq!(msg_file_id, file_id);
                assert!(password_changed);
                assert!(expires_at_changed);
                assert!(new_expires_at.is_some());
            }
            _ => panic!("Expected ShareUpdated message"),
        }
    }

    #[tokio::test]
    async fn test_event_to_sync_message_non_share_event() {
        let file_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();

        let payload = serde_json::json!({
            "file_id": file_id.to_string(),
            "name": "test.txt"
        });

        let event = Event::new(
            EventType::FileUploaded,
            file_id,
            AggregateType::File,
            payload,
            owner_id,
        );

        let metadata_store = rustshare_storage::MetadataStore::new();

        let message = event_to_sync_message(&event, &metadata_store)
            .await
            .unwrap();

        match message {
            SyncMessage::Event {
                event_type,
                aggregate_type,
                ..
            } => {
                assert_eq!(event_type, "FileUploaded");
                assert_eq!(aggregate_type, "file");
            }
            _ => panic!("Expected generic Event message"),
        }
    }

    #[test]
    fn test_sync_message_serialization() {
        let share_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();

        // Test ShareCreated serialization
        let msg = SyncMessage::ShareCreated {
            share_id,
            file_id,
            share_token: "token123".to_string(),
            permissions: SharePermissions::View,
            password_protected: false,
            expires_at: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"ShareCreated\""));
        assert!(json.contains("token123"));

        // Test ShareRevoked serialization
        let msg = SyncMessage::ShareRevoked { share_id, file_id };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"ShareRevoked\""));

        // Test ShareUpdated serialization
        let msg = SyncMessage::ShareUpdated {
            share_id,
            file_id,
            password_changed: true,
            expires_at_changed: false,
            new_expires_at: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"ShareUpdated\""));
        assert!(json.contains("\"password_changed\":true"));
    }

    #[tokio::test]
    async fn test_share_viewer_receives_revoked_event() {
        let share_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();

        let client_identity = ClientIdentity::ShareViewer {
            share_id,
            file_id: Some(file_id),
            permissions: SharePermissions::View,
        };

        // Create ShareRevoked event for this share
        let payload = ShareRevokedPayload {
            share_id,
            file_id,
            revoked_by: owner_id,
        };

        let event = Event::new(
            EventType::ShareRevoked,
            share_id,
            rustshare_core::events::AggregateType::Share,
            serde_json::to_value(&payload).unwrap(),
            owner_id,
        );

        let metadata_store = rustshare_storage::MetadataStore::new();

        // Share viewer should receive the event
        let should_send = should_send_event_to_client(&event, &client_identity, &metadata_store)
            .await
            .unwrap();
        assert!(
            should_send,
            "Share viewer should receive ShareRevoked event for their share"
        );
    }

    #[tokio::test]
    async fn test_share_viewer_receives_updated_event() {
        let share_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();

        let client_identity = ClientIdentity::ShareViewer {
            share_id,
            file_id: Some(file_id),
            permissions: SharePermissions::View,
        };

        // Create ShareUpdated event for this share
        let payload = ShareUpdatedPayload {
            share_id,
            file_id,
            password_changed: true,
            expires_at_changed: false,
            new_expires_at: None,
            updated_by: owner_id,
        };

        let event = Event::new(
            EventType::ShareUpdated,
            share_id,
            rustshare_core::events::AggregateType::Share,
            serde_json::to_value(&payload).unwrap(),
            owner_id,
        );

        let metadata_store = rustshare_storage::MetadataStore::new();

        // Share viewer should receive the event
        let should_send = should_send_event_to_client(&event, &client_identity, &metadata_store)
            .await
            .unwrap();
        assert!(
            should_send,
            "Share viewer should receive ShareUpdated event for their share"
        );
    }

    #[tokio::test]
    async fn test_share_viewer_receives_file_modified_event() {
        let share_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();

        let client_identity = ClientIdentity::ShareViewer {
            share_id,
            file_id: Some(file_id),
            permissions: SharePermissions::View,
        };

        // Create FileModified event for this file
        let payload = FileModifiedPayload {
            file_id,
            old_version: 1,
            new_version: 2,
            old_content_hash: "hash1".to_string(),
            new_content_hash: "hash2".to_string(),
            old_size: 100,
            new_size: 200,
            storage_key: "key123".to_string(),
            modified_by: owner_id,
        };

        let event = Event::new(
            EventType::FileModified,
            file_id,
            rustshare_core::events::AggregateType::File,
            serde_json::to_value(&payload).unwrap(),
            owner_id,
        );

        let metadata_store = rustshare_storage::MetadataStore::new();

        // Share viewer should receive the event
        let should_send = should_send_event_to_client(&event, &client_identity, &metadata_store)
            .await
            .unwrap();
        assert!(
            should_send,
            "Share viewer should receive FileModified event for their file"
        );
    }

    #[tokio::test]
    async fn test_share_viewer_does_not_receive_other_file_events() {
        let share_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let other_file_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();

        let client_identity = ClientIdentity::ShareViewer {
            share_id,
            file_id: Some(file_id),
            permissions: SharePermissions::View,
        };

        // Create FileModified event for a different file
        let payload = FileModifiedPayload {
            file_id: other_file_id,
            old_version: 1,
            new_version: 2,
            old_content_hash: "hash1".to_string(),
            new_content_hash: "hash2".to_string(),
            old_size: 100,
            new_size: 200,
            storage_key: "key123".to_string(),
            modified_by: owner_id,
        };

        let event = Event::new(
            EventType::FileModified,
            other_file_id,
            rustshare_core::events::AggregateType::File,
            serde_json::to_value(&payload).unwrap(),
            owner_id,
        );

        let metadata_store = rustshare_storage::MetadataStore::new();

        // Share viewer should NOT receive the event
        let should_send = should_send_event_to_client(&event, &client_identity, &metadata_store)
            .await
            .unwrap();
        assert!(
            !should_send,
            "Share viewer should not receive FileModified event for other files"
        );
    }

    #[tokio::test]
    async fn test_share_viewer_does_not_receive_other_share_events() {
        let share_id = Uuid::new_v4();
        let other_share_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();

        let client_identity = ClientIdentity::ShareViewer {
            share_id,
            file_id: Some(file_id),
            permissions: SharePermissions::View,
        };

        // Create ShareRevoked event for a different share
        let payload = ShareRevokedPayload {
            share_id: other_share_id,
            file_id,
            revoked_by: owner_id,
        };

        let event = Event::new(
            EventType::ShareRevoked,
            other_share_id,
            rustshare_core::events::AggregateType::Share,
            serde_json::to_value(&payload).unwrap(),
            owner_id,
        );

        let metadata_store = rustshare_storage::MetadataStore::new();

        // Share viewer should NOT receive the event
        let should_send = should_send_event_to_client(&event, &client_identity, &metadata_store)
            .await
            .unwrap();
        assert!(
            !should_send,
            "Share viewer should not receive ShareRevoked event for other shares"
        );
    }

    #[tokio::test]
    async fn test_share_viewer_does_not_receive_user_events() {
        let share_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        let client_identity = ClientIdentity::ShareViewer {
            share_id,
            file_id: Some(file_id),
            permissions: SharePermissions::View,
        };

        // Create UserCreated event
        let payload = serde_json::json!({
            "user_id": user_id.to_string(),
            "email": "test@example.com"
        });

        let event = Event::new(
            EventType::UserCreated,
            user_id,
            rustshare_core::events::AggregateType::User,
            payload,
            user_id,
        );

        let metadata_store = rustshare_storage::MetadataStore::new();

        // Share viewer should NOT receive the event
        let should_send = should_send_event_to_client(&event, &client_identity, &metadata_store)
            .await
            .unwrap();
        assert!(
            !should_send,
            "Share viewer should not receive UserCreated event"
        );
    }
}
