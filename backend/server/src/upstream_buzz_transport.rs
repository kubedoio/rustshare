//! Default-off, upstream-only relay transport foundation.
//!
//! This module has no route or startup caller. A session is returned only
//! after the tenant mapping is reloaded and its generation still matches.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use rustshare_core::domain::{TenantId, WorkspaceId};
use rustshare_resource_auth::upstream_relay_mapping::{
    verify_connected_relay, ConnectedRelayEvidence, MappingError, TrustedRelayMapping,
};
use rustshare_storage::upstream_relay_mapping_store::{
    UpstreamRelayMappingStore, UpstreamRelayMappingStoreError,
};
use serde_json::Value;
use thiserror::Error;
use tokio::net::{lookup_host, TcpStream};
use tokio_rustls::rustls::{pki_types::ServerName, ClientConfig, RootCertStore};
use tokio_rustls::TlsConnector;
use tokio_tungstenite::{client_async, tungstenite::Message, WebSocketStream};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const SESSION_REVALIDATE_INTERVAL: Duration = Duration::from_secs(5);
const NIP11_MAX_BYTES: usize = 64 * 1024;

type RelaySocket = WebSocketStream<tokio_rustls::client::TlsStream<TcpStream>>;

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
pub struct TenantRelaySession {
    store: UpstreamRelayMappingStore,
    tenant_id: TenantId,
    workspace_id: WorkspaceId,
    scope: rustshare_resource_auth::upstream_relay_mapping::MappingScope,
    socket: RelaySocket,
    last_revalidation: tokio::time::Instant,
}

impl TenantRelaySession {
    pub fn scope(&self) -> &rustshare_resource_auth::upstream_relay_mapping::MappingScope {
        &self.scope
    }

    /// Send a protocol frame only while the stored tenant mapping still matches.
    pub async fn send(&mut self, message: Message) -> Result<(), UpstreamBuzzTransportError> {
        self.revalidate().await?;
        self.socket
            .send(message)
            .await
            .map_err(|_| UpstreamBuzzTransportError::Connection)
    }

    /// Receive frames while periodically fencing the session against mapping
    /// updates, deactivation and expiry. The timestamp persists across calls,
    /// so a busy stream cannot keep resetting the revalidation deadline.
    pub async fn receive(&mut self) -> Result<Option<Message>, UpstreamBuzzTransportError> {
        loop {
            tokio::select! {
                frame = self.socket.next() => {
                    let frame = frame
                        .transpose()
                        .map_err(|_| UpstreamBuzzTransportError::Connection)?;
                    self.revalidate().await?;
                    self.last_revalidation = tokio::time::Instant::now();
                    return Ok(frame);
                }
                _ = tokio::time::sleep_until(self.last_revalidation + SESSION_REVALIDATE_INTERVAL) => {
                    self.revalidate().await?;
                    self.last_revalidation = tokio::time::Instant::now();
                    self.socket.send(Message::Ping(Vec::new().into())).await
                        .map_err(|_| UpstreamBuzzTransportError::Connection)?;
                }
            }
        }
    }

    async fn revalidate(&mut self) -> Result<(), UpstreamBuzzTransportError> {
        let current = load_mapping(&self.store, self.tenant_id, self.workspace_id).await;
        if !matches!(current, Ok(ref mapping) if mapping.scope() == self.scope) {
            let _ = self.socket.close(None).await;
            return Err(UpstreamBuzzTransportError::SessionFenced);
        }
        Ok(())
    }
}

/// Resolve and connect only to operator-approved destinations from the current
/// tenant/workspace mapping. The store enforces tenant RLS; callers supply IDs
/// from their already-authorized subject context, never API-provided relay data.
pub async fn connect(
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

    verify_nip11(&attempt, peer).await?;
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
    let (socket, _) = tokio::time::timeout(CONNECT_TIMEOUT, client_async(attempt.relay_url(), tls))
        .await
        .map_err(|_| UpstreamBuzzTransportError::Connection)?
        .map_err(|_| UpstreamBuzzTransportError::Connection)?;

    let current = load_mapping(store, tenant_id, workspace_id).await?;
    let evidence = ConnectedRelayEvidence {
        scope: attempt.scope().clone(),
        relay_url: attempt.relay_url().to_owned(),
        tls_server_name: attempt.relay_host().to_owned(),
        tls_certificate_and_hostname_verified: true,
        peer_ip: actual_peer.ip(),
        peer_port: actual_peer.port(),
        community_id: attempt.community_id().to_owned(),
        relay_pubkey: attempt.relay_pubkey().to_owned(),
    };
    verify_connected_relay(&attempt, &current, &evidence, chrono::Utc::now())?;

    Ok(TenantRelaySession {
        store: store.clone(),
        tenant_id,
        workspace_id,
        scope: attempt.scope().clone(),
        socket,
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
    Arc::new(
        ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    )
}

async fn verify_nip11(
    attempt: &rustshare_resource_auth::upstream_relay_mapping::ConnectionAttempt,
    pinned_peer: SocketAddr,
) -> Result<(), UpstreamBuzzTransportError> {
    let http_url = attempt.relay_url().replacen("wss://", "https://", 1);
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(attempt.relay_host(), &[pinned_peer])
        .timeout(CONNECT_TIMEOUT)
        .build()
        .map_err(|_| UpstreamBuzzTransportError::Connection)?;
    let mut response = tokio::time::timeout(
        CONNECT_TIMEOUT,
        client
            .get(http_url)
            .header(reqwest::header::ACCEPT, "application/nostr+json")
            .send(),
    )
    .await
    .map_err(|_| UpstreamBuzzTransportError::Connection)?
    .map_err(|_| UpstreamBuzzTransportError::Connection)?;
    if !response.status().is_success()
        || response.remote_addr() != Some(pinned_peer)
        || response
            .content_length()
            .is_some_and(|length| length > NIP11_MAX_BYTES as u64)
    {
        return Err(UpstreamBuzzTransportError::Connection);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| UpstreamBuzzTransportError::Connection)?
    {
        if body.len().saturating_add(chunk.len()) > NIP11_MAX_BYTES {
            return Err(UpstreamBuzzTransportError::Connection);
        }
        body.extend_from_slice(&chunk);
    }
    let value: Value =
        serde_json::from_slice(&body).map_err(|_| UpstreamBuzzTransportError::Connection)?;
    if relay_identity(&value) != Some(attempt.relay_pubkey()) {
        return Err(UpstreamBuzzTransportError::Connection);
    }
    Ok(())
}

fn relay_identity(nip11: &Value) -> Option<&str> {
    nip11.get("self")?.as_str()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::relay_identity;

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
}
