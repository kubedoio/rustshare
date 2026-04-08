use anyhow::{Context, Result};
use futures_util::StreamExt;
use std::time::Duration;
use tokio::time::sleep;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::HeaderValue, protocol::Message},
};
use tracing::{debug, error, info, warn};

const DEFAULT_RECONNECT_INTERVAL: Duration = Duration::from_secs(5);
const MAX_RECONNECT_ATTEMPTS: usize = 10;

/// Event types for remote change notifications
#[derive(Debug, Clone)]
pub enum RemoteChangeEvent {
    /// A file was changed on the remote
    FileChanged {
        file_id: String,
        path: String,
    },
    /// A folder was changed on the remote
    FolderChanged {
        folder_id: String,
    },
    /// A sync operation completed on the remote
    SyncComplete {
        cursor: String,
    },
}

/// WebSocket client for real-time remote change notifications
#[derive(Debug)]
pub struct WebSocketClient {
    server_url: String,
    token: String,
    reconnect_interval: Duration,
    max_reconnect_attempts: usize,
}

impl WebSocketClient {
    /// Create a new WebSocket client
    pub fn new(server_url: &str, token: &str) -> Self {
        Self {
            server_url: server_url.to_string(),
            token: token.to_string(),
            reconnect_interval: DEFAULT_RECONNECT_INTERVAL,
            max_reconnect_attempts: MAX_RECONNECT_ATTEMPTS,
        }
    }

    /// Set a custom reconnect interval
    pub fn with_reconnect_interval(mut self, interval: Duration) -> Self {
        self.reconnect_interval = interval;
        self
    }

    /// Set a custom max reconnect attempts
    pub fn with_max_reconnect_attempts(mut self, max_attempts: usize) -> Self {
        self.max_reconnect_attempts = max_attempts;
        self
    }

    /// Connect to the WebSocket server and start listening for events
    pub async fn connect<F>(&self, on_event: F) -> Result<()>
    where
        F: Fn(RemoteChangeEvent) + Send + Clone + 'static,
    {
        let mut attempt = 0;

        loop {
            match self.try_connect(on_event.clone()).await {
                Ok(_) => {
                    info!("WebSocket connection closed gracefully");
                    return Ok(());
                }
                Err(e) => {
                    attempt += 1;
                    if attempt >= self.max_reconnect_attempts {
                        error!(
                            "WebSocket connection failed after {} attempts: {}",
                            attempt, e
                        );
                        return Err(e);
                    }
                    warn!(
                        "WebSocket connection failed (attempt {}/{}): {}. Reconnecting in {:?}...",
                        attempt, self.max_reconnect_attempts, e, self.reconnect_interval
                    );
                    sleep(self.reconnect_interval).await;
                }
            }
        }
    }

