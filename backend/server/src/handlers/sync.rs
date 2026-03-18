use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::StatusCode,
    response::Response,
};
use axum_extra::{
    headers::{Authorization, authorization::Bearer},
    TypedHeader,
};
use futures_util::{StreamExt, SinkExt};
use rustshare_core::events::Event;
use rustshare_core::domain::UserId;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::AppState;

/// Client message for requesting catch-up
#[derive(Debug, Deserialize)]
struct SyncRequest {
    #[serde(rename = "type")]
    msg_type: String,
    last_seen_event_id: Option<String>,
}

/// Notification message sent to client
#[derive(Debug, Serialize)]
struct NotificationMessage {
    event_id: String,
    event_type: String,
    aggregate_id: String,
    aggregate_type: String,
    timestamp: String,
    version: i32,
}

/// Lagged warning message
#[derive(Debug, Serialize)]
struct LaggedMessage {
    #[serde(rename = "type")]
    msg_type: String,
    message: String,
}

/// WebSocket handler for real-time sync
pub async fn sync_handler(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
) -> Result<Response, (StatusCode, String)> {
    // Validate JWT
    let claims = state
        .jwt_manager
        .validate(auth.token())
        .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid token".to_string()))?;

    // Extract user_id from JWT subject claim
    let user_id = UserId::from(
        Uuid::parse_str(&claims.sub)
            .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid user ID".to_string()))?,
    );

    info!("WebSocket connection established for user {}", user_id);

    // Upgrade connection
    Ok(ws.on_upgrade(move |socket| handle_socket(socket, user_id, state)))
}

/// Handle WebSocket connection
async fn handle_socket(socket: WebSocket, user_id: UserId, state: AppState) {
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

    // Send events to client
    let mut send_task = tokio::spawn(async move {
        // Wait briefly for catch-up request
        tokio::select! {
            last_seen_id = &mut recv_task => {
                if let Ok(Some(last_id_str)) = last_seen_id {
                    // Handle catch-up
                    if let Ok(last_id) = Uuid::parse_str(&last_id_str) {
                        match state.event_store.get_events_since(user_id, Some(last_id), 100).await {
                            Ok(events) => {
                                info!("Sending {} catch-up events to user {}", events.len(), user_id);
                                for event in events {
                                    if let Ok(notification) = event_to_notification(&event) {
                                        if let Ok(json) = serde_json::to_string(&notification) {
                                            if sender.send(Message::Text(json)).await.is_err() {
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

        // Stream live events
        loop {
            match event_rx.recv().await {
                Ok(event) => {
                    // Filter by user_id
                    if event.user_id != user_id {
                        continue;
                    }

                    // Serialize and send
                    match event_to_notification(&event) {
                        Ok(notification) => {
                            if let Ok(json) = serde_json::to_string(&notification) {
                                if sender.send(Message::Text(json)).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to serialize event: {}", e);
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Client lagged by {} events", n);
                    let lagged = LaggedMessage {
                        msg_type: "lagged".to_string(),
                        message: format!("Too many events, please sync"),
                    };
                    if let Ok(json) = serde_json::to_string(&lagged) {
                        if sender.send(Message::Text(json)).await.is_err() {
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

    info!("WebSocket connection closed for user {}", user_id);
}

/// Convert Event to NotificationMessage
fn event_to_notification(event: &Event) -> Result<NotificationMessage, String> {
    Ok(NotificationMessage {
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
