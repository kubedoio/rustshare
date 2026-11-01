//! Isolated policy contract for a future upstream-only Buzz connection.
//!
//! This module is not wired to a handler, relay client, or production profile.
//! A caller must load mappings only from the audited Elembra control plane and
//! must make its TLS connector honor the DNS snapshot and SNI fields below.

use std::net::IpAddr;
use std::sync::LazyLock;

use chrono::{DateTime, Utc};
use ipnet::IpNet;
use nostr::PublicKey;
use rustshare_core::domain::{TenantId, WorkspaceId};
use url::Url;
use uuid::Uuid;

/// Trusted, versioned tenant/workspace binding to one upstream relay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayMappingRecord {
    /// Immutable mapping identity.
    pub mapping_id: Uuid,
    /// Elembra tenant whose users may connect to this relay.
    pub tenant_id: TenantId,
    /// Workspace scoped by the Elembra authorization layer.
    pub workspace_id: WorkspaceId,
    /// Buzz community provisioned on this isolated relay.
    pub community_id: String,
    /// Unique deployment identity allocated by the trusted operator service.
    pub relay_deployment_id: Uuid,
    /// Immutable orchestrator identity, including namespace and service UID.
    pub endpoint_service_identity: String,
    /// SHA-256 digest of the expected TLS peer public key (SPKI).
    pub tls_peer_spki_sha256: String,
    /// Exact operator-configured WebSocket URL; callers never supply this.
    pub relay_url: String,
    /// Canonical DNS host expected in both the URL and TLS certificate.
    pub relay_host: String,
    /// NIP-11 `self` identity, pinned from trusted deployment provenance.
    pub relay_pubkey: String,
    /// Digest of the reviewed deployment artifact/source provenance.
    pub deployment_digest: String,
    /// Operator-approved DNS targets. New DNS answers require a versioned update.
    pub approved_ips: Vec<IpAddr>,
    /// Monotonically increasing configuration generation, starting at 1.
    pub generation: u64,
    /// Inactive and expired mappings are never usable.
    pub active: bool,
    /// Mapping must be revalidated before this time.
    pub valid_until: DateTime<Utc>,
    /// Authenticated deployment/operator service identity that signed this update.
    pub updated_by: Uuid,
    /// Append-only audit record for this exact generation.
    pub audit_entry_id: Uuid,
}

/// A validated mapping. Fields are private so callers cannot edit the snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedRelayMapping {
    record: RelayMappingRecord,
    canonical_url: String,
    port: u16,
}

impl TrustedRelayMapping {
    /// Validate a record loaded from the trusted mapping store.
    pub fn validate(record: RelayMappingRecord, now: DateTime<Utc>) -> Result<Self, MappingError> {
        if !record.active || now >= record.valid_until {
            return Err(MappingError::InactiveOrStale);
        }
        Self::validate_record_structure(&record)?;

        let url = Url::parse(&record.relay_url).map_err(|_| MappingError::InvalidRelayUrl)?;
        let host = url.host_str().ok_or(MappingError::InvalidRelayUrl)?;
        let port = url
            .port_or_known_default()
            .ok_or(MappingError::InvalidRelayUrl)?;
        Ok(Self {
            canonical_url: canonical_relay_url(host, url.port()),
            port,
            record,
        })
    }