    /// Attempt a single connection
    async fn try_connect<F>(&self, on_event: F) -> Result<()>
    where
        F: Fn(RemoteChangeEvent) + Send + 'static,
    {
        let ws_url = format!("{}/api/v1/sync/websocket", self.server_url);
        info!("Connecting to WebSocket: {}", ws_url);

        let mut request = ws_url
            .into_client_request()
            .context("Failed to create WebSocket request")?;

        // Add authorization header
        request.headers_mut().insert(
            "Authorization",
            HeaderValue::from_str(&format!("Bearer {}", self.token))
                .context("Failed to create auth header")?,
        );

        let (ws_stream, response) = connect_async(request)
            .await
            .context("Failed to connect to WebSocket server")?;

        info!(
            "WebSocket connected with status: {}",
            response.status()
        );

        let (_, mut read) = ws_stream.split();

        // Handle incoming messages
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    debug!("Received WebSocket message: {}", text);
                    match self.parse_message(&text) {
                        Some(event) => {
                            on_event(event);
                        }
                        None => {
                            warn!("Received unknown message format: {}", text);
                        }
                    }
                }
                Ok(Message::Close(frame)) => {
                    info!("WebSocket closed: {:?}", frame);
                    break;
                }
                Ok(Message::Ping(_data)) => {
                    debug!("Received ping");
                    // Pong is handled automatically by tokio-tungstenite
                }
                Ok(Message::Pong(_)) => {
                    debug!("Received pong");
                }
                Ok(Message::Binary(data)) => {
                    debug!("Received binary message: {} bytes", data.len());
                }
                Ok(Message::Frame(_)) => {
                    // Raw frames are handled internally
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    return Err(e.into());
                }
            }
        }

        Ok(())
    }

    /// Parse a WebSocket message into a RemoteChangeEvent
    fn parse_message(&self, text: &str) -> Option<RemoteChangeEvent> {
        #[derive(serde::Deserialize)]
        struct WsMessage {
            event: String,
            #[serde(flatten)]
            payload: serde_json::Value,
        }

        let msg: WsMessage = serde_json::from_str(text).ok()?;

        match msg.event.as_str() {
            "file_changed" => {
                let file_id = msg.payload.get("file_id")?.as_str()?.to_string();
                let path = msg
                    .payload
                    .get("path")
                    .and_then(|p| p.as_str())
                    .unwrap_or("")
                    .to_string();
                Some(RemoteChangeEvent::FileChanged { file_id, path })
            }
            "folder_changed" => {
                let folder_id = msg.payload.get("folder_id")?.as_str()?.to_string();
                Some(RemoteChangeEvent::FolderChanged { folder_id })
            }
            "sync_complete" => {
                let cursor = msg.payload.get("cursor")?.as_str()?.to_string();
                Some(RemoteChangeEvent::SyncComplete { cursor })
            }
            _ => {
                warn!("Unknown event type: {}", msg.event);
                None
            }
        }
    }

    /// Disconnect from the WebSocket server (no-op in current implementation)
    /// This is provided for API consistency and future extensibility
    pub async fn disconnect(&self) {
        info!("WebSocket disconnect requested");
        // The connection will be dropped when the future returned by connect() is dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_websocket_client_new() {
        let client = WebSocketClient::new("wss://app.rustshare.io", "test-token");
        assert_eq!(client.server_url, "wss://app.rustshare.io");
        assert_eq!(client.token, "test-token");
        assert_eq!(client.reconnect_interval, DEFAULT_RECONNECT_INTERVAL);
        assert_eq!(client.max_reconnect_attempts, MAX_RECONNECT_ATTEMPTS);
    }

    #[test]
    fn test_websocket_client_builder() {
        let client = WebSocketClient::new("wss://app.rustshare.io", "test-token")
            .with_reconnect_interval(Duration::from_secs(10))
            .with_max_reconnect_attempts(5);

        assert_eq!(client.reconnect_interval, Duration::from_secs(10));
        assert_eq!(client.max_reconnect_attempts, 5);
    }

    #[test]
    fn test_parse_message_file_changed() {
        let client = WebSocketClient::new("wss://app.rustshare.io", "test-token");
        let msg = r#"{"event":"file_changed","file_id":"123","path":"/test/file.txt"}"#;

        let event = client.parse_message(msg);
        assert!(matches!(
            event,
            Some(RemoteChangeEvent::FileChanged { file_id, path })
            if file_id == "123" && path == "/test/file.txt"
        ));
    }

    #[test]
    fn test_parse_message_folder_changed() {
        let client = WebSocketClient::new("wss://app.rustshare.io", "test-token");
        let msg = r#"{"event":"folder_changed","folder_id":"456"}"#;

        let event = client.parse_message(msg);
        assert!(matches!(
            event,
            Some(RemoteChangeEvent::FolderChanged { folder_id })
            if folder_id == "456"
        ));
    }

    #[test]
    fn test_parse_message_sync_complete() {
        let client = WebSocketClient::new("wss://app.rustshare.io", "test-token");
        let msg = r#"{"event":"sync_complete","cursor":"abc123"}"#;

        let event = client.parse_message(msg);
        assert!(matches!(
            event,
            Some(RemoteChangeEvent::SyncComplete { cursor })
            if cursor == "abc123"
        ));
    }

    #[test]
    fn test_parse_message_unknown_event() {
        let client = WebSocketClient::new("wss://app.rustshare.io", "test-token");
        let msg = r#"{"event":"unknown","data":"test"}"#;

        let event = client.parse_message(msg);
        assert!(event.is_none());
    }

    #[test]
    fn test_parse_message_invalid_json() {
        let client = WebSocketClient::new("wss://app.rustshare.io", "test-token");
        let msg = "not valid json";

        let event = client.parse_message(msg);
        assert!(event.is_none());
    }
}
