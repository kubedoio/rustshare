use std::net::SocketAddr;
use std::time::Duration;

use rustshare_core::services::{resolve_chat_webhook_socket_addrs, ChatEvent, WebhookDispatcher};
use tracing::warn;

/// HTTP adapter for signed chat webhooks. DNS results are pinned to prevent
/// rebinding after the SSRF check, and redirects are disabled to keep the
/// validated destination authoritative.
#[derive(Debug, Clone)]
pub struct HttpWebhookDispatcher {
    allow_http: bool,
    timeout: Duration,
}

impl HttpWebhookDispatcher {
    /// Use the server's webhook transport policy. The default follows the
    /// historical debug-only HTTP allowance and never reads environment state.
    pub fn new() -> Self {
        Self::with_http_allowed(cfg!(debug_assertions))
    }

    pub fn with_http_allowed(allow_http: bool) -> Self {
        Self {
            allow_http,
            timeout: Duration::from_secs(30),
        }
    }
}

impl Default for HttpWebhookDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

fn webhook_client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder().redirect(reqwest::redirect::Policy::none())
}

fn build_pinned_webhook_client(
    host: &str,
    addrs: &[SocketAddr],
) -> Result<reqwest::Client, String> {
    webhook_client_builder()
        .resolve_to_addrs(host, addrs)
        .build()
        .map_err(|error| format!("Failed to build webhook client: {error}"))
}

impl WebhookDispatcher for HttpWebhookDispatcher {
    async fn dispatch(&self, url: &str, event: &ChatEvent) -> Result<(), String> {
        let (host, addrs) = resolve_chat_webhook_socket_addrs(url, self.allow_http)
            .await
            .map_err(|error| {
                warn!(url = %url, error = %error, "Webhook URL failed SSRF validation at dispatch time");
                "Invalid webhook URL".to_string()
            })?;
        let client = build_pinned_webhook_client(&host, &addrs)?;

        let response = client
            .post(url)
            .header("Content-Type", "application/json")
            .header("X-RustShare-Event", format!("{:?}", event.event_type))
            .header("X-RustShare-Signature", &event.signature)
            .timeout(self.timeout)
            .json(event)
            .send()
            .await
            .map_err(|error| format!("HTTP request failed: {error}"))?;

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read body>".to_string());
            Err(format!("HTTP {status}: {body}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn pinned_client_uses_validated_addresses() {
        build_pinned_webhook_client("example.com", &[SocketAddr::from(([1, 1, 1, 1], 443))])
            .expect("pinned webhook client should build");
    }

    #[tokio::test]
    async fn client_does_not_follow_redirects() {
        let listener = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
            Ok(listener) => listener,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
            Err(error) => panic!("failed to bind redirect test listener: {error}"),
        };
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 1024];
            let _ = socket.read(&mut buffer).await.unwrap();
            socket
                .write_all(
                    b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1/internal\r\nContent-Length: 0\r\n\r\n",
                )
                .await
                .unwrap();
        });

        let response = webhook_client_builder()
            .build()
            .unwrap()
            .post(format!("http://{addr}/webhook"))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), reqwest::StatusCode::FOUND);
        server.await.unwrap();
    }
}