    /// Validate immutable deployment fields for signed update and deactivation statements.
    pub fn validate_record_structure(record: &RelayMappingRecord) -> Result<(), MappingError> {
        if record.mapping_id.is_nil()
            || record.tenant_id.0.is_nil()
            || record.workspace_id.0.is_nil()
            || record.updated_by.is_nil()
            || record.audit_entry_id.is_nil()
            || record.generation == 0
        {
            return Err(MappingError::InactiveOrStale);
        }
        let community_id =
            Uuid::parse_str(&record.community_id).map_err(|_| MappingError::InvalidCommunity)?;
        if community_id.to_string() != record.community_id {
            return Err(MappingError::InvalidCommunity);
        }
        if record.relay_deployment_id.is_nil()
            || record.endpoint_service_identity.is_empty()
            || record.endpoint_service_identity.len() > 512
            || !record
                .endpoint_service_identity
                .bytes()
                .all(|byte| byte.is_ascii_graphic())
            || !is_canonical_sha256_digest(&record.tls_peer_spki_sha256)
        {
            return Err(MappingError::InvalidDeploymentProvenance);
        }
        PublicKey::from_hex(&record.relay_pubkey)
            .map_err(|_| MappingError::InvalidRelayIdentity)?;
        if !is_sha256_digest(&record.deployment_digest) {
            return Err(MappingError::InvalidDeploymentProvenance);
        }

        let url = Url::parse(&record.relay_url).map_err(|_| MappingError::InvalidRelayUrl)?;
        if url.scheme() != "wss"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
        {
            return Err(MappingError::InvalidRelayUrl);
        }
        let host = url.host_str().ok_or(MappingError::InvalidRelayUrl)?;
        if host.parse::<IpAddr>().is_ok()
            || host.ends_with('.')
            || host != record.relay_host
            || !record.relay_host.is_ascii()
            || record.relay_host != record.relay_host.to_ascii_lowercase()
        {
            return Err(MappingError::RelayHostMismatch);
        }
        url.port_or_known_default()
            .ok_or(MappingError::InvalidRelayUrl)?;
        let canonical_url = canonical_relay_url(host, url.port());
        if record.relay_url != canonical_url {
            return Err(MappingError::InvalidRelayUrl);
        }
        if record.approved_ips.is_empty()
            || record.approved_ips.len() > 64
            || record.approved_ips.iter().any(is_unsafe_relay_ip)
            || has_duplicates(&record.approved_ips)
        {
            return Err(MappingError::UnsafeDnsTargets);
        }
        Ok(())
    }

    /// The exact canonical URL the transport must use.
    pub fn relay_url(&self) -> &str {
        &self.canonical_url
    }

    /// The immutable record snapshot that was validated by this policy model.
    pub fn record(&self) -> &RelayMappingRecord {
        &self.record
    }

    /// Scope of this relay mapping.
    pub fn scope(&self) -> MappingScope {
        MappingScope {
            tenant_id: self.record.tenant_id,
            workspace_id: self.record.workspace_id,
            mapping_id: self.record.mapping_id,
            relay_deployment_id: self.record.relay_deployment_id,
            endpoint_service_identity: self.record.endpoint_service_identity.clone(),
            tls_peer_spki_sha256: self.record.tls_peer_spki_sha256.clone(),
            generation: self.record.generation,
            community_id: self.record.community_id.clone(),
        }
    }

    /// Resolve once per connection and pin the validated answer set for the
    /// lifetime of that connection attempt. The connector must dial one of
    /// these addresses directly while retaining the mapped host for SNI.
    pub fn begin_connection(
        &self,
        expected_scope: MappingScope,
        resolved_ips: &[IpAddr],
        now: DateTime<Utc>,
    ) -> Result<ConnectionAttempt, MappingError> {
        if expected_scope != self.scope() {
            return Err(MappingError::ScopeMismatch);
        }
        if now >= self.record.valid_until {
            return Err(MappingError::InactiveOrStale);
        }
        if resolved_ips.is_empty()
            || resolved_ips
                .iter()
                .any(|ip| is_unsafe_relay_ip(ip) || !self.record.approved_ips.contains(ip))
        {
            return Err(MappingError::UnsafeDnsTargets);
        }
        Ok(ConnectionAttempt {
            scope: expected_scope,
            mapping_snapshot: self.record.clone(),
            relay_url: self.canonical_url.clone(),
            relay_host: self.record.relay_host.clone(),
            community_id: self.record.community_id.clone(),
            relay_pubkey: self.record.relay_pubkey.clone(),
            endpoint_service_identity: self.record.endpoint_service_identity.clone(),
            tls_peer_spki_sha256: self.record.tls_peer_spki_sha256.clone(),
            port: self.port,
            pinned_ips: resolved_ips.to_vec(),
            valid_until: self.record.valid_until,
        })
    }
}

/// Scope bound to an established WebSocket; never pool/reuse across scopes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingScope {
    pub tenant_id: TenantId,
    pub workspace_id: WorkspaceId,
    pub mapping_id: Uuid,
    pub relay_deployment_id: Uuid,
    pub endpoint_service_identity: String,
    pub tls_peer_spki_sha256: String,
    pub generation: u64,
    pub community_id: String,
}

/// Immutable values a connection attempt must carry to its TLS/relay verifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionAttempt {
    scope: MappingScope,
    mapping_snapshot: RelayMappingRecord,
    relay_url: String,
    relay_host: String,
    community_id: String,
    relay_pubkey: String,
    endpoint_service_identity: String,
    tls_peer_spki_sha256: String,
    port: u16,
    pinned_ips: Vec<IpAddr>,
    valid_until: DateTime<Utc>,
}

impl ConnectionAttempt {
    /// Tenant/workspace/generation scope captured for the attempt.
    pub fn scope(&self) -> &MappingScope {
        &self.scope
    }

