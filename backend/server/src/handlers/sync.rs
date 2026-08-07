use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use chrono::{DateTime, Utc};
use futures_util::{SinkExt, StreamExt};
use rustshare_core::domain::{SharePermissions, UserId};
use rustshare_core::events::{
    Event, EventType, NotificationCreatedPayload, ReplicationStateChangedPayload,
    ShareCreatedPayload, ShareRevokedPayload, ShareUpdatedPayload,
};
use rustshare_storage::repos::sync::{DeltaResult, SyncCursor, SyncDelta};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::{error, info, warn};
use uuid::Uuid;

use super::ws_auth::{resolve_ws_client_identity, ClientIdentity, WsAuthQuery};
use super::AuthenticatedUser;
use crate::handlers::AppError;
use crate::AppState;

/// Client message for requesting catch-up
#[derive(Debug, Deserialize)]
struct SyncRequest {
    #[serde(rename = "type")]
    msg_type: String,
    last_seen_event_id: Option<String>,
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

/// WebSocket handler for real-time sync
/// Supports authentication via:
/// - Authorization header: `Authorization: Bearer <token>`
/// - Query parameter: `?token=<token>` (for browser WebSocket API compatibility)
pub async fn sync_handler(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    Query(query): Query<WsAuthQuery>,
) -> Result<Response, (StatusCode, String)> {
    let client_identity = resolve_ws_client_identity(&state, &headers, &query).await?;

    match &client_identity {
        ClientIdentity::User { user_id, .. } => {
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
        if let ClientIdentity::User { user_id, .. } = &client_identity_for_task {
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

    // Wait for send task to complete - log but don't fail on error
    if let Err(e) = send_task.await {
        tracing::debug!(error = %e, "send task ended with error");
    }

    match &client_identity {
        ClientIdentity::User { user_id, .. } => {
            info!("WebSocket connection closed for user {}", user_id);
        }
        ClientIdentity::ShareViewer { share_id, .. } => {
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
        ClientIdentity::User { user_id, .. } => {
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

            // Get file to check owner (unchecked: we need owner info for sync filtering)
            let file = metadata_store
                .find_file_by_id_unchecked(file_id)
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
    use rustshare_core::domain::{File, SharePermissions, User};
    use rustshare_core::events::{
        AggregateType, Event, EventType, FileModifiedPayload, ShareCreatedPayload,
        ShareRevokedPayload, ShareUpdatedPayload,
    };
    use rustshare_storage::MetadataStore;
    use sqlx::PgPool;
    use uuid::Uuid;

    const TEST_DATABASE_URL: &str = "postgres://rustshare:rustshare@localhost/rustshare_test";
    const TEST_SHARE_TOKEN: &str = "test-share-token";

    /// Helper to create a test metadata store
    async fn create_test_metadata_store() -> Result<(MetadataStore, PgPool), sqlx::Error> {
        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| TEST_DATABASE_URL.to_string());

        let pool = PgPool::connect(&database_url).await?;

        let metadata_store = MetadataStore::new(pool.clone());
        Ok((metadata_store, pool))
    }

    async fn create_test_user(
        metadata_store: &MetadataStore,
        owner_id: Uuid,
        tenant_id: Uuid,
    ) -> anyhow::Result<User> {
        let mut user = User::new(
            format!("sync_owner_{}", &owner_id.to_string()[..8]),
            "Sync Owner".to_string(),
            "test_password_hash".to_string(),
            format!("sync_owner_{}@test.local", &owner_id.to_string()[..8]),
            false,
            10_737_418_240,
            tenant_id,
        );
        user.id = owner_id;
        metadata_store.create_user(&user).await?;
        Ok(user)
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn test_should_send_share_created_to_file_owner() {
        let (metadata_store, _pool) = create_test_metadata_store()
            .await
            .expect("Failed to create test metadata store");

        let owner_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let share_id = Uuid::new_v4();

        create_test_user(&metadata_store, owner_id, tenant_id)
            .await
            .expect("Failed to create test user");

        // Create a file
        let file = File::new(
            "test.txt".to_string(),
            "/test.txt".to_string(),
            "hash123".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
            tenant_id,
        );
        metadata_store.create_file(&file).await.unwrap();

        // Create ShareCreated event
        let payload = ShareCreatedPayload {
            share_id,
            file_id,
            share_token: TEST_SHARE_TOKEN.to_string(),
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

        // File owner should receive the event
        let should_send = should_send_event_to_user(&event, owner_id, &metadata_store)
            .await
            .unwrap();
        assert!(should_send, "File owner should receive ShareCreated event");

        // Other users should not receive the event
        let other_user = Uuid::new_v4();
        let should_send = should_send_event_to_user(&event, other_user, &metadata_store)
            .await
            .unwrap();
        assert!(
            !should_send,
            "Other users should not receive ShareCreated event"
        );
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn test_should_send_share_revoked_to_file_owner() {
        let (metadata_store, _pool) = create_test_metadata_store()
            .await
            .expect("Failed to create test metadata store");

        let owner_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let share_id = Uuid::new_v4();

        create_test_user(&metadata_store, owner_id, tenant_id)
            .await
            .expect("Failed to create test user");

        // Create a file
        let file = File::new(
            "test.txt".to_string(),
            "/test.txt".to_string(),
            "hash123".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
            tenant_id,
        );
        metadata_store.create_file(&file).await.unwrap();

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

        // File owner should receive the event
        let should_send = should_send_event_to_user(&event, owner_id, &metadata_store)
            .await
            .unwrap();
        assert!(should_send, "File owner should receive ShareRevoked event");

        // Other users should not receive the event
        let other_user = Uuid::new_v4();
        let should_send = should_send_event_to_user(&event, other_user, &metadata_store)
            .await
            .unwrap();
        assert!(
            !should_send,
            "Other users should not receive ShareRevoked event"
        );
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn test_should_send_share_updated_to_file_owner() {
        let (metadata_store, _pool) = create_test_metadata_store()
            .await
            .expect("Failed to create test metadata store");

        let owner_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let share_id = Uuid::new_v4();

        create_test_user(&metadata_store, owner_id, tenant_id)
            .await
            .expect("Failed to create test user");

        // Create a file
        let file = File::new(
            "test.txt".to_string(),
            "/test.txt".to_string(),
            "hash123".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
            tenant_id,
        );
        metadata_store.create_file(&file).await.unwrap();

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

        // File owner should receive the event
        let should_send = should_send_event_to_user(&event, owner_id, &metadata_store)
            .await
            .unwrap();
        assert!(should_send, "File owner should receive ShareUpdated event");

        // Other users should not receive the event
        let other_user = Uuid::new_v4();
        let should_send = should_send_event_to_user(&event, other_user, &metadata_store)
            .await
            .unwrap();
        assert!(
            !should_send,
            "Other users should not receive ShareUpdated event"
        );
    }

    #[tokio::test]
    async fn test_event_to_sync_message_share_created() {
        let share_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();

        let payload = ShareCreatedPayload {
            share_id,
            file_id,
            share_token: TEST_SHARE_TOKEN.to_string(),
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

        // We don't need actual database connection for this test
        // as event_to_sync_message doesn't use metadata_store for ShareCreated events
        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| TEST_DATABASE_URL.to_string());

        // Skip test if database is not available
        let Ok(pool) = PgPool::connect(&database_url).await else {
            println!("Skipping test - database not available");
            return;
        };
        let metadata_store = MetadataStore::new(pool);

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
                assert_eq!(share_token, TEST_SHARE_TOKEN);
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

        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| TEST_DATABASE_URL.to_string());

        // Skip test if database is not available
        let Ok(pool) = PgPool::connect(&database_url).await else {
            println!("Skipping test - database not available");
            return;
        };
        let metadata_store = MetadataStore::new(pool);

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

        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| TEST_DATABASE_URL.to_string());

        // Skip test if database is not available
        let Ok(pool) = PgPool::connect(&database_url).await else {
            println!("Skipping test - database not available");
            return;
        };
        let metadata_store = MetadataStore::new(pool);

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

        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| TEST_DATABASE_URL.to_string());

        // Skip test if database is not available
        let Ok(pool) = PgPool::connect(&database_url).await else {
            println!("Skipping test - database not available");
            return;
        };
        let metadata_store = MetadataStore::new(pool);

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
            share_token: TEST_SHARE_TOKEN.to_string(),
            permissions: SharePermissions::View,
            password_protected: false,
            expires_at: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"ShareCreated\""));
        assert!(json.contains(TEST_SHARE_TOKEN));

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

        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| TEST_DATABASE_URL.to_string());

        let Ok(pool) = sqlx::PgPool::connect(&database_url).await else {
            println!("Skipping test - database not available");
            return;
        };
        let metadata_store = MetadataStore::new(pool);

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

        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| TEST_DATABASE_URL.to_string());

        let Ok(pool) = sqlx::PgPool::connect(&database_url).await else {
            println!("Skipping test - database not available");
            return;
        };
        let metadata_store = MetadataStore::new(pool);

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

        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| TEST_DATABASE_URL.to_string());

        let Ok(pool) = sqlx::PgPool::connect(&database_url).await else {
            println!("Skipping test - database not available");
            return;
        };
        let metadata_store = MetadataStore::new(pool);

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

        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| TEST_DATABASE_URL.to_string());

        let Ok(pool) = sqlx::PgPool::connect(&database_url).await else {
            println!("Skipping test - database not available");
            return;
        };
        let metadata_store = MetadataStore::new(pool);

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

        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| TEST_DATABASE_URL.to_string());

        let Ok(pool) = sqlx::PgPool::connect(&database_url).await else {
            println!("Skipping test - database not available");
            return;
        };
        let metadata_store = MetadataStore::new(pool);

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

        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| TEST_DATABASE_URL.to_string());

