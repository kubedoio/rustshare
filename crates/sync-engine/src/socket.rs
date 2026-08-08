//! Unix socket RPC module for CLI↔Daemon communication
//!
//! This module provides JSON-RPC 2.0 communication over Unix sockets,
//! allowing the CLI to communicate with the sync daemon.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

/// Maximum request line length to prevent DoS (1MB)
const MAX_LINE_LENGTH: usize = 1_048_576;

/// JSON-RPC 2.0 request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    /// JSON-RPC version (always "2.0")
    pub jsonrpc: String,
    /// Request identifier (can be string, number, or null)
    pub id: Option<serde_json::Value>,
    /// Method name to invoke
    pub method: String,
    /// Method parameters (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl RpcRequest {
    /// Create a new RPC request
    pub fn new(method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(1)),
            method: method.into(),
            params,
        }
    }

    /// Create a notification (request without id)
    pub fn notification(method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: method.into(),
            params,
        }
    }
}

/// JSON-RPC 2.0 error object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    /// Error code
    pub code: i32,
    /// Error message
    pub message: String,
    /// Additional error data (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl RpcError {
    /// Standard error codes
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;
    pub const SERVER_ERROR_START: i32 = -32000;
    pub const SERVER_ERROR_END: i32 = -32099;

    /// Create a new RPC error
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// Parse error
    pub fn parse_error(msg: impl Into<String>) -> Self {
        Self::new(Self::PARSE_ERROR, msg)
    }

    /// Invalid request error
    pub fn invalid_request(msg: impl Into<String>) -> Self {
        Self::new(Self::INVALID_REQUEST, msg)
    }

    /// Method not found error
    pub fn method_not_found(method: impl Into<String>) -> Self {
        Self::new(
            Self::METHOD_NOT_FOUND,
            format!("Method not found: {}", method.into()),
        )
    }

    /// Internal error
    pub fn internal_error(msg: impl Into<String>) -> Self {
        Self::new(Self::INTERNAL_ERROR, msg)
    }
}

/// JSON-RPC 2.0 response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    /// JSON-RPC version (always "2.0")
    pub jsonrpc: String,
    /// Request identifier (matches the request)
    pub id: Option<serde_json::Value>,
    /// Result data (present if no error)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Error object (present if error occurred)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl RpcResponse {
    /// Create a successful response
    pub fn success(id: Option<serde_json::Value>, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response
    pub fn error(id: Option<serde_json::Value>, error: RpcError) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }

    /// Check if the response is successful
    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }
}

/// Handler function type for RPC methods
type RpcHandler =
    std::sync::Arc<dyn Fn(serde_json::Value) -> Result<serde_json::Value> + Send + Sync>;

/// Unix socket server for handling RPC requests
pub struct SocketServer {
    socket_path: std::path::PathBuf,
    listener: Option<UnixListener>,
    handlers: std::collections::HashMap<String, RpcHandler>,
    shutdown_tx: Option<broadcast::Sender<()>>,
}