    /// Canonical WebSocket endpoint from the validated mapping.
    pub fn relay_url(&self) -> &str {
        &self.relay_url
    }

    /// DNS name used for SNI and certificate hostname verification.
    pub fn relay_host(&self) -> &str {
        &self.relay_host
    }

    /// Community binding captured from the trusted mapping.
    pub fn community_id(&self) -> &str {
        &self.community_id
    }

    /// Relay public key pinned by trusted provisioning state.
    pub fn relay_pubkey(&self) -> &str {
        &self.relay_pubkey
    }

    /// Port from the validated WebSocket URL.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// DNS addresses approved for this connection attempt.
    pub fn pinned_ips(&self) -> &[IpAddr] {
        &self.pinned_ips
    }
}

/// Evidence returned by the actual transport after TLS and NIP-11 verification.
/// These booleans/identifiers must be populated by the transport, not the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectedRelayEvidence {
    pub scope: MappingScope,
    pub relay_url: String,
    pub tls_server_name: String,
    pub tls_certificate_and_hostname_verified: bool,
    pub peer_ip: IpAddr,
    pub peer_port: u16,
    pub community_id: String,
    pub relay_pubkey: String,
    pub endpoint_service_identity: String,
    pub tls_peer_spki_sha256: String,
}

/// Verify the completed connection against the mapping snapshot and current
/// trusted record. A mapping update during DNS/TLS/handshake invalidates it.
pub fn verify_connected_relay(
    attempt: &ConnectionAttempt,
    current: &TrustedRelayMapping,
    evidence: &ConnectedRelayEvidence,
    now: DateTime<Utc>,
) -> Result<(), MappingError> {
    if now >= attempt.valid_until || now >= current.record.valid_until {
        return Err(MappingError::InactiveOrStale);
    }
    if attempt.scope != current.scope() || attempt.mapping_snapshot != current.record {
        return Err(MappingError::MappingChangedDuringConnect);
    }
    if evidence.scope != attempt.scope {
        return Err(MappingError::ScopeMismatch);
    }
    if evidence.relay_url != attempt.relay_url {
        return Err(MappingError::RelayUrlSubstitution);
    }
    if evidence.tls_server_name != attempt.relay_host {
        return Err(MappingError::RelayHostMismatch);
    }
    if !evidence.tls_certificate_and_hostname_verified {
        return Err(MappingError::TlsVerificationFailed);
    }
    if evidence.peer_port != attempt.port
        || !attempt.pinned_ips.contains(&evidence.peer_ip)
        || !current.record.approved_ips.contains(&evidence.peer_ip)
        || is_unsafe_relay_ip(&evidence.peer_ip)
    {
        return Err(MappingError::PeerAddressMismatch);
    }
    if evidence.community_id != attempt.community_id
        || evidence.relay_pubkey != attempt.relay_pubkey
        || evidence.endpoint_service_identity != attempt.endpoint_service_identity
        || evidence.tls_peer_spki_sha256 != attempt.tls_peer_spki_sha256
        || evidence.community_id != current.record.community_id
        || evidence.relay_pubkey != current.record.relay_pubkey
        || evidence.endpoint_service_identity != current.record.endpoint_service_identity
        || evidence.tls_peer_spki_sha256 != current.record.tls_peer_spki_sha256
    {
        return Err(MappingError::RelayIdentityMismatch);
    }
    Ok(())
}

/// Validate an immutable, audited configuration update using compare-and-swap.
pub fn verify_mapping_update(
    current: &TrustedRelayMapping,
    next: RelayMappingRecord,
    expected_generation: u64,
    now: DateTime<Utc>,
) -> Result<TrustedRelayMapping, MappingError> {
    verify_mapping_revision(current, &next, expected_generation)?;
    TrustedRelayMapping::validate(next, now)
}

/// Check immutable identity and monotonic generation for any mapping revision.
/// Callers still validate the endpoint policy before making an active mapping usable.
pub fn verify_mapping_revision(
    current: &TrustedRelayMapping,
    next: &RelayMappingRecord,
    expected_generation: u64,
) -> Result<(), MappingError> {
    verify_mapping_revision_record(&current.record, next, expected_generation)
}