        let Ok(pool) = sqlx::PgPool::connect(&database_url).await else {
            println!("Skipping test - database not available");
            return;
        };
        let metadata_store = MetadataStore::new(pool);

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

// ============================================================================
// HTTP Sync API Endpoints (for Desktop Client)
// ============================================================================

/// Query parameters for delta requests
#[derive(Debug, Deserialize)]
pub struct DeltaQuery {
    /// The opaque cursor from the previous response
    pub cursor: String,
    /// Maximum number of items to return (default: 100, max: 1000)
    pub limit: Option<usize>,
}

/// Response for the cursor endpoint
#[derive(Debug, Serialize)]
pub struct CursorResponse {
    /// The opaque cursor token to use for delta requests
    pub cursor: String,
    /// The device ID
    pub device_id: Uuid,
    /// The last event ID at this cursor position
    pub last_event_id: Uuid,
    /// When this cursor was created/updated
    pub updated_at: DateTime<Utc>,
}

/// Response for the delta endpoint
#[derive(Debug, Serialize)]
pub struct DeltaResponse {
    /// The delta items
    pub items: Vec<rustshare_storage::repos::sync::SyncDelta>,
    /// The next cursor (if has_more is true)
    pub next_cursor: Option<String>,
    /// Whether there are more items to fetch
    pub has_more: bool,
    /// Total count (if available)
    pub total_count: Option<usize>,
}

/// Get or create a sync cursor for the current device
///
/// GET /api/v1/sync/cursor
///
/// Returns a cursor that represents the current sync checkpoint.
/// Clients should store this cursor and use it for subsequent delta requests.
///
/// # Authentication
///
/// Requires a valid user session (cookie or bearer token).
pub async fn get_sync_cursor(
    State(state): State<AppState>,
    AuthenticatedUser { user_id, .. }: AuthenticatedUser,
) -> Result<impl IntoResponse, AppError> {
    // For now, use the user_id as the device_id if not provided
    // In a full implementation, we would get the device_id from a device auth token
    let device_id = user_id;

    // Create a simple in-memory sync repository using the event store
    // In production, this would use a proper repository implementation
    let cursor_doc = match get_or_create_cursor_impl(&state, user_id, device_id).await {
        Ok(cursor) => cursor,
        Err(e) => {
            error!("Failed to get or create cursor: {}", e);
            return Err(AppError::internal("Failed to create sync cursor"));
        }
    };

    let response = CursorResponse {
        cursor: cursor_doc.cursor,
        device_id: cursor_doc.device_id,
        last_event_id: cursor_doc.last_event_id,
        updated_at: cursor_doc.updated_at,
    };

    Ok((StatusCode::OK, Json(response)))
}

/// Get delta changes since a cursor
///
/// GET /api/v1/sync/delta?cursor=xxx&limit=100
///
/// Returns all changes that have occurred since the given cursor position.
/// The response includes a new cursor for the next page if there are more items.
///
/// # Authentication
///
/// Requires a valid user session (cookie or bearer token).
pub async fn get_sync_delta(
    State(state): State<AppState>,
    AuthenticatedUser { user_id, .. }: AuthenticatedUser,
    Query(query): Query<DeltaQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = query.limit.unwrap_or(100).clamp(1, 1000);

    // Get delta from the event store
    let delta_result = match get_delta_impl(&state, user_id, &query.cursor, limit).await {
        Ok(result) => result,
        Err(e) => {
            error!("Failed to get delta: {}", e);
            return Err(AppError::internal("Failed to retrieve sync delta"));
        }
    };

    let response = DeltaResponse {
        items: delta_result.items,
        next_cursor: delta_result.next_cursor,
        has_more: delta_result.has_more,
        total_count: delta_result.total_count,
    };

    Ok((StatusCode::OK, Json(response)))
}

/// Internal implementation to get or create a cursor
///
/// For Phase 1, we use a simple implementation that:
/// 1. Creates a cursor based on the current time
/// 2. Stores it in memory (in production, this would persist to the document store)
async fn get_or_create_cursor_impl(
    _state: &AppState,
    user_id: Uuid,
    device_id: Uuid,
) -> anyhow::Result<SyncCursor> {
    use rustshare_storage::repos::sync::generate_cursor;

    Ok(SyncCursor {
        user_id,
        device_id,
        cursor: generate_cursor(),
        last_event_id: Uuid::nil(), // No events processed yet
        updated_at: Utc::now(),
    })
}

/// Internal implementation to get delta changes
///
/// For Phase 1, we use the existing EventStore to query events
/// that have occurred since the cursor timestamp.
async fn get_delta_impl(
    state: &AppState,
    user_id: Uuid,
    cursor: &str,
    limit: usize,
) -> anyhow::Result<DeltaResult> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use rustshare_core::events::Event;
    use rustshare_storage::repos::sync::parse_cursor;

