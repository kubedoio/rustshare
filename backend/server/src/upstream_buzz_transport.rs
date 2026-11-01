//! Default-off, upstream-only relay transport foundation.
//!
//! This module has no route or startup caller. A session is returned only
//! after the tenant mapping is reloaded and its generation still matches.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use rustshare_core::domain::{TenantId, WorkspaceId};
use rustshare_resource_auth::upstream_relay_mapping::{
    MappingError, RelayEndpointPolicy, TrustedRelayMapping,
};
use rustshare_storage::upstream_relay_mapping_store::{
    UpstreamRelayMappingStore, UpstreamRelayMappingStoreError,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufStream};
use tokio::net::{lookup_host, TcpStream};
use tokio_rustls::rustls::{
    pki_types::{CertificateDer, ServerName},
    ClientConfig, RootCertStore,
};
use tokio_rustls::TlsConnector;
use tokio_tungstenite::{client_async, tungstenite::Message, WebSocketStream};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(2);
const SESSION_REVALIDATE_INTERVAL: Duration = Duration::from_secs(5);
const NIP11_MAX_BYTES: usize = 64 * 1024;
const MAX_TRUSTED_SERVICE_ROOTS: usize = 8;

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

/// Operator-mounted TLS roots for one exact signed private-service identity.
/// It is never built from request data or selected from the relay URL.
pub(crate) enum RelayTlsTrustConfig {
    StrictPublic,
    TrustedService {
        endpoint_service_identity: String,
        roots: Vec<CertificateDer<'static>>,
    },
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
    trust: &RelayTlsTrustConfig,
) -> Result<TenantRelaySession, UpstreamBuzzTransportError> {
    tokio::time::timeout(CONNECT_TIMEOUT, async {
        let mapping = load_mapping(store, tenant_id, workspace_id).await?;
        #[cfg(test)]
        eprintln!("upstream transport stage: mapping loaded");
        validate_trust_profile(&mapping, trust)?;
        #[cfg(test)]
        eprintln!("upstream transport stage: trust profile accepted");
        let scope = mapping.scope();
        let attempt = resolve_connection_attempt(&mapping, &scope).await?;
        #[cfg(test)]
        eprintln!("upstream transport stage: DNS answers pinned");
        let tcp = connect_pinned(attempt.pinned_ips(), attempt.port()).await?;
        #[cfg(test)]
        eprintln!("upstream transport stage: approved TCP peer connected");

        let tls_config = tls_config(trust)?;
        let server_name = ServerName::try_from(attempt.relay_host().to_owned())
            .map_err(|_| UpstreamBuzzTransportError::Connection)?;
        let tls = tokio::time::timeout(
            CONNECT_TIMEOUT,
            TlsConnector::from(tls_config).connect(server_name, tcp),
        )
        .await
        .map_err(|_| UpstreamBuzzTransportError::Connection)?
        .map_err(|_| UpstreamBuzzTransportError::Connection)?;
        #[cfg(test)]
        eprintln!("upstream transport stage: TLS certificate and hostname verified");
        verify_tls_spki_pin(&tls, &attempt.scope().tls_peer_spki_sha256)?;
        #[cfg(test)]
        eprintln!("upstream transport stage: TLS SPKI pin verified");
        let mut tls = BufStream::new(tls);
        verify_nip11(&attempt, &mut tls).await?;
        #[cfg(test)]
        eprintln!("upstream transport stage: NIP-11 relay identity verified");
        let (socket, _) =
            tokio::time::timeout(CONNECT_TIMEOUT, client_async(attempt.relay_url(), tls))
                .await
                .map_err(|_| UpstreamBuzzTransportError::Connection)?
                .map_err(|_| UpstreamBuzzTransportError::Connection)?;
        #[cfg(test)]
        eprintln!("upstream transport stage: WebSocket upgraded");

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
    })
    .await
    .map_err(|_| UpstreamBuzzTransportError::Connection)?
}

async fn resolve_connection_attempt(
    mapping: &TrustedRelayMapping,
    expected_scope: &rustshare_resource_auth::upstream_relay_mapping::MappingScope,
) -> Result<
    rustshare_resource_auth::upstream_relay_mapping::ConnectionAttempt,
    UpstreamBuzzTransportError,