/// Check a revision when the current record is expired or inactive and cannot
/// be represented as a trusted, connectable mapping.
pub fn verify_mapping_revision_record(
    current: &RelayMappingRecord,
    next: &RelayMappingRecord,
    expected_generation: u64,
) -> Result<(), MappingError> {
    if current.generation != expected_generation {
        return Err(MappingError::StaleMappingGeneration);
    }
    if next.mapping_id != current.mapping_id
        || next.tenant_id != current.tenant_id
        || next.workspace_id != current.workspace_id
        || next.generation
            != expected_generation
                .checked_add(1)
                .ok_or(MappingError::StaleMappingGeneration)?
        || next.audit_entry_id == current.audit_entry_id
        || next.updated_by.is_nil()
    {
        return Err(MappingError::InvalidMappingUpdate);
    }
    Ok(())
}

/// Require all active workspaces in a tenant to use one isolated relay and
/// forbid any relay/community/host from being shared between tenants.
pub fn verify_tenant_mappings(mappings: &[TrustedRelayMapping]) -> Result<(), MappingError> {
    for (index, mapping) in mappings.iter().enumerate() {
        if mappings[..index].iter().any(|earlier| {
            earlier.record.mapping_id == mapping.record.mapping_id
                || (earlier.record.tenant_id == mapping.record.tenant_id
                    && (earlier.record.workspace_id == mapping.record.workspace_id
                        || earlier.canonical_url != mapping.canonical_url
                        || earlier.record.community_id != mapping.record.community_id
                        || earlier.record.relay_deployment_id
                            != mapping.record.relay_deployment_id
                        || earlier.record.endpoint_service_identity
                            != mapping.record.endpoint_service_identity
                        || earlier.record.tls_peer_spki_sha256
                            != mapping.record.tls_peer_spki_sha256
                        || earlier.record.relay_pubkey != mapping.record.relay_pubkey
                        || earlier.record.deployment_digest != mapping.record.deployment_digest
                        || earlier.record.approved_ips != mapping.record.approved_ips))
                || (earlier.record.tenant_id != mapping.record.tenant_id
                    && earlier.record.workspace_id == mapping.record.workspace_id)
        }) {
            if mappings[..index]
                .iter()
                .any(|earlier| earlier.record.mapping_id == mapping.record.mapping_id)
            {
                return Err(MappingError::DuplicateMappingId);
            }
            if mappings[..index].iter().any(|earlier| {
                earlier.record.tenant_id != mapping.record.tenant_id
                    && earlier.record.workspace_id == mapping.record.workspace_id
            }) {
                return Err(MappingError::ConflictingMapping);
            }
            return Err(MappingError::ConflictingMapping);
        }
        if mappings[..index].iter().any(|earlier| {
            earlier.record.tenant_id != mapping.record.tenant_id
                && (earlier.canonical_url == mapping.canonical_url
                    || earlier.record.community_id == mapping.record.community_id
                    || earlier.record.relay_host == mapping.record.relay_host
                    || earlier.record.relay_pubkey == mapping.record.relay_pubkey
                    || earlier.record.relay_deployment_id == mapping.record.relay_deployment_id
                    || earlier.record.endpoint_service_identity
                        == mapping.record.endpoint_service_identity)
        }) {
            return Err(MappingError::CrossTenantRelayReuse);
        }
    }
    Ok(())
}

/// Conservative deny policy for internal and special-use addresses. IPv6 is
/// accepted only from global-unicast space; special-use subranges remain denied.
fn is_unsafe_relay_ip(ip: &IpAddr) -> bool {
    rustshare_core::validation::is_internal_ip(ip)
        || matches!(ip, IpAddr::V6(ipv6) if ipv6.segments()[0] & 0xe000 != 0x2000)
        || SPECIAL_USE_IP_RANGES.iter().any(|range| range.contains(ip))
}

static SPECIAL_USE_IP_RANGES: LazyLock<Vec<IpNet>> = LazyLock::new(|| {
    include_str!("upstream_special_use_ip_ranges.txt")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.parse().expect("checked-in IANA special-use CIDR"))
        .collect()
});

/// Resolve exactly one trusted mapping for a caller's tenant/workspace.
/// Missing or duplicate records never fall back to another tenant or relay.
pub fn select_mapping(
    mappings: &[TrustedRelayMapping],
    tenant_id: TenantId,
    workspace_id: WorkspaceId,
) -> Result<&TrustedRelayMapping, MappingError> {
    let mut matches = mappings.iter().filter(|mapping| {
        mapping.record.tenant_id == tenant_id && mapping.record.workspace_id == workspace_id
    });
    let selected = matches.next().ok_or(MappingError::MissingMapping)?;
    if matches.next().is_some() {
        return Err(MappingError::AmbiguousMapping);
    }
    Ok(selected)
}

