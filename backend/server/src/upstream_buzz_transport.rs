//! Default-off, upstream-only relay transport foundation.
//!
//! This module has no route or startup caller. A session is returned only
//! after the tenant mapping is reloaded and its generation still matches.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use rustshare_core::domain::{TenantId, WorkspaceId};
use rustshare_resource_auth::upstream_relay_mapping::{MappingError, TrustedRelayMapping};
use rustshare_storage::upstream_relay_mapping_store::{
    UpstreamRelayMappingStore, UpstreamRelayMappingStoreError,
};
use serde_json::Value;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufStream};
use tokio::net::{lookup_host, TcpStream};
use tokio_rustls::rustls::{pki_types::ServerName, ClientConfig, RootCertStore};
use tokio_rustls::TlsConnector;
use tokio_tungstenite::{client_async, tungstenite::Message, WebSocketStream};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const SESSION_REVALIDATE_INTERVAL: Duration = Duration::from_secs(5);
const NIP11_MAX_BYTES: usize = 64 * 1024;

type RelaySocket = WebSocketStream<BufStream<tokio_rustls::client::TlsStream<TcpStream>>>;

#[derive(Debug, Error)]
pub enum UpstreamBuzzTransportError {
    #[error("trusted relay mapping unavailable")]
    MappingStore(#[from] UpstreamRelayMappingStoreError),
    #[error("trusted relay mapping rejected connection")]
    Mapping(#[from] MappingError),
    #[error("trusted relay connection failed")]
    Connection,
    #[error("trusted relay mapping changed; session closed")]
    SessionFenced,
}

/// A WebSocket dedicated to one exact tenant, workspace and mapping generation.
pub(crate) struct TenantRelaySession {
    store: UpstreamRelayMappingStore,
    tenant_id: TenantId,
    workspace_id: WorkspaceId,
    scope: rustshare_resource_auth::upstream_relay_mapping::MappingScope,
    socket: Option<RelaySocket>,
    last_revalidation: tokio::time::Instant,
}

impl TenantRelaySession {
    pub(crate) fn scope(&self) -> &rustshare_resource_auth::upstream_relay_mapping::MappingScope {
        &self.scope
    }

    /// Receive frames while fencing the session against mapping updates,
    /// deactivation and expiry. This transport foundation exposes no raw send.
    pub(crate) async fn receive(&mut self) -> Result<Option<Message>, UpstreamBuzzTransportError> {
        loop {
            tokio::select! {
                frame = async {
                    match self.socket.as_mut() {
                        Some(socket) => socket.next().await,
                        None => None,
                    }
                } => {
                    let frame = match frame {
                        Some(Ok(frame)) => Some(frame),
                        Some(Err(_)) => {
                            self.socket.take();
                            return Err(UpstreamBuzzTransportError::Connection);
                        }
                        None => {
                            self.socket.take();
                            return Ok(None);
                        }
                    };
                    self.revalidate().await?;
                    self.last_revalidation = tokio::time::Instant::now();
                    return Ok(frame);
                }
                _ = tokio::time::sleep_until(self.last_revalidation + SESSION_REVALIDATE_INTERVAL) => {
                    self.revalidate().await?;
                    self.last_revalidation = tokio::time::Instant::now();
                }
            }
        }
    }

    async fn revalidate(&mut self) -> Result<(), UpstreamBuzzTransportError> {
        let current = load_mapping(&self.store, self.tenant_id, self.workspace_id).await;
        if !matches!(current, Ok(ref mapping) if mapping.scope() == self.scope) {
            self.socket.take();
            return Err(UpstreamBuzzTransportError::SessionFenced);
        }
        Ok(())
    }
}

/// Resolve and connect only to operator-approved destinations from the current
/// tenant/workspace mapping. The store enforces tenant RLS; callers supply IDs
/// from their already-authorized subject context, never API-provided relay data.
pub(crate) async fn connect(
    store: &UpstreamRelayMappingStore,
    tenant_id: TenantId,
    workspace_id: WorkspaceId,
) -> Result<TenantRelaySession, UpstreamBuzzTransportError> {
    let mapping = load_mapping(store, tenant_id, workspace_id).await?;
    let scope = mapping.scope();
    let host = mapping.record().relay_host.as_str();
    let port = mapping_port(&mapping)?;
    let resolved = tokio::time::timeout(CONNECT_TIMEOUT, lookup_host((host, port)))
        .await
        .map_err(|_| UpstreamBuzzTransportError::Connection)?
        .map_err(|_| UpstreamBuzzTransportError::Connection)?
        .map(|addr| addr.ip())
        .collect::<Vec<_>>();
    let attempt = mapping.begin_connection(scope, &resolved, chrono::Utc::now())?;
    let peer = attempt
        .pinned_ips()
        .first()
        .copied()
        .map(|ip| SocketAddr::new(ip, attempt.port()))
        .ok_or(UpstreamBuzzTransportError::Connection)?;

    let tcp = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(peer))
        .await
        .map_err(|_| UpstreamBuzzTransportError::Connection)?
        .map_err(|_| UpstreamBuzzTransportError::Connection)?;
    let actual_peer = tcp
        .peer_addr()
        .map_err(|_| UpstreamBuzzTransportError::Connection)?;
    if actual_peer != peer {
        return Err(UpstreamBuzzTransportError::Connection);
    }

    let tls_config = tls_config();
    let server_name = ServerName::try_from(attempt.relay_host().to_owned())
        .map_err(|_| UpstreamBuzzTransportError::Connection)?;
    let tls = tokio::time::timeout(
        CONNECT_TIMEOUT,
        TlsConnector::from(tls_config).connect(server_name, tcp),
    )
    .await
    .map_err(|_| UpstreamBuzzTransportError::Connection)?
    .map_err(|_| UpstreamBuzzTransportError::Connection)?;
    let mut tls = BufStream::new(tls);
    verify_nip11(&attempt, &mut tls).await?;
    let (socket, _) = tokio::time::timeout(CONNECT_TIMEOUT, client_async(attempt.relay_url(), tls))
        .await
        .map_err(|_| UpstreamBuzzTransportError::Connection)?
        .map_err(|_| UpstreamBuzzTransportError::Connection)?;

    let current = load_mapping(store, tenant_id, workspace_id).await?;
    if current != mapping {
        return Err(MappingError::MappingChangedDuringConnect.into());
    }

    Ok(TenantRelaySession {
        store: store.clone(),
        tenant_id,
        workspace_id,
        scope: attempt.scope().clone(),
        socket: Some(socket),
        last_revalidation: tokio::time::Instant::now(),
    })
}

async fn load_mapping(
    store: &UpstreamRelayMappingStore,
    tenant_id: TenantId,
    workspace_id: WorkspaceId,
) -> Result<TrustedRelayMapping, UpstreamBuzzTransportError> {
    tokio::time::timeout(CONNECT_TIMEOUT, store.load(tenant_id, workspace_id))
        .await
        .map_err(|_| UpstreamBuzzTransportError::Connection)?
        .map_err(Into::into)
}

fn mapping_port(mapping: &TrustedRelayMapping) -> Result<u16, UpstreamBuzzTransportError> {
    url::Url::parse(mapping.relay_url())
        .ok()
        .and_then(|url| url.port_or_known_default())
        .ok_or(UpstreamBuzzTransportError::Connection)
}

fn tls_config() -> Arc<ClientConfig> {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let mut config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Arc::new(config)
}

async fn verify_nip11(
    attempt: &rustshare_resource_auth::upstream_relay_mapping::ConnectionAttempt,
    stream: &mut BufStream<tokio_rustls::client::TlsStream<TcpStream>>,
) -> Result<(), UpstreamBuzzTransportError> {
    tokio::time::timeout(CONNECT_TIMEOUT, verify_nip11_inner(attempt, stream))
        .await
        .map_err(|_| UpstreamBuzzTransportError::Connection)?
}

async fn verify_nip11_inner(
    attempt: &rustshare_resource_auth::upstream_relay_mapping::ConnectionAttempt,
    stream: &mut BufStream<tokio_rustls::client::TlsStream<TcpStream>>,
) -> Result<(), UpstreamBuzzTransportError> {
    let authority = format!("{}:{}", attempt.relay_host(), attempt.port());
    let request = format!(
        "GET / HTTP/1.1\r\nHost: {authority}\r\nAccept: application/nostr+json\r\nConnection: keep-alive\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|_| UpstreamBuzzTransportError::Connection)?;
    stream
        .flush()
        .await
        .map_err(|_| UpstreamBuzzTransportError::Connection)?;

    let mut response = Vec::with_capacity(4096);
    let (header_len, body_len) = loop {
        if let Some(header) = parse_nip11_headers(&response)? {
            break header;
        }
        if response.len() >= 16 * 1024 {
            return Err(UpstreamBuzzTransportError::Connection);
        }
        let mut bytes = [0; 2048];
        let count = stream
            .read(&mut bytes)
            .await
            .map_err(|_| UpstreamBuzzTransportError::Connection)?;
        if count == 0 {
            return Err(UpstreamBuzzTransportError::Connection);
        }
        response.extend_from_slice(&bytes[..count]);
    };
    let already_read = response.len().saturating_sub(header_len);
    if already_read > body_len {
        return Err(UpstreamBuzzTransportError::Connection);
    }
    while response.len().saturating_sub(header_len) < body_len {
        let mut bytes = [0; 2048];
        let count = stream
            .read(&mut bytes)
            .await
            .map_err(|_| UpstreamBuzzTransportError::Connection)?;
        if count == 0 || response.len() + count > header_len + NIP11_MAX_BYTES {
            return Err(UpstreamBuzzTransportError::Connection);
        }
        response.extend_from_slice(&bytes[..count]);
    }
    if response.len() != header_len + body_len {
        return Err(UpstreamBuzzTransportError::Connection);
    }
    let value: Value = serde_json::from_slice(&response[header_len..])
        .map_err(|_| UpstreamBuzzTransportError::Connection)?;
    if relay_identity(&value) != Some(attempt.relay_pubkey()) {
        return Err(UpstreamBuzzTransportError::Connection);
    }
    Ok(())
}

fn parse_nip11_headers(
    response: &[u8],
) -> Result<Option<(usize, usize)>, UpstreamBuzzTransportError> {
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut parsed = httparse::Response::new(&mut headers);
    let header_len = match parsed
        .parse(response)
        .map_err(|_| UpstreamBuzzTransportError::Connection)?
    {
        httparse::Status::Partial => return Ok(None),
        httparse::Status::Complete(header_len) => header_len,
    };
    if header_len > 16 * 1024 {
        return Err(UpstreamBuzzTransportError::Connection);
    }
    let content_length = parsed
        .headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case("content-length"));
    let mut connection_close = false;
    let mut has_transfer_encoding = false;
    let mut content_type = false;
    let mut content_length_count = 0;
    for header in parsed.headers.iter() {
        if header.name.eq_ignore_ascii_case("content-length") {
            content_length_count += 1;
        } else if header.name.eq_ignore_ascii_case("connection") {
            connection_close |= std::str::from_utf8(header.value).ok().is_some_and(|value| {
                value
                    .split(',')
                    .any(|token| token.trim().eq_ignore_ascii_case("close"))
            });
        } else if header.name.eq_ignore_ascii_case("transfer-encoding") {
            has_transfer_encoding = true;
        } else if header.name.eq_ignore_ascii_case("content-type") {
            content_type = std::str::from_utf8(header.value)
                .ok()
                .and_then(|value| value.split(';').next())
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/nostr+json"));
        }
    }
    let body_len = content_length
        .and_then(|header| std::str::from_utf8(header.value).ok())
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or(UpstreamBuzzTransportError::Connection)?;
    if parsed.version != Some(1)
        || parsed.code != Some(200)
        || content_length_count != 1
        || connection_close
        || has_transfer_encoding
        || !content_type
        || body_len > NIP11_MAX_BYTES
    {
        return Err(UpstreamBuzzTransportError::Connection);
    }
    Ok(Some((header_len, body_len)))
}

fn relay_identity(nip11: &Value) -> Option<&str> {
    nip11.get("self")?.as_str()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{parse_nip11_headers, relay_identity};

    #[test]
    fn nip11_identity_requires_exact_self_field() {
        let identity = "ab".repeat(32);
        assert_eq!(
            relay_identity(&json!({"self": identity})),
            Some(identity.as_str())
        );
        assert_eq!(relay_identity(&json!({"pubkey": identity})), None);
        assert_eq!(relay_identity(&json!({"self": 7})), None);
    }

    #[test]
    fn nip11_response_requires_bounded_persistent_http_1_1_json() {
        let valid = b"HTTP/1.1 200 OK\r\nContent-Type: application/nostr+json; charset=utf-8\r\nContent-Length: 10\r\n\r\n";
        let parsed = parse_nip11_headers(valid).unwrap().unwrap();
        assert_eq!(parsed.0, valid.len());
        assert_eq!(parsed.1, 10);

        let invalid = [
            &b"HTTP/1.1 302 Found\r\nContent-Type: application/nostr+json\r\nContent-Length: 2\r\n\r\n"[..],
            b"HTTP/1.1 200 OK\r\nContent-Type: application/nostr+json\r\nTransfer-Encoding: chunked\r\n\r\n",
            b"HTTP/1.1 200 OK\r\nContent-Type: application/nostr+json\r\nConnection: close\r\nContent-Length: 2\r\n\r\n",
            b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 2\r\n\r\n",
            b"HTTP/1.1 200 OK\r\nContent-Type: application/nostr+json\r\nContent-Length: 2\r\nContent-Length: 2\r\n\r\n",
            b"HTTP/1.1 200 OK\r\nContent-Type: application/nostr+json\r\nContent-Length: 65537\r\n\r\n",
        ];
        for response in invalid {
            assert!(parse_nip11_headers(response).is_err());
        }
    }
}