> {
    let host = mapping.record().relay_host.as_str();
    let port = mapping_port(mapping)?;
    let resolved = tokio::time::timeout(CONNECT_TIMEOUT, lookup_host((host, port)))
        .await
        .map_err(|_| UpstreamBuzzTransportError::Connection)?
        .map_err(|_| UpstreamBuzzTransportError::Connection)?
        .map(|addr| addr.ip())
        .collect::<Vec<_>>();
    Ok(mapping.begin_connection(expected_scope.clone(), &resolved, chrono::Utc::now())?)
}

async fn connect_pinned(
    pinned_ips: &[IpAddr],
    port: u16,
) -> Result<TcpStream, UpstreamBuzzTransportError> {
    let deadline = tokio::time::Instant::now() + CONNECT_TIMEOUT;
    for ip in pinned_ips {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        let peer = SocketAddr::new(*ip, port);
        let timeout = remaining.min(CONNECT_ATTEMPT_TIMEOUT);
        let Ok(Ok(tcp)) = tokio::time::timeout(timeout, TcpStream::connect(peer)).await else {
            continue;
        };
        if tcp
            .peer_addr()
            .map_err(|_| UpstreamBuzzTransportError::Connection)?
            != peer
        {
            return Err(UpstreamBuzzTransportError::Connection);
        }
        return Ok(tcp);
    }
    Err(UpstreamBuzzTransportError::Connection)
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

fn validate_trust_profile(
    mapping: &TrustedRelayMapping,
    trust: &RelayTlsTrustConfig,
) -> Result<(), UpstreamBuzzTransportError> {
    use rustshare_resource_auth::upstream_relay_mapping::RelayEndpointPolicy;

    match (mapping.record().endpoint_policy, trust) {
        (RelayEndpointPolicy::StrictPublic, RelayTlsTrustConfig::StrictPublic) => Ok(()),
        (
            RelayEndpointPolicy::TrustedService,
            RelayTlsTrustConfig::TrustedService {
                endpoint_service_identity,
                roots,
            },
        ) if endpoint_service_identity == &mapping.record().endpoint_service_identity
            && !roots.is_empty()
            && roots.len() <= MAX_TRUSTED_SERVICE_ROOTS =>
        {
            Ok(())
        }
        _ => Err(UpstreamBuzzTransportError::Mapping(
            MappingError::InvalidDeploymentProvenance,
        )),
    }
}

fn tls_config(
    trust: &RelayTlsTrustConfig,
) -> Result<Arc<ClientConfig>, UpstreamBuzzTransportError> {
    let mut roots = RootCertStore::empty();
    match trust {
        RelayTlsTrustConfig::StrictPublic => {
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        }
        RelayTlsTrustConfig::TrustedService {
            roots: certificates,
            ..
        } => {
            for certificate in certificates {
                roots
                    .add(certificate.clone())
                    .map_err(|_| UpstreamBuzzTransportError::Connection)?;
            }
        }
    }
    // SQLx enables ring while the AWS SDK enables aws-lc-rs in this process.
    let mut config = ClientConfig::builder_with_provider(Arc::new(
        tokio_rustls::rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|_| UpstreamBuzzTransportError::Connection)?
    .with_root_certificates(roots)
    .with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}

fn verify_tls_spki_pin(
    tls: &tokio_rustls::client::TlsStream<TcpStream>,
    expected: &str,
) -> Result<(), UpstreamBuzzTransportError> {
    let certificate = tls
        .get_ref()
        .1
        .peer_certificates()
        .and_then(|certificates| certificates.first())
        .ok_or(UpstreamBuzzTransportError::Connection)?;
    let (_, certificate) = x509_parser::parse_x509_certificate(certificate.as_ref())
        .map_err(|_| UpstreamBuzzTransportError::Connection)?;
    let digest = Sha256::digest(certificate.tbs_certificate.subject_pki.raw);
    if expected != format!("sha256:{}", hex::encode(digest)) {
        return Err(UpstreamBuzzTransportError::Connection);
    }
    Ok(())
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
        if let Some(header) = parse_nip11_headers(
            &response,
            attempt.scope().endpoint_policy == RelayEndpointPolicy::TrustedService,
        )? {
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
    allow_json_media_type: bool,
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
            let media_type = std::str::from_utf8(header.value)
                .ok()
                .and_then(|value| value.split(';').next())
                .map(str::trim);
            content_type = media_type.is_some_and(|value| {
                value.eq_ignore_ascii_case("application/nostr+json")
                    || (allow_json_media_type && value.eq_ignore_ascii_case("application/json"))
            });
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
    use chrono::{Duration as ChronoDuration, Utc};
    use serde_json::json;
    use std::time::Duration;
    use uuid::Uuid;

    use rustshare_core::domain::{TenantId, WorkspaceId};
    use rustshare_resource_auth::upstream_relay_mapping::{
        RelayEndpointPolicy, RelayMappingRecord, TrustedRelayMapping,
    };
    use tokio_rustls::rustls::pki_types::CertificateDer;

    use super::{
        connect_pinned, parse_nip11_headers, relay_identity, resolve_connection_attempt,
        tls_config, validate_trust_profile, MappingError, RelayTlsTrustConfig,
    };

    fn mapping(host: &str, port: u16) -> TrustedRelayMapping {
        let tenant_id = TenantId(Uuid::new_v4());
        TrustedRelayMapping::validate(
            RelayMappingRecord {
                mapping_id: Uuid::new_v4(),
                tenant_id,
                workspace_id: WorkspaceId(Uuid::new_v4()),
                community_id: Uuid::new_v4().to_string(),
                endpoint_policy: rustshare_resource_auth::upstream_relay_mapping::RelayEndpointPolicy::StrictPublic,
                relay_deployment_id: Uuid::new_v4(),
                endpoint_service_identity: format!("k8s://{}/{}/buzz/{}", Uuid::new_v4(), "test-ns", Uuid::new_v4()),
                tls_peer_spki_sha256: format!("sha256:{}", "b2".repeat(32)),
                relay_url: if port == 443 {
                    format!("wss://{host}")
                } else {
                    format!("wss://{host}:{port}")
                },
                relay_host: host.into(),
                relay_pubkey: "22".repeat(32),
                deployment_digest: format!("sha256:{}", "ab".repeat(32)),
                approved_ips: vec!["8.8.8.8".parse().unwrap()],
                generation: 1,
                active: true,
                valid_until: Utc::now() + ChronoDuration::minutes(5),
                updated_by: Uuid::new_v4(),
                audit_entry_id: Uuid::new_v4(),
            },
            Utc::now(),
        )
        .unwrap()
    }

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
    fn tls_profile_is_bound_to_signed_endpoint_policy_and_service_identity() {
        let strict = mapping("relay.example.test", 443);
        assert!(validate_trust_profile(&strict, &RelayTlsTrustConfig::StrictPublic).is_ok());
        assert!(validate_trust_profile(
            &strict,
            &RelayTlsTrustConfig::TrustedService {
                endpoint_service_identity: strict.record().endpoint_service_identity.clone(),
                roots: vec![CertificateDer::from(vec![1, 2, 3])],
            }
        )
        .is_err());

        let mut record = strict.record().clone();
        record.endpoint_policy = RelayEndpointPolicy::TrustedService;
        let cluster = Uuid::new_v4();
        let service_uid = Uuid::new_v4();
        record.endpoint_service_identity = format!("k8s://{cluster}/tenant-a/buzz/{service_uid}");
        record.relay_host = "buzz.tenant-a.svc.cluster.local".into();
        record.relay_url = format!("wss://{}", record.relay_host);
        record.approved_ips = vec!["10.96.32.10".parse().unwrap()];
        let private = TrustedRelayMapping::validate(record, Utc::now()).unwrap();
        let exact_roots = RelayTlsTrustConfig::TrustedService {
            endpoint_service_identity: private.record().endpoint_service_identity.clone(),
            roots: vec![CertificateDer::from(vec![1, 2, 3])],
        };
        assert!(validate_trust_profile(&private, &exact_roots).is_ok());
        assert!(validate_trust_profile(
            &private,
            &RelayTlsTrustConfig::TrustedService {
                endpoint_service_identity: "other-service".into(),
                roots: vec![CertificateDer::from(vec![1, 2, 3])],
            }
        )
        .is_err());
        assert!(tls_config(&exact_roots).is_err());
    }

    #[test]
    fn nip11_response_requires_bounded_persistent_http_1_1_json() {
        let valid = b"HTTP/1.1 200 OK\r\nContent-Type: application/nostr+json; charset=utf-8\r\nContent-Length: 10\r\n\r\n";
        let parsed = parse_nip11_headers(valid, false).unwrap().unwrap();
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
            assert!(parse_nip11_headers(response, false).is_err());
        }
    }

    #[test]
    fn nip11_application_json_is_accepted_only_for_trusted_service() {
        let response =
            b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n";
        assert!(parse_nip11_headers(response, false).is_err());
        assert!(parse_nip11_headers(response, true).is_ok());
    }

    #[tokio::test]
    async fn resolver_rejects_loopback_before_any_socket_dial() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let mapping = mapping("localhost", port);

        assert!(matches!(
            resolve_connection_attempt(&mapping, &mapping.scope()).await,
            Err(super::UpstreamBuzzTransportError::Mapping(
                MappingError::UnsafeDnsTargets
            ))
        ));
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(25), listener.accept())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn pinned_connection_tries_remaining_approved_addresses() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let ips: [std::net::IpAddr; 2] =
            ["127.0.0.2".parse().unwrap(), "127.0.0.1".parse().unwrap()];

        let stream = connect_pinned(&ips, port).await.unwrap();
        let (_, peer) = tokio::time::timeout(Duration::from_secs(1), listener.accept())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            stream.peer_addr().unwrap().ip(),
            "127.0.0.1".parse::<std::net::IpAddr>().unwrap()
        );
        assert_eq!(peer.ip(), "127.0.0.1".parse::<std::net::IpAddr>().unwrap());
    }
}

