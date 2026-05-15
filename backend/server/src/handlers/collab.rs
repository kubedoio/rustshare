//! Note autosave WebSocket handler.
//!
//! Each note document has a "room" identified by its note ID.
//! Clients send JSON `note_save` messages and receive direct JSON acknowledgements.

use axum::{
    extract::{
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::{HeaderMap, StatusCode},
    response::Response,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

use super::ws_auth::ClientIdentity;

use super::ws_auth::{resolve_ws_client_identity, WsAuthQuery};
use crate::AppState;

// ---------------------------------------------------------------------------
// Room management
// ---------------------------------------------------------------------------

/// State shared across all collaboration rooms.
pub struct CollabRooms {
    inner: Arc<RwLock<HashMap<String, Arc<CollabRoom>>>>,
}

impl CollabRooms {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get an existing room or create one for connection accounting.
    pub async fn get_or_create(
        &self,
        doc_id: &str,
        _initial_content: Option<String>,
    ) -> Arc<CollabRoom> {
        // Fast path: room already exists
        {
            let rooms = self.inner.read().await;
            if let Some(room) = rooms.get(doc_id) {
                room.client_count.fetch_add(1, Ordering::SeqCst);
                return room.clone();
            }
        }

        // Slow path: create new room
        let mut rooms = self.inner.write().await;
        // Double-check after acquiring write lock
        if let Some(room) = rooms.get(doc_id) {
            room.client_count.fetch_add(1, Ordering::SeqCst);
            return room.clone();
        }

        let room = Arc::new(CollabRoom {
            doc_id: doc_id.to_string(),
            client_count: AtomicUsize::new(1),
            last_user_id: RwLock::new(None),
        });

        rooms.insert(doc_id.to_string(), room.clone());
        info!("Created collab room for doc_id={}", doc_id);
        room
    }

    /// Decrement client count and remove room if empty.
    pub async fn leave(&self, room: &CollabRoom) {
        let count = room.client_count.fetch_sub(1, Ordering::SeqCst);
        if count == 1 {
            // Last client left — remove room after brief grace period
            let doc_id = room.doc_id.clone();
            let rooms_arc = Arc::clone(&self.inner);
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                let mut rooms = rooms_arc.write().await;
                if let Some(r) = rooms.get(&doc_id) {
                    if r.client_count.load(Ordering::SeqCst) == 0 {
                        rooms.remove(&doc_id);
                        info!("Removed empty collab room for doc_id={}", doc_id);
                    }
                }
            });
        }
    }
}

/// A single collaborative document room.
pub struct CollabRoom {
    doc_id: String,
    client_count: AtomicUsize,
    last_user_id: RwLock<Option<rustshare_core::domain::UserId>>,
}

impl CollabRoom {
    /// Update the last known user ID for persistence attribution.
    pub async fn set_last_user(&self, user_id: rustshare_core::domain::UserId) {
        let mut last = self.last_user_id.write().await;
        *last = Some(user_id);
    }
}

// ---------------------------------------------------------------------------
// WebSocket handler
// ---------------------------------------------------------------------------

