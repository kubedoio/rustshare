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
        let tls_result = tokio::time::timeout(
            CONNECT_TIMEOUT,
            TlsConnector::from(tls_config).connect(server_name, tcp),
        )
        .await;
        let tls = match tls_result {
            Ok(Ok(tls)) => tls,
            Ok(Err(_)) => {
                #[cfg(test)]
                eprintln!("upstream transport stage: TLS handshake rejected");
                return Err(UpstreamBuzzTransportError::Connection);
            }
            Err(_) => {
                #[cfg(test)]
                eprintln!("upstream transport stage: TLS handshake timed out");
                return Err(UpstreamBuzzTransportError::Connection);
            }
        };
        #[cfg(test)]
        eprintln!("upstream transport stage: TLS certificate and hostname verified");
        if verify_tls_spki_pin(&tls, &attempt.scope().tls_peer_spki_sha256).is_err() {
            #[cfg(test)]
            eprintln!("upstream transport stage: TLS SPKI pin rejected");
            return Err(UpstreamBuzzTransportError::Connection);
        }
        #[cfg(test)]
        eprintln!("upstream transport stage: TLS SPKI pin verified");
        let mut tls = BufStream::new(tls);
        if verify_nip11(&attempt, &mut tls).await.is_err() {
            #[cfg(test)]
            eprintln!("upstream transport stage: NIP-11 identity rejected");
            return Err(UpstreamBuzzTransportError::Connection);
        }
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

    async fn authenticate_member(
        session: &mut TenantRelaySession,
        user_keys: &Keys,
        relay_url: &str,
    ) {
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
        let relay = RelayUrl::parse(relay_url).unwrap();
        let auth = EventBuilder::auth(challenge, relay)
            .sign_with_keys(user_keys)
            .unwrap();
        let auth_id = auth.id.to_hex();
        session
            .socket
            .as_mut()
            .expect("connected socket")
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
                return;
            }
        }
    }

    async fn read_channel_history(
        session: &mut TenantRelaySession,
        channel_id: &str,
    ) -> (String, Vec<(String, String)>) {
        let subscription = Uuid::new_v4().to_string();
        session
            .socket
            .as_mut()
            .expect("connected socket")
            .send(Message::Text(
                serde_json::json!([
                    "REQ",
                    subscription,
                    {"kinds": [9], "#h": [channel_id], "limit": 50}
                ])
                .to_string()
                .into(),
            ))
            .await
            .unwrap();
        let mut events = Vec::new();
        let terminal = loop {
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
                let has_channel = event["tags"].as_array().is_some_and(|tags| {
                    tags.iter().any(|tag| tag[0] == "h" && tag[1] == channel_id)
                });
                assert_eq!(event["kind"], 9);
                assert!(
                    has_channel,
                    "relay returned an event outside the requested channel"
                );
                let id = event["id"].as_str().expect("event id");
                let author = event["pubkey"].as_str().expect("event author");
                assert_eq!(id.len(), 64, "NIP-01 event ids are 32-byte hex");
                assert_eq!(author.len(), 64, "Nostr public keys are 32-byte hex");
                assert!(id.bytes().all(|byte| byte.is_ascii_hexdigit()));
                assert!(author.bytes().all(|byte| byte.is_ascii_hexdigit()));
                events.push((id.to_owned(), author.to_owned()));
            } else if response[0] == "EOSE" && response[1] == subscription {
                break "EOSE";
            } else if response[0] == "CLOSED" && response[1] == subscription {
                break if response[2]
                    .as_str()
                    .is_some_and(|reason| reason.starts_with("restricted:"))
                {
                    "CLOSED_RESTRICTED"
                } else {
                    "CLOSED_OTHER"
                };
            }
        };
        if terminal == "EOSE" {
            session
                .socket
                .as_mut()
                .expect("connected socket")
                .send(Message::Text(
                    serde_json::json!(["CLOSE", subscription])
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
        }
        (terminal.to_owned(), events)
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
        let database_url =
            crate::test_guard::disposable_database_url_from_env().unwrap_or_else(|message| {
                panic!("trusted-service test requires disposable PostgreSQL: {message}")
            });
        let relay_host = required("BUZZ_TEST_RELAY_HOST");
        let relay_url = required("BUZZ_TEST_RELAY_URL");
        let community_id = required("BUZZ_TEST_COMMUNITY_ID");
        let relay_pubkey = required("BUZZ_TEST_RELAY_PUBKEY");
        let trusted_relay_pubkey = relay_pubkey.clone();
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
                relay_url: relay_url.clone(),
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
        let store =
            UpstreamRelayMappingStore::with_provisioning_verifier(pool.clone(), verifier.clone());
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

        let mut wrong_relay_identity = statement.clone();
        wrong_relay_identity.operation_id = Uuid::new_v4();
        wrong_relay_identity.change_reference_id = Uuid::new_v4();
        wrong_relay_identity.action = MappingProvisioningAction::Update;
        wrong_relay_identity.expected_generation = 3;
        wrong_relay_identity.record.generation = 4;
        wrong_relay_identity.record.audit_entry_id = Uuid::new_v4();
        wrong_relay_identity.record.relay_pubkey = Keys::generate().public_key().to_hex();
        wrong_relay_identity.issued_at = test_now();
        wrong_relay_identity.expires_at =
            wrong_relay_identity.issued_at + chrono::Duration::minutes(2);
        wrong_relay_identity.signature = operator_key
            .sign(&wrong_relay_identity.signing_bytes().unwrap())
            .to_bytes()
            .to_vec();
        store.apply(&wrong_relay_identity).await.unwrap();
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

        statement = wrong_relay_identity;
        statement.operation_id = Uuid::new_v4();
        statement.change_reference_id = Uuid::new_v4();
        statement.expected_generation = 4;
        statement.record.generation = 5;
        statement.record.audit_entry_id = Uuid::new_v4();
        statement.record.relay_pubkey = trusted_relay_pubkey;
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
                endpoint_service_identity: service_identity.clone(),
                roots: vec![CertificateDer::from(root_der.clone())],
            },
        )
        .await
        .unwrap();
        assert_eq!(session.scope().generation, 5);

        authenticate_member(&mut session, &user_keys, &relay_url).await;

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
        statement.expected_generation = 5;
        statement.record.generation = 6;
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

        let outage_pool = PgPool::connect(&database_url).await.unwrap();
        let outage_store = UpstreamRelayMappingStore::with_provisioning_verifier(
            outage_pool.clone(),
            verifier.clone(),
        );
        let mut subscribed_session = connect(
            &outage_store,
            TenantId(tenant_id),
            WorkspaceId(workspace_id),
            &RelayTlsTrustConfig::TrustedService {
                endpoint_service_identity: service_identity.clone(),
                roots: vec![CertificateDer::from(root_der.clone())],
            },
        )
        .await
        .unwrap();
        authenticate_member(&mut subscribed_session, &user_keys, &relay_url).await;
        let outage_subscription = Uuid::new_v4().to_string();
        subscribed_session
            .socket
            .as_mut()
            .expect("connected socket")
            .send(Message::Text(
                serde_json::json!([
                    "REQ",
                    outage_subscription,
                    {"kinds": [9], "#h": [dm_channel], "limit": 50}
                ])
                .to_string()
                .into(),
            ))
            .await
            .unwrap();
        let mut outage_subscription_saw_dm = false;
        loop {
            let frame = tokio::time::timeout(Duration::from_secs(15), subscribed_session.receive())
                .await
                .expect("outage test subscription timed out")
                .unwrap()
                .expect("relay closed before outage test subscription completed");
            let Message::Text(frame) = frame else {
                continue;
            };
            let response: Value = serde_json::from_str(&frame).unwrap();
            if response[0] == "EVENT" && response[1] == outage_subscription {
                outage_subscription_saw_dm |= response[2]["id"] == expected_dm_id;
            } else if response[0] == "EOSE" && response[1] == outage_subscription {
                break;
            }
        }
        assert!(
            outage_subscription_saw_dm,
            "active NIP-29 subscription saw no expected DM"
        );

        let outage_ack_path = std::path::PathBuf::from(required("BUZZ_TEST_DB_OUTAGE_ACK_FILE"));
        assert!(
            !outage_ack_path.exists(),
            "database-outage acknowledgement must start absent"
        );
        println!("BUZZ_TEST_DB_OUTAGE_READY");
        std::io::Write::flush(&mut std::io::stdout()).unwrap();
        tokio::time::timeout(Duration::from_secs(120), async {
            while !outage_ack_path.exists() {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("disposable PostgreSQL outage acknowledgement timed out");
        assert!(matches!(
            tokio::time::timeout(
                SESSION_REVALIDATE_INTERVAL + CONNECT_TIMEOUT + Duration::from_secs(2),
                subscribed_session.receive()
            )
            .await
            .expect("database-outage fencing timed out"),
            Err(UpstreamBuzzTransportError::SessionFenced)
        ));
        assert!(subscribed_session.socket.is_none());

        let restore_ack_path = std::path::PathBuf::from(required("BUZZ_TEST_DB_RESTORE_ACK_FILE"));
        assert!(
            !restore_ack_path.exists(),
            "database-restore acknowledgement must start absent"
        );
        println!("BUZZ_TEST_DB_RESTORE_READY");
        std::io::Write::flush(&mut std::io::stdout()).unwrap();
        tokio::time::timeout(Duration::from_secs(120), async {
            while !restore_ack_path.exists() {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("disposable PostgreSQL restore acknowledgement timed out");

        let restored_pool = PgPool::connect(&database_url).await.unwrap();
        let restored_store =
            UpstreamRelayMappingStore::with_provisioning_verifier(restored_pool, verifier);
        let mut revoked_session = connect(
            &restored_store,
            TenantId(tenant_id),
            WorkspaceId(workspace_id),
            &RelayTlsTrustConfig::TrustedService {
                endpoint_service_identity: service_identity.clone(),
                roots: vec![CertificateDer::from(root_der.clone())],
            },
        )
        .await
        .unwrap();
        assert_eq!(revoked_session.scope().generation, 6);

        statement.operation_id = Uuid::new_v4();
        statement.change_reference_id = Uuid::new_v4();
        statement.action = MappingProvisioningAction::Deactivate;
        statement.expected_generation = 6;
        statement.record.generation = 7;
        statement.record.active = false;
        statement.record.audit_entry_id = Uuid::new_v4();
        statement.issued_at = test_now();
        statement.expires_at = statement.issued_at + chrono::Duration::minutes(2);
        statement.signature = operator_key
            .sign(&statement.signing_bytes().unwrap())
            .to_bytes()
            .to_vec();
        restored_store.apply(&statement).await.unwrap();
        assert!(matches!(
            tokio::time::timeout(
                SESSION_REVALIDATE_INTERVAL + Duration::from_secs(2),
                revoked_session.receive()
            )
            .await
            .expect("mapping revocation revalidation timed out"),
            Err(UpstreamBuzzTransportError::SessionFenced)
        ));
    }

    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL and two isolated private Buzz relays in the Cilium test namespace"]
    async fn trusted_service_tenant_sessions_are_isolated_across_private_dm_relays() {
        assert_eq!(
            required("BUZZ_TEST_DISPOSABLE_CONFIRM"),
            "DISPOSABLE_ONLY",
            "this test creates permanent mapping audit rows; use a disposable database"
        );
        let database_url =
            crate::test_guard::disposable_database_url_from_env().unwrap_or_else(|message| {
                panic!("cross-tenant Buzz test requires disposable PostgreSQL: {message}")
            });
        let relay_url_a = required("BUZZ_TEST_RELAY_URL");
        let relay_host_a = required("BUZZ_TEST_RELAY_HOST");
        let community_id_a = required("BUZZ_TEST_COMMUNITY_ID");
        let relay_pubkey_a = required("BUZZ_TEST_RELAY_PUBKEY");
        let deployment_digest_a = required("BUZZ_TEST_DEPLOYMENT_DIGEST");
        let service_identity_a = required("BUZZ_TEST_SERVICE_IDENTITY");
        let spki_pin_a = required("BUZZ_TEST_TLS_SPKI_PIN");
        let approved_ip_a = required("BUZZ_TEST_APPROVED_IP")
            .parse::<std::net::IpAddr>()
            .unwrap();
        let root_der_a = std::fs::read(required("BUZZ_TEST_TLS_ROOT_DER"))
            .expect("read tenant A's disposable service CA certificate");
        let member_a = Keys::parse(&required("BUZZ_TEST_MEMBER_NSEC"))
            .expect("tenant A's disposable relay member nsec must parse");
        let channel_a = required("BUZZ_TEST_DM_CHANNEL_ID");
        let event_id_a = required("BUZZ_TEST_EXPECTED_DM_EVENT_ID");
        let author_a = required("BUZZ_TEST_EXPECTED_DM_AUTHOR");

        let relay_url_b = required("BUZZ_TEST_B_RELAY_URL");
        let relay_host_b = required("BUZZ_TEST_B_RELAY_HOST");
        let community_id_b = required("BUZZ_TEST_B_COMMUNITY_ID");
        let relay_pubkey_b = required("BUZZ_TEST_B_RELAY_PUBKEY");
        let deployment_digest_b = required("BUZZ_TEST_B_DEPLOYMENT_DIGEST");
        let service_identity_b = required("BUZZ_TEST_B_SERVICE_IDENTITY");
        let spki_pin_b = required("BUZZ_TEST_B_TLS_SPKI_PIN");
        let approved_ip_b = required("BUZZ_TEST_B_APPROVED_IP")
            .parse::<std::net::IpAddr>()
            .unwrap();
        let root_der_b = std::fs::read(required("BUZZ_TEST_B_TLS_ROOT_DER"))
            .expect("read tenant B's disposable service CA certificate");
        let member_b = Keys::parse(&required("BUZZ_TEST_B_MEMBER_NSEC"))
            .expect("tenant B's disposable relay member nsec must parse");
        let channel_b = required("BUZZ_TEST_B_DM_CHANNEL_ID");
        let event_id_b = required("BUZZ_TEST_B_EXPECTED_DM_EVENT_ID");
        let author_b = required("BUZZ_TEST_B_EXPECTED_DM_AUTHOR");

        assert_ne!(community_id_a, community_id_b);
        assert_ne!(relay_url_a, relay_url_b);
        assert_ne!(relay_host_a, relay_host_b);
        assert_ne!(relay_pubkey_a, relay_pubkey_b);
        assert_ne!(service_identity_a, service_identity_b);
        assert_ne!(spki_pin_a, spki_pin_b);
        assert_ne!(channel_a, channel_b);

        let pool = PgPool::connect(&database_url).await.unwrap();
        sqlx::migrate!("../migrations").run(&pool).await.unwrap();
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let workspace_a = Uuid::new_v4();
        let workspace_b = Uuid::new_v4();
        for (tenant_id, label) in [(tenant_a, "A"), (tenant_b, "B")] {
            sqlx::query("INSERT INTO tenants (id,name) VALUES ($1,$2)")
                .bind(tenant_id)
                .bind(format!("disposable cross-tenant Buzz {label} {tenant_id}"))
                .execute(&pool)
                .await
                .unwrap();
        }

        let operator_service_id = Uuid::new_v4();
        let operator_key_id = "disposable-two-tenant-authority-v1".to_owned();
        let operator_key = SigningKey::from_bytes(&rand::random());
        let now = test_now();
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
                tenant_id: TenantId(tenant_a),
                workspace_id: WorkspaceId(workspace_a),
                community_id: community_id_a.clone(),
                endpoint_policy: RelayEndpointPolicy::TrustedService,
                relay_deployment_id: Uuid::new_v4(),
                endpoint_service_identity: service_identity_a.clone(),
                tls_peer_spki_sha256: spki_pin_a.clone(),
                relay_url: relay_url_a.clone(),
                relay_host: relay_host_a,
                relay_pubkey: relay_pubkey_a,
                deployment_digest: deployment_digest_a,
                approved_ips: vec![approved_ip_a],
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

        let mut statement_b = statement.clone();
        statement_b.operation_id = Uuid::new_v4();
        statement_b.change_reference_id = Uuid::new_v4();
        statement_b.record.mapping_id = Uuid::new_v4();
        statement_b.record.tenant_id = TenantId(tenant_b);
        statement_b.record.workspace_id = WorkspaceId(workspace_b);
        statement_b.record.community_id = community_id_b.clone();
        statement_b.record.relay_deployment_id = Uuid::new_v4();
        statement_b.record.endpoint_service_identity = service_identity_b.clone();
        statement_b.record.tls_peer_spki_sha256 = spki_pin_b.clone();
        statement_b.record.relay_url = relay_url_b.clone();
        statement_b.record.relay_host = relay_host_b;
        statement_b.record.relay_pubkey = relay_pubkey_b;
        statement_b.record.deployment_digest = deployment_digest_b;
        statement_b.record.approved_ips = vec![approved_ip_b];
        statement_b.record.audit_entry_id = Uuid::new_v4();
        statement_b.signature = operator_key
            .sign(&statement_b.signing_bytes().unwrap())
            .to_bytes()
            .to_vec();
        store.apply(&statement_b).await.unwrap();

        let trust_a = RelayTlsTrustConfig::TrustedService {
            endpoint_service_identity: service_identity_a,
            roots: vec![CertificateDer::from(root_der_a)],
        };
        let trust_b = RelayTlsTrustConfig::TrustedService {
            endpoint_service_identity: service_identity_b,
            roots: vec![CertificateDer::from(root_der_b)],
        };

        let wrong_mapping = connect(
            &store,
            TenantId(tenant_b),
            WorkspaceId(workspace_a),
            &trust_a,
        )
        .await;
        match wrong_mapping {
            Err(error) => assert!(
                matches!(
                    &error,
                    UpstreamBuzzTransportError::MappingStore(
                        UpstreamRelayMappingStoreError::Policy(MappingError::MissingMapping)
                    )
                ),
                "cross-tenant lookup must fail before network access; got {error:?}"
            ),
            Ok(_) => panic!("cross-tenant lookup unexpectedly connected"),
        }

        let mut session_a = connect(
            &store,
            TenantId(tenant_a),
            WorkspaceId(workspace_a),
            &trust_a,
        )
        .await
        .expect("tenant A must connect to its mapped private relay");
        let mut session_b = connect(
            &store,
            TenantId(tenant_b),
            WorkspaceId(workspace_b),
            &trust_b,
        )
        .await
        .expect("tenant B must connect to its own mapped private relay");
        assert_eq!(session_a.scope().tenant_id, TenantId(tenant_a));
        assert_eq!(session_a.scope().workspace_id, WorkspaceId(workspace_a));
        assert_eq!(session_a.scope().community_id, community_id_a);
        assert_eq!(
            session_a.scope().endpoint_policy,
            RelayEndpointPolicy::TrustedService
        );
        assert_eq!(session_a.scope().generation, 1);
        assert_eq!(session_b.scope().tenant_id, TenantId(tenant_b));
        assert_eq!(session_b.scope().workspace_id, WorkspaceId(workspace_b));
        assert_eq!(session_b.scope().community_id, community_id_b);
        assert_eq!(
            session_b.scope().endpoint_policy,
            RelayEndpointPolicy::TrustedService
        );
        assert_eq!(session_b.scope().generation, 1);
        assert_ne!(session_a.scope().mapping_id, session_b.scope().mapping_id);
        assert_ne!(
            session_a.scope().relay_deployment_id,
            session_b.scope().relay_deployment_id
        );
        assert_ne!(
            session_a.scope().endpoint_service_identity,
            session_b.scope().endpoint_service_identity
        );

        authenticate_member(&mut session_a, &member_a, &relay_url_a).await;
        authenticate_member(&mut session_b, &member_b, &relay_url_b).await;

        let (terminal_a, events_a) = read_channel_history(&mut session_a, &channel_a).await;
        assert_eq!(terminal_a, "EOSE");
        assert!(events_a
            .iter()
            .any(|(id, author)| id == &event_id_a && author == &author_a));
        let (terminal_b, events_b) = read_channel_history(&mut session_b, &channel_b).await;
        assert_eq!(terminal_b, "EOSE");
        assert!(events_b
            .iter()
            .any(|(id, author)| id == &event_id_b && author == &author_b));

        let (foreign_terminal, foreign_events) =
            read_channel_history(&mut session_b, &channel_a).await;
        println!(
            "cross_tenant_private_history terminal={foreign_terminal} event_count={}",
            foreign_events.len()
        );
        assert!(
            matches!(foreign_terminal.as_str(), "EOSE" | "CLOSED_RESTRICTED"),
            "unexpected terminal response for inaccessible private channel"
        );
        assert!(
            foreign_events.is_empty(),
            "tenant B's mapped relay returned tenant A private-channel events"
        );
    }

    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL and the TLS WebSocket handshake-stall service"]
    async fn trusted_service_mapping_change_during_websocket_upgrade_is_rejected() {
        assert_eq!(
            required("BUZZ_TEST_DISPOSABLE_CONFIRM"),
            "DISPOSABLE_ONLY",
            "this test creates permanent mapping audit rows; use a disposable database"
        );
        let database_url = required("DATABASE_URL");
        let relay_host = required("BUZZ_TEST_RACE_HOST");
        let relay_url = required("BUZZ_TEST_RACE_RELAY_URL");
        let approved_ip = required("BUZZ_TEST_RACE_APPROVED_IP")
            .parse::<std::net::IpAddr>()
            .unwrap();
        let root_der = std::fs::read(required("BUZZ_TEST_RACE_TLS_ROOT_DER"))
            .expect("read the disposable handshake-stall CA certificate");
        let community_id = Uuid::new_v4().to_string();
        let relay_pubkey = required("BUZZ_TEST_RACE_RELAY_PUBKEY");
        let service_identity = required("BUZZ_TEST_RACE_SERVICE_IDENTITY");
        let deployment_digest = required("BUZZ_TEST_RACE_DEPLOYMENT_DIGEST");
        let spki_pin = required("BUZZ_TEST_RACE_TLS_SPKI_PIN");
        let control_token = required("BUZZ_TEST_RACE_CONTROL_TOKEN");
        assert!(control_token.len() >= 32, "provide a per-run control token");

        let pool = PgPool::connect(&database_url).await.unwrap();
        sqlx::migrate!("../migrations").run(&pool).await.unwrap();
        let tenant_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        sqlx::query("INSERT INTO tenants (id,name) VALUES ($1,$2)")
            .bind(tenant_id)
            .bind(format!("disposable handshake race {tenant_id}"))
            .execute(&pool)
            .await
            .unwrap();

        let operator_service_id = Uuid::new_v4();
        let operator_key_id = "disposable-handshake-race-authority-v1".to_owned();
        let operator_key = SigningKey::from_bytes(&rand::random());
        let now = test_now();
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
                relay_host: relay_host.clone(),
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

        let connect_trust = RelayTlsTrustConfig::TrustedService {
            endpoint_service_identity: service_identity,
            roots: vec![CertificateDer::from(root_der.clone())],
        };
        let connect_store = store.clone();
        let mut connection = tokio::spawn(async move {
            connect(
                &connect_store,
                TenantId(tenant_id),
                WorkspaceId(workspace_id),
                &connect_trust,
            )
            .await
        });

        let control_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .add_root_certificate(reqwest::Certificate::from_der(&root_der).unwrap())
            .resolve_to_addrs(&relay_host, &[std::net::SocketAddr::new(approved_ip, 443)])
            .build()
            .unwrap();
        let status_url = format!("https://{relay_host}/__test/status");
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                if connection.is_finished() {
                    panic!("connector completed before the test service held its upgrade");
                }
                if let Ok(response) = control_client
                    .get(&status_url)
                    .bearer_auth(&control_token)
                    .send()
                    .await
                {
                    if response
                        .json::<Value>()
                        .await
                        .is_ok_and(|status| status["waiting"] == true)
                    {
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("test service did not hold the WebSocket upgrade");

        statement.operation_id = Uuid::new_v4();
        statement.change_reference_id = Uuid::new_v4();
        statement.action = MappingProvisioningAction::Update;
        statement.expected_generation = 1;
        statement.record.generation = 2;
        statement.record.audit_entry_id = Uuid::new_v4();
        statement.issued_at = test_now();
        statement.expires_at = statement.issued_at + chrono::Duration::minutes(2);
        statement.signature = operator_key
            .sign(&statement.signing_bytes().unwrap())
            .to_bytes()
            .to_vec();
        store.apply(&statement).await.unwrap();

        control_client
            .post(format!("https://{relay_host}/__test/release"))
            .bearer_auth(&control_token)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();

        let result = tokio::time::timeout(Duration::from_secs(5), &mut connection)
            .await
            .expect("connector did not finish after the test released the upgrade")
            .unwrap();
        assert!(matches!(
            result,
            Err(UpstreamBuzzTransportError::Mapping(
                MappingError::MappingChangedDuringConnect
            ))
        ));
    }
}