#[cfg(all(test, feature = "upstream-buzz-runtime"))]
mod disposable_service_tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use ed25519_dalek::{Signer, SigningKey};
    use futures_util::SinkExt;
    use nostr::{EventBuilder, Keys, RelayUrl};
    use rustshare_core::domain::{TenantId, WorkspaceId};
    use rustshare_resource_auth::upstream_relay_mapping::{
        RelayEndpointPolicy, RelayMappingRecord,
    };
    use rustshare_resource_auth::upstream_relay_provisioning::{
        MappingProvisioningAction, MappingProvisioningStatement, MappingProvisioningVerifier,
    };
    use rustshare_storage::upstream_relay_mapping_store::UpstreamRelayMappingStore;
    use sqlx::PgPool;
    use std::collections::HashMap;
    use uuid::Uuid;

    fn required(name: &str) -> String {
        std::env::var(name)
            .unwrap_or_else(|_| panic!("{name} must be set for the disposable relay test"))
    }

    fn test_now() -> chrono::DateTime<Utc> {
        Utc.timestamp_micros(Utc::now().timestamp_micros()).unwrap()
    }

    // Run only inside the Cilium-restricted connector namespace against disposable services.
    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL, operator-attested mapping inputs, and private Buzz service"]
    async fn trusted_service_transport_connects_and_authenticates_member() {
        assert_eq!(
            required("BUZZ_TEST_DISPOSABLE_CONFIRM"),
            "DISPOSABLE_ONLY",
            "this test creates permanent mapping audit rows; use a disposable database"
        );
        let database_url = required("DATABASE_URL");
        let relay_host = required("BUZZ_TEST_RELAY_HOST");
        let relay_url = required("BUZZ_TEST_RELAY_URL");
        let community_id = required("BUZZ_TEST_COMMUNITY_ID");
        let relay_pubkey = required("BUZZ_TEST_RELAY_PUBKEY");
        let deployment_digest = required("BUZZ_TEST_DEPLOYMENT_DIGEST");
        let service_identity = required("BUZZ_TEST_SERVICE_IDENTITY");
        let spki_pin = required("BUZZ_TEST_TLS_SPKI_PIN");
        let trusted_spki_pin = spki_pin.clone();
        let approved_ip = required("BUZZ_TEST_APPROVED_IP").parse().unwrap();
        let root_der = std::fs::read(required("BUZZ_TEST_TLS_ROOT_DER"))
            .expect("read the disposable service CA certificate in DER format");
        let user_keys = Keys::parse(&required("BUZZ_TEST_MEMBER_NSEC"))
            .expect("disposable relay member nsec must parse");

        let pool = PgPool::connect(&database_url).await.unwrap();
        sqlx::migrate!("../migrations").run(&pool).await.unwrap();
        let tenant_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        sqlx::query("INSERT INTO tenants (id,name) VALUES ($1,$2)")
            .bind(tenant_id)
            .bind(format!("disposable Buzz transport {tenant_id}"))
            .execute(&pool)
            .await
            .unwrap();

        let operator_service_id = Uuid::new_v4();
        let operator_key_id = "disposable-test-authority-v1".to_owned();
        let operator_key = SigningKey::from_bytes(&rand::random());
        let now = Utc::now();
        let now = Utc.timestamp_micros(now.timestamp_micros()).unwrap();
        let mut statement = MappingProvisioningStatement {
            operator_service_id,
            operator_key_id: operator_key_id.clone(),
            operation_id: Uuid::new_v4(),
            change_reference_id: Uuid::new_v4(),
            action: MappingProvisioningAction::Create,
            expected_generation: 0,
            issued_at: now,
            expires_at: now + chrono::Duration::minutes(2),
            record: RelayMappingRecord {
                mapping_id: Uuid::new_v4(),
                tenant_id: TenantId(tenant_id),
                workspace_id: WorkspaceId(workspace_id),
                community_id,
                endpoint_policy: RelayEndpointPolicy::TrustedService,
                relay_deployment_id: Uuid::new_v4(),
                endpoint_service_identity: service_identity.clone(),
                tls_peer_spki_sha256: spki_pin,
                relay_url,
                relay_host,
                relay_pubkey,
                deployment_digest,
                approved_ips: vec![approved_ip],
                generation: 1,
                active: true,
                valid_until: now + chrono::Duration::hours(1),
                updated_by: operator_service_id,
                audit_entry_id: Uuid::new_v4(),
            },
            signature: Vec::new(),
        };
        statement.signature = operator_key
            .sign(&statement.signing_bytes().unwrap())
            .to_bytes()
            .to_vec();
        let verifier = Arc::new(
            MappingProvisioningVerifier::new(
                operator_service_id,
                HashMap::from([(operator_key_id, operator_key.verifying_key())]),
            )
            .unwrap(),
        );
        let store = UpstreamRelayMappingStore::with_provisioning_verifier(pool.clone(), verifier);
        store.apply(&statement).await.unwrap();

        let wrong_trust = RelayTlsTrustConfig::TrustedService {
            endpoint_service_identity: "k8s://wrong/tenant-a/buzz-edge/wrong".into(),
            roots: vec![CertificateDer::from(root_der.clone())],
        };
        assert!(connect(
            &store,
            TenantId(tenant_id),
            WorkspaceId(workspace_id),
            &wrong_trust
        )
        .await
        .is_err());

        let mut wrong_pin = statement.clone();
        wrong_pin.operation_id = Uuid::new_v4();
        wrong_pin.change_reference_id = Uuid::new_v4();
        wrong_pin.action = MappingProvisioningAction::Update;
        wrong_pin.expected_generation = 1;
        wrong_pin.record.generation = 2;
        wrong_pin.record.audit_entry_id = Uuid::new_v4();
        wrong_pin.record.tls_peer_spki_sha256 = format!("sha256:{}", "00".repeat(32));
        wrong_pin.issued_at = test_now();
        wrong_pin.expires_at = wrong_pin.issued_at + chrono::Duration::minutes(2);
        wrong_pin.signature = operator_key
            .sign(&wrong_pin.signing_bytes().unwrap())
            .to_bytes()
            .to_vec();
        store.apply(&wrong_pin).await.unwrap();
        assert!(matches!(
            connect(
                &store,
                TenantId(tenant_id),
                WorkspaceId(workspace_id),
                &RelayTlsTrustConfig::TrustedService {
                    endpoint_service_identity: service_identity.clone(),
                    roots: vec![CertificateDer::from(root_der.clone())],
                },
            )
            .await,
            Err(UpstreamBuzzTransportError::Connection)
        ));

        let mut statement = wrong_pin;
        statement.operation_id = Uuid::new_v4();
        statement.change_reference_id = Uuid::new_v4();
        statement.expected_generation = 2;
        statement.record.generation = 3;
        statement.record.audit_entry_id = Uuid::new_v4();
        statement.record.tls_peer_spki_sha256 = trusted_spki_pin;
        statement.issued_at = test_now();
        statement.expires_at = statement.issued_at + chrono::Duration::minutes(2);
        statement.signature = operator_key
            .sign(&statement.signing_bytes().unwrap())
            .to_bytes()
            .to_vec();
        store.apply(&statement).await.unwrap();

        let mut session = connect(
            &store,
            TenantId(tenant_id),
            WorkspaceId(workspace_id),
            &RelayTlsTrustConfig::TrustedService {
                endpoint_service_identity: service_identity,
                roots: vec![CertificateDer::from(root_der)],
            },
        )
        .await
        .unwrap();
        assert_eq!(session.scope().generation, 3);

        let challenge = tokio::time::timeout(Duration::from_secs(15), session.receive())
            .await
            .expect("relay AUTH challenge timed out")
            .unwrap()
            .expect("relay closed before AUTH challenge");
        let Message::Text(challenge) = challenge else {
            panic!("expected upstream relay AUTH frame")
        };
        let challenge: Value = serde_json::from_str(&challenge).unwrap();
        assert_eq!(challenge[0], "AUTH");
        let challenge = challenge[1].as_str().expect("AUTH challenge string");
        let relay = RelayUrl::parse(&required("BUZZ_TEST_RELAY_URL")).unwrap();
        let auth = EventBuilder::auth(challenge, relay)
            .sign_with_keys(&user_keys)
            .unwrap();
        let auth_id = auth.id.to_hex();
        let socket = session.socket.as_mut().expect("connected socket");
        socket
            .send(Message::Text(
                serde_json::json!(["AUTH", auth]).to_string().into(),
            ))
            .await
            .unwrap();

        loop {
            let frame = tokio::time::timeout(Duration::from_secs(15), session.receive())
                .await
                .expect("relay AUTH response timed out")
                .unwrap()
                .expect("relay closed before AUTH response");
            let Message::Text(frame) = frame else {
                continue;
            };
            let response: Value = serde_json::from_str(&frame).unwrap();
            if response[0] == "OK" && response[1] == auth_id {
                assert!(
                    response[2].as_bool().unwrap_or(false),
                    "upstream relay rejected member NIP-42"
                );
                break;
            }
        }

        let dm_channel = required("BUZZ_TEST_DM_CHANNEL_ID");
        let expected_dm_id = required("BUZZ_TEST_EXPECTED_DM_EVENT_ID");
        let expected_author = required("BUZZ_TEST_EXPECTED_DM_AUTHOR");
        let other_member = required("BUZZ_TEST_OTHER_MEMBER_PUBKEY");
        let subscription = Uuid::new_v4().to_string();
        session
            .socket
            .as_mut()
            .expect("connected socket")
            .send(Message::Text(
                serde_json::json!([
                    "REQ",
                    subscription,
                    {"kinds": [9], "#h": [dm_channel], "limit": 50}
                ])
                .to_string()
                .into(),
            ))
            .await
            .unwrap();
        let mut expected_dm_seen = false;
        let mut observed_dm_events = Vec::new();
        loop {
            let frame = tokio::time::timeout(Duration::from_secs(15), session.receive())
                .await
                .expect("private-channel history timed out")
                .unwrap()
                .expect("relay closed before private-channel history completed");
            let Message::Text(frame) = frame else {
                continue;
            };
            let response: Value = serde_json::from_str(&frame).unwrap();
            if response[0] == "EVENT" && response[1] == subscription {
                let event = &response[2];
                let has_expected_channel = event["tags"].as_array().is_some_and(|tags| {
                    tags.iter().any(|tag| tag[0] == "h" && tag[1] == dm_channel)
                });
                assert!(
                    has_expected_channel,
                    "relay returned an event outside the requested channel"
                );
                assert_eq!(event["kind"], 9);
                let author = event["pubkey"].as_str().unwrap_or_default();
                assert!(
                    author == expected_author || author == other_member,
                    "private channel returned an event from a nonmember author"
                );
                let event_id = event["id"].as_str().unwrap_or_default();
                observed_dm_events.push(format!("{event_id}:{author}"));
                if event_id == expected_dm_id.as_str() {
                    assert_eq!(author, expected_author);
                }
                expected_dm_seen |= event_id == expected_dm_id.as_str();
            } else if response[0] == "EOSE" && response[1] == subscription {
                break;
            }
        }
        assert!(
            expected_dm_seen,
            "member session did not retrieve the expected private-channel DM {expected_dm_id}; observed {observed_dm_events:?}"
        );

        statement.operation_id = Uuid::new_v4();
        statement.change_reference_id = Uuid::new_v4();
        statement.action = MappingProvisioningAction::Update;
        statement.expected_generation = 3;
        statement.record.generation = 4;
        statement.record.audit_entry_id = Uuid::new_v4();
        statement.issued_at = test_now();
        statement.expires_at = statement.issued_at + chrono::Duration::minutes(2);
        statement.signature = operator_key
            .sign(&statement.signing_bytes().unwrap())
            .to_bytes()
            .to_vec();
        store.apply(&statement).await.unwrap();
        assert!(matches!(
            tokio::time::timeout(
                SESSION_REVALIDATE_INTERVAL + Duration::from_secs(2),
                session.receive()
            )
            .await
            .expect("mapping generation revalidation timed out"),
            Err(UpstreamBuzzTransportError::SessionFenced)
        ));
    }
}