/// Query parameters for collab WebSocket connection.
#[derive(Debug, serde::Deserialize)]
pub struct CollabQuery {
    /// Note / document ID to collaborate on.
    pub doc_id: String,
    /// Optional JWT token for authentication.
    pub token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum CollabTextRequest {
    #[serde(rename = "note_save")]
    NoteSave { content: String },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum CollabTextResponse<'a> {
    #[serde(rename = "note_saved")]
    NoteSaved { doc_id: &'a str, content: &'a str },
    #[serde(rename = "presence")]
    Presence { user_count: usize },
    #[serde(rename = "error")]
    Error { message: String },
}

/// WebSocket handler for note autosave.
pub async fn collab_handler(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    Query(query): Query<CollabQuery>,
) -> Result<Response, (StatusCode, String)> {
    // Authenticate
    let auth_query = WsAuthQuery { token: query.token };
    let client_identity = resolve_ws_client_identity(&state, &headers, &auth_query).await?;

    info!(
        "Collab WebSocket connection for doc_id={} by user={:?}",
        query.doc_id, client_identity
    );

    let ClientIdentity::User(user_id) = &client_identity else {
        return Err((
            StatusCode::UNAUTHORIZED,
            "note autosave requires an authenticated user".to_string(),
        ));
    };

    let file_id = Uuid::parse_str(&query.doc_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid document id".to_string()))?;

    state
        .note_service
        .get_note(file_id, *user_id)
        .await
        .map_err(|e| {
            warn!(
                "Failed to authorize collab doc_id={} for user={:?}: {:?}",
                query.doc_id, user_id, e
            );
            (StatusCode::FORBIDDEN, "note is not available".to_string())
        })?;

    let room = state.collab_rooms.get_or_create(&query.doc_id, None).await;

    // Track user for persistence attribution
    if let ClientIdentity::User(user_id) = &client_identity {
        room.set_last_user(*user_id).await;
    }

    Ok(ws.on_upgrade(move |socket| {
        handle_collab_socket(socket, room, state, query.doc_id, client_identity)
    }))
}

async fn handle_collab_socket(
    mut socket: WebSocket,
    room: Arc<CollabRoom>,
    state: AppState,
    doc_id: String,
    client_identity: ClientIdentity,
) {
    if let ClientIdentity::User(user_id) = &client_identity {
        room.set_last_user(*user_id).await;
    }

    let presence = CollabTextResponse::Presence {
        user_count: room.client_count.load(Ordering::SeqCst),
    };
    if let Ok(serialized) = serde_json::to_string(&presence) {
        if socket
            .send(WsMessage::Text(serialized.into()))
            .await
            .is_err()
        {
            state.collab_rooms.leave(&room).await;
            return;
        }
    }

    while let Some(Ok(msg)) = socket.next().await {
        match msg {
            WsMessage::Text(text) => {
                debug!("[collab] doc_id={} recv text: {} bytes", doc_id, text.len());
                match handle_collab_text_message(&text, state.clone(), &doc_id, &client_identity)
                    .await
                {
                    Ok(Some(response)) => {
                        if socket.send(WsMessage::Text(response.into())).await.is_err() {
                            break;
                        }
                    }
                    Ok(None) => {}
                    Err(message) => {
                        warn!(
                            "[collab] doc_id={} text message failed: {}",
                            doc_id, message
                        );
                        let response = CollabTextResponse::Error { message };
                        if let Ok(serialized) = serde_json::to_string(&response) {
                            if socket
                                .send(WsMessage::Text(serialized.into()))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                }
            }
            WsMessage::Binary(data) => {
                debug!(
                    "[collab] doc_id={} ignoring binary collab payload: {} bytes",
                    doc_id,
                    data.len()
                );
            }
            WsMessage::Ping(data) => {
                if socket.send(WsMessage::Pong(data)).await.is_err() {
                    break;
                }
            }
            WsMessage::Pong(data) => {
                debug!("[collab] doc_id={} recv pong: {} bytes", doc_id, data.len());
            }
            WsMessage::Close(frame) => {
                debug!("[collab] doc_id={} recv close: {:?}", doc_id, frame);
                break;
            }
        }
    }

    state.collab_rooms.leave(&room).await;
    info!("Collab WebSocket disconnected for doc_id={}", doc_id);
}

async fn handle_collab_text_message(
    text: &str,
    state: AppState,
    doc_id: &str,
    client_identity: &ClientIdentity,
) -> Result<Option<String>, String> {
    let request: CollabTextRequest =
        serde_json::from_str(text).map_err(|e| format!("invalid collab text message: {}", e))?;

    match request {
        CollabTextRequest::NoteSave { content } => {
            let ClientIdentity::User(user_id) = client_identity else {
                return Err("note autosave requires an authenticated user".to_string());
            };
            let file_id = Uuid::parse_str(doc_id)
                .map_err(|_| "note autosave requires a valid document id".to_string())?;

            state
                .note_service
                .save_note(file_id, *user_id, content.clone(), None, None)
                .await
                .map_err(|e| format!("failed to save note: {}", e))?;

            let response = CollabTextResponse::NoteSaved {
                doc_id,
                content: &content,
            };
            serde_json::to_string(&response)
                .map(Some)
                .map_err(|e| format!("failed to serialize save response: {}", e))
        }
    }
}