impl SocketServer {
    /// Create a new socket server
    pub fn new(socket_path: impl Into<std::path::PathBuf>) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        Self {
            socket_path: socket_path.into(),
            listener: None,
            handlers: std::collections::HashMap::new(),
            shutdown_tx: Some(shutdown_tx),
        }
    }

    /// Register an RPC method handler
    pub fn register_method<F>(&mut self, method: impl Into<String>, handler: F)
    where
        F: Fn(serde_json::Value) -> Result<serde_json::Value> + Send + Sync + 'static,
    {
        self.handlers
            .insert(method.into(), std::sync::Arc::new(handler));
    }

    /// Bind the server to the socket
    pub async fn bind(&mut self) -> Result<()> {
        let path = &self.socket_path;

        // Remove stale socket if it exists
        if path.exists() {
            debug!("Removing stale socket at {:?}", path);
            tokio::fs::remove_file(path)
                .await
                .context("Failed to remove stale socket")?;
        }

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .context("Failed to create socket directory")?;
        }

        // Bind to the socket
        let listener = UnixListener::bind(path).context("Failed to bind to Unix socket")?;

        // Set socket permissions to 0600 (user-only access)
        let path_clone = path.clone();
        tokio::task::spawn_blocking(move || {
            let metadata = std::fs::metadata(&path_clone)?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600);
            std::fs::set_permissions(&path_clone, permissions)
                .context("Failed to set socket permissions")
        })
        .await
        .context("Failed to spawn blocking task for permissions")??;

        info!("Socket server bound to {:?}", path);
        self.listener = Some(listener);

        Ok(())
    }

    /// Run the server, accepting and handling connections
    pub async fn run(&self) -> Result<()> {
        let listener = self
            .listener
            .as_ref()
            .ok_or_else(|| anyhow!("Server not bound"))?;

        let mut shutdown_rx = self
            .shutdown_tx
            .as_ref()
            .ok_or_else(|| anyhow!("Shutdown channel not initialized"))?
            .subscribe();

        info!("Socket server started, accepting connections");

        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, _addr)) => {
                            debug!("New connection accepted");
                            let handlers = self.handlers.clone();
                            tokio::spawn(async move {
                                if let Err(e) = Self::handle_connection(stream, handlers).await {
                                    error!("Connection handler error: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            error!("Failed to accept connection: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Shutdown signal received, stopping server");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Handle a single connection
    async fn handle_connection(
        stream: UnixStream,
        handlers: std::collections::HashMap<String, RpcHandler>,
    ) -> Result<()> {
        let (reader, mut writer) = stream.into_split();
        let mut buf_reader = BufReader::new(reader);
        let mut line = String::new();

        loop {
            line.clear();
            match buf_reader.read_line(&mut line).await {
                Ok(0) => {
                    // Connection closed
                    debug!("Connection closed by peer");
                    break;
                }
                Ok(_n) => {
                    // Check for request size limit to prevent DoS
                    if line.len() > MAX_LINE_LENGTH {
                        error!(
                            "Request too large: {} bytes (max: {})",
                            line.len(),
                            MAX_LINE_LENGTH
                        );
                        let error_response = RpcResponse::error(
                            None,
                            RpcError::invalid_request(format!(
                                "Request too large: {} bytes (max: {})",
                                line.len(),
                                MAX_LINE_LENGTH
                            )),
                        );
                        let response_json = serde_json::to_string(&error_response)?;
                        let response_line = format!("{}\n", response_json);
                        if let Err(e) = writer.write_all(response_line.as_bytes()).await {
                            error!("Failed to write error response: {}", e);
                        }
                        if let Err(e) = writer.flush().await {
                            error!("Failed to flush error response: {}", e);
                        }
                        break;
                    }

                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    debug!("Received request: {}", trimmed);

                    // JSON-RPC 2.0: a notification (no "id") must not receive a
                    // response. Determine this before processing so even error
                    // responses are suppressed for notifications. Malformed
                    // JSON that is not a notification still gets a parse error.
                    let is_notification = serde_json::from_str::<serde_json::Value>(trimmed)
                        .ok()
                        .map(|value| value.get("id").is_none())
                        .unwrap_or(false);

                    let response = Self::process_request(trimmed, &handlers);
                    if is_notification {
                        continue;
                    }
                    let response_json = serde_json::to_string(&response)?;
                    let response_line = format!("{}\n", response_json);

                    if let Err(e) = writer.write_all(response_line.as_bytes()).await {
                        error!("Failed to write response: {}", e);
                        break;
                    }

                    if let Err(e) = writer.flush().await {
                        error!("Failed to flush response: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to read from socket: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Process a single request and return the response
    fn process_request(
        request_json: &str,
        handlers: &std::collections::HashMap<String, RpcHandler>,
    ) -> RpcResponse {
        // Parse the request
        let request: RpcRequest = match serde_json::from_str(request_json) {
            Ok(req) => req,
            Err(e) => {
                return RpcResponse::error(
                    None,
                    RpcError::parse_error(format!("Failed to parse request: {}", e)),
                );
            }
        };

        // Validate JSON-RPC version
        if request.jsonrpc != "2.0" {
            return RpcResponse::error(
                request.id,
                RpcError::invalid_request("Invalid JSON-RPC version"),
            );
        }

        // Look up the handler
        let handler = match handlers.get(&request.method) {
            Some(h) => h,
            None => {
                return RpcResponse::error(request.id, RpcError::method_not_found(&request.method));
            }
        };

        // Execute the handler
        let params = request.params.unwrap_or(serde_json::Value::Null);
        match handler(params) {
            Ok(result) => RpcResponse::success(request.id, result),
            Err(e) => RpcResponse::error(request.id, RpcError::internal_error(e.to_string())),
        }
    }

    /// Get the socket path
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

impl Drop for SocketServer {
    fn drop(&mut self) {
        // Clean up socket file on drop to prevent leaks
        if self.socket_path.exists() {
            // Use synchronous remove since Drop can't be async
            if let Err(e) = std::fs::remove_file(&self.socket_path) {
                warn!("Failed to remove socket file on drop: {}", e);
            } else {
                debug!("Socket file removed on drop: {:?}", self.socket_path);
            }
        }
    }
}

/// Unix socket client for sending RPC requests
pub struct SocketClient {
    socket_path: std::path::PathBuf,
    reader: Option<tokio::io::BufReader<tokio::net::unix::OwnedReadHalf>>,
    writer: Option<tokio::net::unix::OwnedWriteHalf>,
}

impl SocketClient {
    /// Create a new socket client
    pub fn new(socket_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
            reader: None,
            writer: None,
        }
    }

    /// Connect to the socket server
    pub async fn connect(&mut self) -> Result<()> {
        let stream = UnixStream::connect(&self.socket_path)
            .await
            .with_context(|| format!("Failed to connect to socket at {:?}", self.socket_path))?;

        let (read_half, write_half) = stream.into_split();

        debug!("Connected to socket at {:?}", self.socket_path);
        self.reader = Some(BufReader::new(read_half));
        self.writer = Some(write_half);
        Ok(())
    }

    /// Send an RPC request and wait for the response
    pub async fn call(&mut self, request: &RpcRequest) -> Result<RpcResponse> {
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| anyhow!("Not connected"))?;
        let reader = self
            .reader
            .as_mut()
            .ok_or_else(|| anyhow!("Not connected"))?;

        // Serialize and send the request
        let request_json = serde_json::to_string(request)?;
        let request_line = format!("{}\n", request_json);

        writer
            .write_all(request_line.as_bytes())
            .await
            .context("Failed to write request")?;
        writer.flush().await.context("Failed to flush request")?;

        // Read the response
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .await
            .context("Failed to read response")?;

        // Parse the response
        let response: RpcResponse =
            serde_json::from_str(line.trim()).context("Failed to parse response")?;

        Ok(response)
    }

    /// Send a notification (no response expected)
    pub async fn notify(
        &mut self,
        method: impl Into<String>,
        params: Option<serde_json::Value>,
    ) -> Result<()> {
        let request = RpcRequest::notification(method, params);

        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| anyhow!("Not connected"))?;

        let request_json = serde_json::to_string(&request)?;
        let request_line = format!("{}\n", request_json);

        writer
            .write_all(request_line.as_bytes())
            .await
            .context("Failed to write notification")?;
        writer
            .flush()
            .await
            .context("Failed to flush notification")?;

        Ok(())
    }

    /// Perform a health check ping
    pub async fn ping(&mut self) -> Result<bool> {
        let request = RpcRequest::new("daemon.ping", None);

        match self.call(&request).await {
            Ok(response) => Ok(response.is_success()),
            Err(e) => {
                warn!("Ping failed: {}", e);
                Ok(false)
            }
        }
    }

    /// Disconnect from the server
    pub async fn disconnect(mut self) -> Result<()> {
        self.reader = None;
        self.writer = None;
        debug!("Disconnected from socket");
        Ok(())
    }

    /// Get the socket path
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_rpc_request_creation() {
        let req = RpcRequest::new("test_method", Some(serde_json::json!({"key": "value"})));
        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.method, "test_method");
        assert!(req.id.is_some());

        let notification = RpcRequest::notification("notify", None);
        assert!(notification.id.is_none());
    }

    #[test]
    fn test_rpc_error_creation() {
        let err = RpcError::new(-32600, "Invalid request");
        assert_eq!(err.code, -32600);
        assert_eq!(err.message, "Invalid request");

        let err = RpcError::method_not_found("missing_method");
        assert_eq!(err.code, RpcError::METHOD_NOT_FOUND);
    }

    #[test]
    fn test_rpc_response_creation() {
        let success = RpcResponse::success(Some(serde_json::json!(1)), serde_json::json!("result"));
        assert!(success.is_success());

        let error =
            RpcResponse::error(Some(serde_json::json!(1)), RpcError::internal_error("oops"));
        assert!(!error.is_success());
    }

    #[test]
    fn test_rpc_request_serialization() {
        let req = RpcRequest::new("test", Some(serde_json::json!({"foo": "bar"})));
        let json = serde_json::to_string(&req).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["method"], "test");
    }
}