fn canonical_relay_url(host: &str, explicit_port: Option<u16>) -> String {
    match explicit_port.filter(|port| *port != 443) {
        Some(port) => format!("wss://{host}:{port}"),
        None => format!("wss://{host}"),
    }
}

fn is_sha256_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

fn is_canonical_sha256_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    })
}

fn has_duplicates(values: &[IpAddr]) -> bool {
    values
        .iter()
        .enumerate()
        .any(|(index, value)| values[..index].contains(value))
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum MappingError {
    #[error("relay mapping is missing, inactive, or stale")]
    InactiveOrStale,
    #[error("community ID is invalid")]
    InvalidCommunity,
    #[error("relay URL must be canonical wss with no credentials, path, query, or fragment")]
    InvalidRelayUrl,
    #[error("relay URL host does not match the trusted host")]
    RelayHostMismatch,
    #[error("relay identity is not a valid pinned Nostr public key")]
    InvalidRelayIdentity,
    #[error("deployment provenance must be a SHA-256 digest")]
    InvalidDeploymentProvenance,
    #[error("DNS targets are empty, duplicated, internal, or not operator-approved")]
    UnsafeDnsTargets,
    #[error("tenant/workspace scope does not match this mapping")]
    ScopeMismatch,
    #[error("mapping generation changed while connecting")]
    MappingChangedDuringConnect,
    #[error("mapping generation is stale")]
    StaleMappingGeneration,
    #[error("mapping update is not sequential, immutable, and audited")]
    InvalidMappingUpdate,
    #[error("TLS certificate chain or hostname verification failed")]
    TlsVerificationFailed,
    #[error("connected peer does not match the pinned DNS targets and port")]
    PeerAddressMismatch,
    #[error("relay URL substitution is forbidden")]
    RelayUrlSubstitution,
    #[error("relay community or pinned relay identity mismatch")]
    RelayIdentityMismatch,
    #[error("tenant has conflicting active mappings")]
    ConflictingMapping,
    #[error("tenant/workspace relay mapping is missing")]
    MissingMapping,
    #[error("tenant/workspace resolves to multiple relay mappings")]
    AmbiguousMapping,
    #[error("mapping identity is reused")]
    DuplicateMappingId,
    #[error("relay host, community, or URL is shared across tenants")]
    CrossTenantRelayReuse,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_record(
        tenant_id: TenantId,
        workspace_id: WorkspaceId,
        host: &str,
    ) -> RelayMappingRecord {
        RelayMappingRecord {
            mapping_id: Uuid::new_v4(),
            tenant_id,
            workspace_id,
            community_id: Uuid::new_v4().to_string(),
            relay_deployment_id: Uuid::new_v4(),
            endpoint_service_identity: format!(
                "cluster/test-ns/buzz-{tenant_id}#{}",
                Uuid::new_v4()
            ),
            tls_peer_spki_sha256: format!("sha256:{}", "b2".repeat(32)),
            relay_url: format!("wss://{host}"),
            relay_host: host.to_string(),
            relay_pubkey: "22".repeat(32),
            deployment_digest: format!("sha256:{}", "ab".repeat(32)),
            approved_ips: vec!["8.8.8.8".parse().unwrap()],
            generation: 1,
            active: true,
            valid_until: Utc::now() + chrono::Duration::hours(1),
            updated_by: Uuid::new_v4(),
            audit_entry_id: Uuid::new_v4(),
        }
    }

    fn ids() -> (TenantId, WorkspaceId) {
        (TenantId(Uuid::new_v4()), WorkspaceId(Uuid::new_v4()))
    }

    fn valid_connection(mapping: &TrustedRelayMapping, ip: IpAddr) -> ConnectedRelayEvidence {
        ConnectedRelayEvidence {
            scope: mapping.scope(),
            relay_url: mapping.relay_url().to_string(),
            tls_server_name: mapping.record.relay_host.clone(),
            tls_certificate_and_hostname_verified: true,
            peer_ip: ip,
            peer_port: 443,
            community_id: mapping.record.community_id.clone(),
            relay_pubkey: mapping.record.relay_pubkey.clone(),
            endpoint_service_identity: mapping.record.endpoint_service_identity.clone(),
            tls_peer_spki_sha256: mapping.record.tls_peer_spki_sha256.clone(),
        }
    }

    #[test]
    fn mapping_rejects_host_substitution() {
        let (tenant, workspace) = ids();
        let mut record = valid_record(tenant, workspace, "relay.tenant.example");
        record.relay_host = "other.tenant.example".into();
        assert_eq!(
            TrustedRelayMapping::validate(record, Utc::now()).unwrap_err(),
            MappingError::RelayHostMismatch
        );
    }

    #[test]
    fn mapping_rejects_userinfo_non_wss_paths_and_ip_literals() {
        let (tenant, workspace) = ids();
        for url in [
            "ws://relay.tenant.example",
            "wss://user@relay.tenant.example",
            "wss://relay.tenant.example/path",
            "wss://127.0.0.1",
        ] {
            let mut record = valid_record(tenant, workspace, "relay.tenant.example");
            record.relay_url = url.into();
            assert!(TrustedRelayMapping::validate(record, Utc::now()).is_err());
        }
        let mut noncanonical = valid_record(tenant, workspace, "relay.tenant.example");
        noncanonical.relay_url = "wss://RELAY.tenant.example:443".into();
        assert_eq!(
            TrustedRelayMapping::validate(noncanonical, Utc::now()).unwrap_err(),
            MappingError::InvalidRelayUrl
        );
    }

    #[test]
    fn mapping_selection_fails_closed_for_missing_or_ambiguous_scope() {
        let (tenant, workspace) = ids();
        let mapping = TrustedRelayMapping::validate(
            valid_record(tenant, workspace, "relay.tenant.example"),
            Utc::now(),
        )
        .unwrap();
        assert_eq!(
            select_mapping(&[], tenant, workspace).unwrap_err(),
            MappingError::MissingMapping
        );
        assert_eq!(
            select_mapping(&[mapping.clone(), mapping.clone()], tenant, workspace).unwrap_err(),
            MappingError::AmbiguousMapping
        );
        assert_eq!(
            select_mapping(
                std::slice::from_ref(&mapping),
                TenantId(Uuid::new_v4()),
                workspace
            )
            .unwrap_err(),
            MappingError::MissingMapping
        );
        assert!(select_mapping(std::slice::from_ref(&mapping), tenant, workspace).is_ok());
    }

    #[test]
    fn mapping_rejects_internal_or_duplicated_dns_targets() {
        let (tenant, workspace) = ids();
        for ips in [
            vec!["127.0.0.1".parse().unwrap()],
            vec!["169.254.169.254".parse().unwrap()],
            vec!["8.8.8.8".parse().unwrap(), "8.8.8.8".parse().unwrap()],
            vec!["192.0.2.1".parse().unwrap()],
            vec!["198.18.0.1".parse().unwrap()],
            vec!["192.31.196.1".parse().unwrap()],
            vec!["192.52.193.1".parse().unwrap()],
            vec!["192.175.48.1".parse().unwrap()],
            vec!["2001:db8::1".parse().unwrap()],
            vec!["2001:2::1".parse().unwrap()],
            vec!["3fff::1".parse().unwrap()],
            vec!["2001::1".parse().unwrap()],
        ] {
            let mut record = valid_record(tenant, workspace, "relay.tenant.example");
            record.approved_ips = ips;
            assert_eq!(
                TrustedRelayMapping::validate(record, Utc::now()).unwrap_err(),
                MappingError::UnsafeDnsTargets
            );
        }
    }

    #[test]
    fn connection_rejects_dns_rebinding_to_unapproved_target() {
        let (tenant, workspace) = ids();
        let mapping = TrustedRelayMapping::validate(
            valid_record(tenant, workspace, "relay.tenant.example"),
            Utc::now(),
        )
        .unwrap();
        assert_eq!(
            mapping
                .begin_connection(
                    mapping.scope(),
                    &["169.254.169.254".parse().unwrap()],
                    Utc::now()
                )
                .unwrap_err(),
            MappingError::UnsafeDnsTargets
        );
    }

    #[test]
    fn connection_rejects_wrong_tenant_workspace_scope() {
        let (tenant, workspace) = ids();
        let mapping = TrustedRelayMapping::validate(
            valid_record(tenant, workspace, "relay.tenant.example"),
            Utc::now(),
        )
        .unwrap();
        let (other_tenant, other_workspace) = ids();
        assert_eq!(
            mapping
                .begin_connection(
                    MappingScope {
                        tenant_id: other_tenant,
                        workspace_id: other_workspace,
                        ..mapping.scope()
                    },
                    &["8.8.8.8".parse().unwrap()],
                    Utc::now()
                )
                .unwrap_err(),
            MappingError::ScopeMismatch
        );
    }

    #[test]
    fn connection_rejects_wrong_tls_host_or_unverified_certificate() {
        let (tenant, workspace) = ids();
        let mapping = TrustedRelayMapping::validate(
            valid_record(tenant, workspace, "relay.tenant.example"),
            Utc::now(),
        )
        .unwrap();
        let attempt = mapping
            .begin_connection(mapping.scope(), &["8.8.8.8".parse().unwrap()], Utc::now())
            .unwrap();
        let mut evidence = valid_connection(&mapping, "8.8.8.8".parse().unwrap());
        evidence.tls_server_name = "wrong.tenant.example".into();
        assert_eq!(
            verify_connected_relay(&attempt, &mapping, &evidence, Utc::now()).unwrap_err(),
            MappingError::RelayHostMismatch
        );
        evidence.tls_server_name = "relay.tenant.example".into();
        evidence.tls_certificate_and_hostname_verified = false;
        assert_eq!(
            verify_connected_relay(&attempt, &mapping, &evidence, Utc::now()).unwrap_err(),
            MappingError::TlsVerificationFailed
        );
    }

    #[test]
    fn connection_rejects_wrong_relay_identity_and_community() {
        let (tenant, workspace) = ids();
        let mapping = TrustedRelayMapping::validate(
            valid_record(tenant, workspace, "relay.tenant.example"),
            Utc::now(),
        )
        .unwrap();
        let attempt = mapping
            .begin_connection(mapping.scope(), &["8.8.8.8".parse().unwrap()], Utc::now())
            .unwrap();
        let mut evidence = valid_connection(&mapping, "8.8.8.8".parse().unwrap());
        evidence.relay_pubkey = "33".repeat(32);
        assert_eq!(
            verify_connected_relay(&attempt, &mapping, &evidence, Utc::now()).unwrap_err(),
            MappingError::RelayIdentityMismatch
        );
        evidence.relay_pubkey = mapping.record.relay_pubkey.clone();
        evidence.community_id = "other-community".into();
        assert_eq!(
            verify_connected_relay(&attempt, &mapping, &evidence, Utc::now()).unwrap_err(),
            MappingError::RelayIdentityMismatch
        );
    }

    #[test]
    fn connection_rejects_mapping_mutation_without_generation_change() {
        let (tenant, workspace) = ids();
        let mapping = TrustedRelayMapping::validate(
            valid_record(tenant, workspace, "relay.tenant.example"),
            Utc::now(),
        )
        .unwrap();
        let attempt = mapping
            .begin_connection(mapping.scope(), &["8.8.8.8".parse().unwrap()], Utc::now())
            .unwrap();
        let evidence = valid_connection(&mapping, "8.8.8.8".parse().unwrap());
        let mut changed_record = mapping.record.clone();
        changed_record.relay_url = "wss://other.tenant.example".into();
        changed_record.relay_host = "other.tenant.example".into();
        let current = TrustedRelayMapping::validate(changed_record, Utc::now()).unwrap();

        assert_eq!(
            verify_connected_relay(&attempt, &current, &evidence, Utc::now()).unwrap_err(),
            MappingError::MappingChangedDuringConnect
        );
    }

    #[test]
    fn connection_rejects_relay_url_substitution_and_unpinned_peer() {
        let (tenant, workspace) = ids();
        let mapping = TrustedRelayMapping::validate(
            valid_record(tenant, workspace, "relay.tenant.example"),
            Utc::now(),
        )
        .unwrap();
        let attempt = mapping
            .begin_connection(mapping.scope(), &["8.8.8.8".parse().unwrap()], Utc::now())
            .unwrap();
        let mut evidence = valid_connection(&mapping, "8.8.8.8".parse().unwrap());
        evidence.relay_url = "wss://attacker.example".into();
        assert_eq!(
            verify_connected_relay(&attempt, &mapping, &evidence, Utc::now()).unwrap_err(),
            MappingError::RelayUrlSubstitution
        );
        evidence.relay_url = mapping.relay_url().into();
        evidence.peer_ip = "1.1.1.1".parse().unwrap();
        assert_eq!(
            verify_connected_relay(&attempt, &mapping, &evidence, Utc::now()).unwrap_err(),
            MappingError::PeerAddressMismatch
        );
    }

    #[test]
    fn connection_rejects_mapping_change_during_handshake() {
        let (tenant, workspace) = ids();
        let original = valid_record(tenant, workspace, "relay.tenant.example");
        let mapping = TrustedRelayMapping::validate(original.clone(), Utc::now()).unwrap();
        let attempt = mapping
            .begin_connection(mapping.scope(), &["8.8.8.8".parse().unwrap()], Utc::now())
            .unwrap();
        let mut updated = original;
        updated.generation = 2;
        updated.audit_entry_id = Uuid::new_v4();
        let current = TrustedRelayMapping::validate(updated, Utc::now()).unwrap();
        let evidence = valid_connection(&mapping, "8.8.8.8".parse().unwrap());
        assert_eq!(
            verify_connected_relay(&attempt, &current, &evidence, Utc::now()).unwrap_err(),
            MappingError::MappingChangedDuringConnect
        );
    }

    #[test]
    fn mapping_updates_require_sequential_audited_generation() {
        let (tenant, workspace) = ids();
        let current = TrustedRelayMapping::validate(
            valid_record(tenant, workspace, "relay.tenant.example"),
            Utc::now(),
        )
        .unwrap();
        let mut next = current.record.clone();
        next.generation = 2;
        next.audit_entry_id = Uuid::new_v4();
        assert!(verify_mapping_update(&current, next.clone(), 1, Utc::now()).is_ok());
        assert_eq!(
            verify_mapping_update(&current, next.clone(), 0, Utc::now()).unwrap_err(),
            MappingError::StaleMappingGeneration
        );
        next.audit_entry_id = current.record.audit_entry_id;
        assert_eq!(
            verify_mapping_update(&current, next, 1, Utc::now()).unwrap_err(),
            MappingError::InvalidMappingUpdate
        );
    }

    #[test]
    fn tenant_mapping_set_forbids_conflicts_and_cross_tenant_reuse() {
        let (tenant_a, workspace_a) = ids();
        let (_, workspace_b) = ids();
        let (tenant_b, workspace_c) = ids();
        let first = TrustedRelayMapping::validate(
            valid_record(tenant_a, workspace_a, "a.tenant.example"),
            Utc::now(),
        )
        .unwrap();
        let mut same_tenant = valid_record(tenant_a, workspace_b, "a.tenant.example");
        same_tenant.community_id = first.record.community_id.clone();
        same_tenant.relay_deployment_id = first.record.relay_deployment_id;
        same_tenant.endpoint_service_identity = first.record.endpoint_service_identity.clone();
        same_tenant.tls_peer_spki_sha256 = first.record.tls_peer_spki_sha256.clone();
        same_tenant.relay_pubkey = first.record.relay_pubkey.clone();
        same_tenant.approved_ips = first.record.approved_ips.clone();
        let same_tenant = TrustedRelayMapping::validate(same_tenant, Utc::now()).unwrap();
        assert!(verify_tenant_mappings(&[first.clone(), same_tenant]).is_ok());
        let reused = TrustedRelayMapping::validate(
            valid_record(tenant_b, workspace_c, "a.tenant.example"),
            Utc::now(),
        )
        .unwrap();
        assert_eq!(
            verify_tenant_mappings(&[first, reused]).unwrap_err(),
            MappingError::CrossTenantRelayReuse
        );

        let (_, workspace_d) = ids();
        let mut same_key = valid_record(tenant_b, workspace_d, "other.tenant.example");
        same_key.community_id = Uuid::new_v4().to_string();
        same_key.relay_pubkey = "22".repeat(32);
        let same_key = TrustedRelayMapping::validate(same_key, Utc::now()).unwrap();
        let (tenant_c, workspace_e) = ids();
        let mut first_identity = valid_record(tenant_c, workspace_e, "third.tenant.example");
        first_identity.relay_pubkey = "22".repeat(32);
        let first_identity = TrustedRelayMapping::validate(first_identity, Utc::now()).unwrap();
        assert_eq!(
            verify_tenant_mappings(&[first_identity, same_key]).unwrap_err(),
            MappingError::CrossTenantRelayReuse
        );
    }

    #[test]
    fn same_tenant_workspaces_must_match_provenance_and_dns_policy() {
        let (tenant, workspace_a) = ids();
        let (_, workspace_b) = ids();
        let first = TrustedRelayMapping::validate(
            valid_record(tenant, workspace_a, "relay.tenant.example"),
            Utc::now(),
        )
        .unwrap();
        let mut other = valid_record(tenant, workspace_b, "relay.tenant.example");
        other.community_id = first.record.community_id.clone();
        other.relay_pubkey = first.record.relay_pubkey.clone();
        other.deployment_digest = format!("sha256:{}", "cd".repeat(32));
        let other = TrustedRelayMapping::validate(other, Utc::now()).unwrap();
        assert_eq!(
            verify_tenant_mappings(&[first, other]).unwrap_err(),
            MappingError::ConflictingMapping
        );
    }
}
