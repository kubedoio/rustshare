//! Feature-gated verification of trusted operator mapping statements.

use std::collections::HashMap;
use std::net::IpAddr;

use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::upstream_relay_mapping::{MappingError, RelayMappingRecord, TrustedRelayMapping};

const DOMAIN: &[u8] = b"rustshare/upstream-buzz/mapping-provision/v1\0";
const MAX_STATEMENT_AGE: Duration = Duration::minutes(5);
const MAX_FUTURE_SKEW: Duration = Duration::seconds(30);
const MAX_APPROVED_IPS: usize = 64;
const MAX_SIGNED_STRING: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MappingProvisioningAction {
    Create = 1,
    BindWorkspace = 2,
    Update = 3,
    Renew = 4,
    Deactivate = 5,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingProvisioningStatement {
    pub operator_service_id: Uuid,
    pub operator_key_id: String,
    pub operation_id: Uuid,
    pub change_reference_id: Uuid,
    pub action: MappingProvisioningAction,
    pub expected_generation: u64,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub record: RelayMappingRecord,
    pub signature: Vec<u8>,
}

impl MappingProvisioningStatement {
    /// Deterministic, domain-separated bytes the deployment signer must sign.
    pub fn signing_bytes(&self) -> Result<Vec<u8>, MappingProvisioningError> {
        let mut out = Vec::with_capacity(1024);
        out.extend_from_slice(DOMAIN);
        out.extend_from_slice(self.operator_service_id.as_bytes());
        put_string(&mut out, &self.operator_key_id)?;
        out.extend_from_slice(self.operation_id.as_bytes());
        out.extend_from_slice(self.change_reference_id.as_bytes());
        out.push(self.action as u8);
        out.extend_from_slice(&self.expected_generation.to_be_bytes());
        out.extend_from_slice(&self.issued_at.timestamp_micros().to_be_bytes());
        out.extend_from_slice(&self.expires_at.timestamp_micros().to_be_bytes());

        let record = &self.record;
        out.extend_from_slice(record.mapping_id.as_bytes());
        out.extend_from_slice(record.tenant_id.0.as_bytes());
        out.extend_from_slice(record.workspace_id.0.as_bytes());
        put_string(&mut out, &record.community_id)?;
        out.push(record.endpoint_policy as u8);
        out.extend_from_slice(record.relay_deployment_id.as_bytes());
        put_string(&mut out, &record.endpoint_service_identity)?;
        put_string(&mut out, &record.tls_peer_spki_sha256)?;
        put_string(&mut out, &record.relay_url)?;
        put_string(&mut out, &record.relay_host)?;
        put_string(&mut out, &record.relay_pubkey)?;
        put_string(&mut out, &record.deployment_digest)?;
        put_count(&mut out, record.approved_ips.len())?;
        for ip in &record.approved_ips {
            match ip {
                IpAddr::V4(ip) => {
                    out.push(4);
                    out.extend_from_slice(&ip.octets());
                }
                IpAddr::V6(ip) => {
                    out.push(6);
                    out.extend_from_slice(&ip.octets());
                }
            }
        }
        out.extend_from_slice(&record.generation.to_be_bytes());
        out.push(u8::from(record.active));
        out.extend_from_slice(&record.valid_until.timestamp_micros().to_be_bytes());
        out.extend_from_slice(record.updated_by.as_bytes());
        out.extend_from_slice(record.audit_entry_id.as_bytes());
        Ok(out)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MappingProvisioningError {
    #[error("mapping provisioning statement is invalid")]
    InvalidStatement,
    #[error("mapping provisioning signer is not trusted")]
    UntrustedSigner,
    #[error("mapping provisioning signature is invalid")]
    InvalidSignature,
    #[error("mapping provisioning statement is expired or not yet valid")]
    Expired,
    #[error(transparent)]
    Mapping(#[from] MappingError),
}

/// Trust configuration comes from deployment-owned configuration, never a request.
pub struct MappingProvisioningVerifier {
    operator_service_id: Uuid,
    keys: HashMap<String, VerifyingKey>,
}

impl MappingProvisioningVerifier {
    pub fn new(
        operator_service_id: Uuid,
        keys: HashMap<String, VerifyingKey>,
    ) -> Result<Self, MappingProvisioningError> {
        if operator_service_id.is_nil()
            || keys.iter().any(|(id, _)| {
                id.is_empty() || id.len() > 128 || !id.bytes().all(|byte| byte.is_ascii_graphic())
            })
        {
            return Err(MappingProvisioningError::InvalidStatement);
        }
        Ok(Self {
            operator_service_id,
            keys,
        })
    }

    pub fn verify(
        &self,
        statement: &MappingProvisioningStatement,
        now: DateTime<Utc>,
    ) -> Result<VerifiedMappingProvisioning, MappingProvisioningError> {
        if statement.operator_service_id != self.operator_service_id
            || statement.record.updated_by != self.operator_service_id
            || statement.operation_id.is_nil()
            || statement.change_reference_id.is_nil()
            || statement.expected_generation > i64::MAX as u64
            || statement.record.audit_entry_id.is_nil()
            || statement.record.tenant_id.0.is_nil()
            || statement.record.workspace_id.0.is_nil()
            || statement.record.mapping_id.is_nil()
            || !statement
                .issued_at
                .timestamp_subsec_nanos()
                .is_multiple_of(1_000)
            || !statement
                .expires_at
                .timestamp_subsec_nanos()
                .is_multiple_of(1_000)
            || !statement
                .record
                .valid_until
                .timestamp_subsec_nanos()
                .is_multiple_of(1_000)
        {
            return Err(MappingProvisioningError::InvalidStatement);
        }
        if statement.issued_at > now + MAX_FUTURE_SKEW
            || statement.expires_at <= now
            || statement.expires_at <= statement.issued_at
            || statement
                .expires_at
                .timestamp_micros()
                .saturating_sub(statement.issued_at.timestamp_micros())
                > MAX_STATEMENT_AGE.num_microseconds().unwrap_or(i64::MAX)
        {
            return Err(MappingProvisioningError::Expired);
        }
        let key = self
            .keys
            .get(&statement.operator_key_id)
            .ok_or(MappingProvisioningError::UntrustedSigner)?;
        let signature = Signature::from_slice(&statement.signature)
            .map_err(|_| MappingProvisioningError::InvalidSignature)?;
        let signing_bytes = statement.signing_bytes()?;
        key.verify_strict(&signing_bytes, &signature)
            .map_err(|_| MappingProvisioningError::InvalidSignature)?;
        verify_generation_action(statement)?;
        validate_record(statement, now)?;
        let statement_digest = Sha256::digest(&signing_bytes).into();

        Ok(VerifiedMappingProvisioning {
            statement: statement.clone(),
            statement_digest,
        })
    }
}

#[derive(Debug, Clone)]
pub struct VerifiedMappingProvisioning {
    statement: MappingProvisioningStatement,
    statement_digest: [u8; 32],
}

impl VerifiedMappingProvisioning {
    pub fn action(&self) -> MappingProvisioningAction {
        self.statement.action
    }

    pub fn record(&self) -> &RelayMappingRecord {
        &self.statement.record
    }

    pub fn expected_generation(&self) -> u64 {
        self.statement.expected_generation
    }

    pub fn operation_id(&self) -> Uuid {
        self.statement.operation_id
    }

    pub fn change_reference_id(&self) -> Uuid {
        self.statement.change_reference_id
    }

    pub fn operator_service_id(&self) -> Uuid {
        self.statement.operator_service_id
    }

    pub fn operator_key_id(&self) -> &str {
        &self.statement.operator_key_id
    }

    pub fn statement_digest(&self) -> &[u8; 32] {
        &self.statement_digest
    }
}

fn verify_generation_action(
    statement: &MappingProvisioningStatement,
) -> Result<(), MappingProvisioningError> {
    let record = &statement.record;
    let valid = match statement.action {
        MappingProvisioningAction::Create => {
            statement.expected_generation == 0 && record.generation == 1 && record.active
        }
        MappingProvisioningAction::BindWorkspace => {
            statement.expected_generation == record.generation && record.active
        }
        MappingProvisioningAction::Update | MappingProvisioningAction::Renew => {
            statement.expected_generation.checked_add(1) == Some(record.generation) && record.active
        }
        MappingProvisioningAction::Deactivate => {
            statement.expected_generation.checked_add(1) == Some(record.generation)
                && !record.active
        }
    };
    valid
        .then_some(())
        .ok_or(MappingProvisioningError::InvalidStatement)
}

fn validate_record(
    statement: &MappingProvisioningStatement,
    now: DateTime<Utc>,
) -> Result<(), MappingProvisioningError> {
    if statement.action == MappingProvisioningAction::Deactivate {
        TrustedRelayMapping::validate_record_structure(&statement.record)?;
        Ok(())
    } else {
        TrustedRelayMapping::validate(statement.record.clone(), now)?;
        Ok(())
    }
}

fn put_string(out: &mut Vec<u8>, value: &str) -> Result<(), MappingProvisioningError> {
    if value.len() > MAX_SIGNED_STRING {
        return Err(MappingProvisioningError::InvalidStatement);
    }
    let length =
        u32::try_from(value.len()).map_err(|_| MappingProvisioningError::InvalidStatement)?;
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(value.as_bytes());
    Ok(())
}

fn put_count(out: &mut Vec<u8>, value: usize) -> Result<(), MappingProvisioningError> {
    if value > MAX_APPROVED_IPS {
        return Err(MappingProvisioningError::InvalidStatement);
    }
    let value = u32::try_from(value).map_err(|_| MappingProvisioningError::InvalidStatement)?;
    out.extend_from_slice(&value.to_be_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::upstream_relay_mapping::RelayEndpointPolicy;
    use ed25519_dalek::{Signer, SigningKey};
    use rustshare_core::domain::{TenantId, WorkspaceId};

    const NOW: i64 = 1_800_000_000;

    fn signed(
        action: MappingProvisioningAction,
    ) -> (
        MappingProvisioningVerifier,
        MappingProvisioningStatement,
        DateTime<Utc>,
    ) {
        let now = DateTime::from_timestamp(NOW, 0).unwrap();
        let signing_key = SigningKey::from_bytes(&[7; 32]);
        let operator = Uuid::new_v4();
        let mut record = RelayMappingRecord {
            mapping_id: Uuid::new_v4(),
            tenant_id: TenantId(Uuid::new_v4()),
            workspace_id: WorkspaceId(Uuid::new_v4()),
            community_id: Uuid::new_v4().to_string(),
            endpoint_policy: RelayEndpointPolicy::StrictPublic,
            relay_deployment_id: Uuid::new_v4(),
            endpoint_service_identity: "cluster/ns/buzz#service-uid".into(),
            tls_peer_spki_sha256: format!("sha256:{}", "ab".repeat(32)),
            relay_url: "wss://relay.example.test".into(),
            relay_host: "relay.example.test".into(),
            relay_pubkey: nostr::Keys::generate().public_key().to_hex(),
            deployment_digest: format!("sha256:{}", "cd".repeat(32)),
            approved_ips: vec!["8.8.8.8".parse().unwrap()],
            generation: 1,
            active: true,
            valid_until: now + Duration::hours(1),
            updated_by: operator,
            audit_entry_id: Uuid::new_v4(),
        };
        let expected_generation = match action {
            MappingProvisioningAction::Create => 0,
            MappingProvisioningAction::BindWorkspace => 1,
            _ => {
                record.generation = 2;
                if action == MappingProvisioningAction::Deactivate {
                    record.active = false;
                }
                1
            }
        };
        let mut statement = MappingProvisioningStatement {
            operator_service_id: operator,
            operator_key_id: "operator-2026-01".into(),
            operation_id: Uuid::new_v4(),
            change_reference_id: Uuid::new_v4(),
            action,
            expected_generation,
            issued_at: now,
            expires_at: now + Duration::minutes(2),
            record,
            signature: vec![],
        };
        statement.signature = signing_key
            .sign(&statement.signing_bytes().unwrap())
            .to_bytes()
            .to_vec();
        let verifier = MappingProvisioningVerifier::new(
            operator,
            HashMap::from([("operator-2026-01".into(), signing_key.verifying_key())]),
        )
        .unwrap();
        (verifier, statement, now)
    }

    #[test]
    fn verifies_operator_signed_statement() {
        let (verifier, statement, now) = signed(MappingProvisioningAction::Create);
        assert!(verifier.verify(&statement, now).is_ok());
    }

    #[test]
    fn rejects_statement_field_substitution() {
        let (verifier, mut statement, now) = signed(MappingProvisioningAction::Create);
        statement.record.relay_host = "attacker.example.test".into();
        assert_eq!(
            verifier.verify(&statement, now).unwrap_err(),
            MappingProvisioningError::InvalidSignature
        );
    }

    #[test]
    fn trusted_service_policy_is_signed_and_bound_to_exact_kubernetes_service() {
        let (verifier, mut statement, now) = signed(MappingProvisioningAction::Create);
        let cluster = Uuid::new_v4();
        let service_uid = Uuid::new_v4();
        statement.record.endpoint_policy = RelayEndpointPolicy::TrustedService;
        statement.record.endpoint_service_identity =
            format!("k8s://{cluster}/tenant-a/buzz/{service_uid}");
        statement.record.relay_host = "buzz.tenant-a.svc.cluster.local".into();
        statement.record.relay_url = format!("wss://{}", statement.record.relay_host);
        statement.record.approved_ips = vec!["10.96.32.10".parse().unwrap()];
        let signer = SigningKey::from_bytes(&[7; 32]);
        statement.signature = signer
            .sign(&statement.signing_bytes().unwrap())
            .to_bytes()
            .to_vec();
        assert!(verifier.verify(&statement, now).is_ok());

        statement.record.relay_host = "buzz.other-tenant.svc.cluster.local".into();
        statement.record.relay_url = format!("wss://{}", statement.record.relay_host);
        statement.signature = signer
            .sign(&statement.signing_bytes().unwrap())
            .to_bytes()
            .to_vec();
        assert_eq!(
            verifier.verify(&statement, now).unwrap_err(),
            MappingProvisioningError::Mapping(MappingError::InvalidDeploymentProvenance)
        );
    }

    #[test]
    fn revoked_operator_key_is_rejected_by_reloaded_trust_configuration() {
        let (verifier, statement, now) = signed(MappingProvisioningAction::Create);
        assert!(verifier.verify(&statement, now).is_ok());

        // Trust is deployment configuration: removing the key and rebuilding
        // the verifier revokes still-valid statements signed by that key.
        let revoked =
            MappingProvisioningVerifier::new(statement.operator_service_id, HashMap::new())
                .unwrap();
        assert_eq!(
            revoked.verify(&statement, now).unwrap_err(),
            MappingProvisioningError::UntrustedSigner
        );
    }

    #[test]
    fn rejects_expired_or_overlong_statement() {
        let (verifier, statement, now) = signed(MappingProvisioningAction::Create);
        assert_eq!(
            verifier
                .verify(&statement, now + Duration::minutes(3))
                .unwrap_err(),
            MappingProvisioningError::Expired
        );
    }

    #[test]
    fn rejects_generation_action_mismatch() {
        let (verifier, mut statement, now) = signed(MappingProvisioningAction::Update);
        statement.record.generation = 3;
        statement.signature = SigningKey::from_bytes(&[7; 32])
            .sign(&statement.signing_bytes().unwrap())
            .to_bytes()
            .to_vec();
        assert_eq!(
            verifier.verify(&statement, now).unwrap_err(),
            MappingProvisioningError::InvalidStatement
        );
    }

    #[test]
    fn rejects_wrong_operator_identity() {
        let (verifier, mut statement, now) = signed(MappingProvisioningAction::Create);
        statement.operator_service_id = Uuid::new_v4();
        assert_eq!(
            verifier.verify(&statement, now).unwrap_err(),
            MappingProvisioningError::InvalidStatement
        );
    }
}