    // Parse the cursor to get the timestamp
    let since_timestamp = match parse_cursor(cursor) {
        Ok(ts) => ts,
        Err(e) => {
            anyhow::bail!("Invalid cursor: {}", e);
        }
    };

    // Get events from the event store using the last_seen_event_id pattern
    // We use a nil UUID as the starting point since we don't have a proper event log cursor yet
    let events: Vec<Event> = state
        .event_store
        .get_events_since(user_id, None, limit as i64)
        .await?;

    // Convert events to sync deltas
    let mut items = Vec::new();
    let mut last_timestamp = since_timestamp;
    let mut _last_event_id = Uuid::nil();

    for event in &events {
        // Skip events that occurred before our cursor timestamp
        if event.timestamp <= since_timestamp {
            continue;
        }

        // Convert event to delta (simplified for Phase 1)
        if let Some(delta) = event_to_delta(event) {
            items.push(delta);
        }

        last_timestamp = event.timestamp;
        _last_event_id = event.id;
    }

    // Generate next cursor
    let has_more = events.len() == limit;
    let next_cursor = if has_more {
        let timestamp_millis = last_timestamp.timestamp_millis();
        let nonce = Uuid::new_v4();
        let token = format!("{}:{}", timestamp_millis, nonce);
        Some(STANDARD.encode(token))
    } else {
        None
    };

    Ok(DeltaResult {
        items,
        next_cursor,
        has_more,
        total_count: None,
    })
}

/// Convert a core Event to a SyncDelta
fn event_to_delta(event: &Event) -> Option<SyncDelta> {
    use rustshare_core::events::*;
    use rustshare_storage::repos::sync::SyncDelta;

    match event.event_type {
        EventType::FileUploaded => {
            let payload: FileUploadedPayload =
                serde_json::from_value(event.payload.clone()).ok()?;
            Some(SyncDelta::FileCreated {
                event_id: event.id,
                timestamp: event.timestamp,
                file_id: payload.file_id,
                name: payload.name,
                path: payload.path,
                parent_id: payload.parent_folder_id,
                size: payload.size,
                mime_type: payload.mime_type,
                content_hash: payload.content_hash,
                version_id: Uuid::new_v4(), // In real impl, this comes from the event
            })
        }
        EventType::FileModified => {
            let payload: FileModifiedPayload =
                serde_json::from_value(event.payload.clone()).ok()?;
            Some(SyncDelta::FileModified {
                event_id: event.id,
                timestamp: event.timestamp,
                file_id: payload.file_id,
                name: "unknown".to_string(), // Would need to look up file
                path: "unknown".to_string(),
                size: payload.new_size,
                mime_type: "application/octet-stream".to_string(),
                content_hash: payload.new_content_hash,
                version_id: Uuid::new_v4(),
                version_number: payload.new_version,
            })
        }
        EventType::FileRenamed => {
            let payload: FileRenamedPayload = serde_json::from_value(event.payload.clone()).ok()?;
            Some(SyncDelta::FileRenamed {
                event_id: event.id,
                timestamp: event.timestamp,
                file_id: payload.file_id,
                old_name: payload.old_name,
                new_name: payload.new_name,
                old_path: payload.old_path,
                new_path: payload.new_path,
            })
        }
        EventType::FileMoved => {
            let payload: FileMovedPayload = serde_json::from_value(event.payload.clone()).ok()?;
            Some(SyncDelta::FileMoved {
                event_id: event.id,
                timestamp: event.timestamp,
                file_id: payload.file_id,
                name: "unknown".to_string(),
                old_parent_id: payload.old_parent_folder_id,
                new_parent_id: payload.new_parent_folder_id,
                old_path: payload.old_path,
                new_path: payload.new_path,
            })
        }
        EventType::FileDeleted => {
            let payload: FileDeletedPayload = serde_json::from_value(event.payload.clone()).ok()?;
            Some(SyncDelta::FileDeleted {
                event_id: event.id,
                timestamp: event.timestamp,
                file_id: payload.file_id,
                name: payload.file_name,
                path: "unknown".to_string(),
                parent_id: payload.folder_id,
            })
        }
        EventType::FolderCreated => {
            let payload: FolderCreatedPayload =
                serde_json::from_value(event.payload.clone()).ok()?;
            Some(SyncDelta::FolderCreated {
                event_id: event.id,
                timestamp: event.timestamp,
                folder_id: payload.folder_id,
                name: payload.name,
                path: payload.path,
                parent_id: payload.parent_folder_id,
            })
        }
        EventType::FolderRenamed => {
            let payload: FolderRenamedPayload =
                serde_json::from_value(event.payload.clone()).ok()?;
            Some(SyncDelta::FolderRenamed {
                event_id: event.id,
                timestamp: event.timestamp,
                folder_id: payload.folder_id,
                old_name: payload.old_name,
                new_name: payload.new_name,
                old_path: payload.old_path,
                new_path: payload.new_path,
            })
        }
        EventType::FolderMoved => {
            let payload: FolderMovedPayload = serde_json::from_value(event.payload.clone()).ok()?;
            Some(SyncDelta::FolderMoved {
                event_id: event.id,
                timestamp: event.timestamp,
                folder_id: payload.folder_id,
                name: "unknown".to_string(),
                old_parent_id: payload.old_parent_folder_id,
                new_parent_id: payload.new_parent_folder_id,
                old_path: payload.old_path,
                new_path: payload.new_path,
            })
        }
        EventType::FolderDeleted => {
            // FolderDeleted payload doesn't exist yet, use generic
            Some(SyncDelta::FolderDeleted {
                event_id: event.id,
                timestamp: event.timestamp,
                folder_id: event.aggregate_id,
                name: "unknown".to_string(),
                path: "unknown".to_string(),
                parent_id: None,
            })
        }
        EventType::ShareCreated => {
            let payload: ShareCreatedPayload =
                serde_json::from_value(event.payload.clone()).ok()?;
            Some(SyncDelta::ShareCreated {
                event_id: event.id,
                timestamp: event.timestamp,
                share_id: payload.share_id,
                resource_type: "file".to_string(),
                resource_id: payload.file_id,
                resource_name: "unknown".to_string(),
                permissions: format!("{:?}", payload.permissions),
                scope: "public".to_string(),
                recipient_user_id: None,
            })
        }
        EventType::ShareRevoked => {
            let payload: ShareRevokedPayload =
                serde_json::from_value(event.payload.clone()).ok()?;
            Some(SyncDelta::ShareRevoked {
                event_id: event.id,
                timestamp: event.timestamp,
                share_id: payload.share_id,
                resource_type: "file".to_string(),
                resource_id: payload.file_id,
            })
        }
        _ => {
            // Other event types not yet mapped
            None
        }
    }
}
