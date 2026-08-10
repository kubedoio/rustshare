//! Tenant-scoped Elembra Principal ↔ Buzz identity and admission contracts.
//!
//! Buzz remains authoritative for signed events. This module only proves key
//! possession and evaluates Elembra's separate admission boundary.

use chrono::{DateTime, Duration, Utc};
use nostr::{Event, EventBuilder, Keys, Kind, PublicKey, Tag, TagKind, Timestamp};
use rustshare_core::domain::{PrincipalId, TenantId, WorkspaceId};
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

const NIP42_TOLERANCE_SECS: u64 = 60;
pub const BUZZ_RELAY_ADD_MEMBER_KIND: u16 = 9030;
pub const BUZZ_RELAY_REMOVE_MEMBER_KIND: u16 = 9031;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuzzAdmissionOperation {
    Admit,
    Revoke,
}

impl BuzzAdmissionOperation {
    /// NIP-43 command kind. The bridge signs the resulting event with its
    /// separately provisioned Buzz service identity, never a human binding key.
    pub const fn buzz_kind(self) -> u16 {
        match self {
            Self::Admit => BUZZ_RELAY_ADD_MEMBER_KIND,
            Self::Revoke => BUZZ_RELAY_REMOVE_MEMBER_KIND,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BuzzAdmissionCommandError {
    #[error("invalid Buzz public key: {0}")]
    InvalidPubkey(String),
    #[error("failed to sign Buzz admission command: {0}")]
    Signing(String),
}

/// Build the NIP-43 relay membership command used by the external Buzz bridge.
/// `bridge_keys` is the separately provisioned service/admin identity; a
/// human's sovereign signing key is never needed here.
pub fn build_buzz_admission_event(
    operation: BuzzAdmissionOperation,
    buzz_pubkey: &str,
    bridge_keys: &Keys,
) -> Result<Event, BuzzAdmissionCommandError> {
    build_buzz_admission_event_at(operation, buzz_pubkey, bridge_keys, Timestamp::now())
}

/// Deterministic variant for an outbox consumer: using the source event time
/// makes retries reproduce the same Nostr event id for the same operation.
pub fn build_buzz_admission_event_at(
    operation: BuzzAdmissionOperation,
    buzz_pubkey: &str,
    bridge_keys: &Keys,
    created_at: Timestamp,
) -> Result<Event, BuzzAdmissionCommandError> {
    let pubkey = PublicKey::from_hex(buzz_pubkey)
        .map_err(|error| BuzzAdmissionCommandError::InvalidPubkey(error.to_string()))?;
    EventBuilder::new(Kind::from(operation.buzz_kind()), "")
        .tag(Tag::public_key(pubkey))
        .custom_created_at(created_at)
        .sign_with_keys(bridge_keys)
        .map_err(|error| BuzzAdmissionCommandError::Signing(error.to_string()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingStatus {
    Pending,
    Active,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatIdentityBinding {
    pub binding_id: Uuid,
    pub tenant_id: TenantId,
    pub principal_id: PrincipalId,
    /// Nostr x-only public key, lowercase hex.
    pub buzz_pubkey: String,
    pub status: BindingStatus,
    pub created_at: DateTime<Utc>,
    pub verified_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub rotation_of: Option<Uuid>,
    #[serde(default)]
    pub audit_metadata: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingChallenge {
    pub challenge_id: Uuid,
    pub tenant_id: TenantId,
    pub principal_id: PrincipalId,
    pub buzz_pubkey: String,
    pub relay_url: String,
    pub rotation_of: Option<Uuid>,
    pub nonce: String,
    pub expires_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
}

impl BindingChallenge {
    pub fn issue(
        tenant_id: TenantId,
        principal_id: PrincipalId,
        buzz_pubkey: impl Into<String>,
        relay_url: impl Into<String>,
        rotation_of: Option<Uuid>,
        now: DateTime<Utc>,
        ttl: Duration,
    ) -> Self {
        Self {
            challenge_id: Uuid::new_v4(),
            tenant_id,
            principal_id,
            buzz_pubkey: buzz_pubkey.into().to_ascii_lowercase(),
            relay_url: relay_url.into(),
            rotation_of,
            nonce: Uuid::new_v4().to_string(),
            expires_at: now + ttl,
            consumed_at: None,
        }
    }

    /// Verify a real NIP-42 AUTH event and consume this challenge exactly once.
    pub fn verify_and_consume(
        &mut self,
        expected_tenant_id: TenantId,
        expected_principal_id: PrincipalId,
        event: &Event,
        now: DateTime<Utc>,
    ) -> Result<(), BindingError> {
        if self.tenant_id != expected_tenant_id || self.principal_id != expected_principal_id {
            return Err(BindingError::ChallengeScopeMismatch);
        }
        if self.consumed_at.is_some() {
            return Err(BindingError::ChallengeReplayed);
        }
        if now >= self.expires_at {
            return Err(BindingError::ChallengeExpired);
        }
        if event.kind != Kind::Authentication {
            return Err(BindingError::InvalidProof);
        }
        event.verify().map_err(|_| BindingError::InvalidProof)?;
        if event.pubkey.to_string() != self.buzz_pubkey {
            return Err(BindingError::PubkeyMismatch);
        }
        let challenge = event
            .tags
            .find(TagKind::Challenge)
            .and_then(|tag| tag.content())
            .ok_or(BindingError::ChallengeMismatch)?;
        if challenge != self.nonce {
            return Err(BindingError::ChallengeMismatch);
        }
        let relay = event
            .tags
            .find(TagKind::Relay)
            .and_then(|tag| tag.content())
            .ok_or(BindingError::RelayMismatch)?;
        if normalize_url(relay) != normalize_url(&self.relay_url) {
            return Err(BindingError::RelayMismatch);
        }
        let delta = now.timestamp().abs_diff(event.created_at.as_secs() as i64);
        if delta > NIP42_TOLERANCE_SECS {
            return Err(BindingError::InvalidProof);
        }
        self.consumed_at = Some(now);
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceCommunityMapping {
    pub tenant_id: TenantId,
    pub workspace_id: WorkspaceId,
    pub community_id: String,
    pub relay_url: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuzzAdmission {
    pub tenant_id: TenantId,
    pub workspace_id: WorkspaceId,
    pub community_id: String,
    pub buzz_pubkey: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionDecision {
    Allowed,
    Denied(BindingError),
}

impl AdmissionDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }
}

/// Fail-closed admission evaluation. Cryptographic validity alone never grants access.
pub fn authorize_admission(
    principal_active: bool,
    chat_access: bool,
    binding: &ChatIdentityBinding,
    mapping: &WorkspaceCommunityMapping,
    admission: &BuzzAdmission,
) -> AdmissionDecision {
    if !principal_active || !chat_access {
        return AdmissionDecision::Denied(BindingError::PrincipalInactive);
    }
    if binding.status != BindingStatus::Active {
        return AdmissionDecision::Denied(BindingError::BindingInactive);
    }
    if binding.tenant_id != mapping.tenant_id
        || binding.tenant_id != admission.tenant_id
        || mapping.tenant_id != admission.tenant_id
        || mapping.workspace_id != admission.workspace_id
        || mapping.community_id != admission.community_id
        || binding.buzz_pubkey != admission.buzz_pubkey
        || !mapping.active
        || !admission.active
    {
        return AdmissionDecision::Denied(BindingError::TenantOrCommunityMismatch);
    }
    AdmissionDecision::Allowed
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum BindingError {
    #[error("binding challenge was already consumed")]
    ChallengeReplayed,
    #[error("binding challenge is scoped to a different tenant or Principal")]
    ChallengeScopeMismatch,
    #[error("binding challenge expired")]
    ChallengeExpired,
    #[error("invalid NIP-42 proof")]
    InvalidProof,
    #[error("proof public key does not match the challenged key")]
    PubkeyMismatch,
    #[error("proof challenge does not match")]
    ChallengeMismatch,
    #[error("proof relay does not match the mapped community relay")]
    RelayMismatch,
    #[error("Principal is inactive or lacks Chat access")]
    PrincipalInactive,
    #[error("identity binding is inactive")]
    BindingInactive,
    #[error("tenant, workspace, community, or key does not match")]
    TenantOrCommunityMismatch,
}

fn normalize_url(value: &str) -> String {
    let Ok(mut url) = Url::parse(value) else {
        return value.trim_end_matches('/').to_ascii_lowercase();
    };
    let path = url.path().trim_end_matches('/').to_owned();
    url.set_path(&path);
    url.to_string().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, RelayUrl};

    const RELAY: &str = "wss://chat.example.test";

    fn ids() -> (TenantId, WorkspaceId, PrincipalId) {
        (
            TenantId(Uuid::new_v4()),
            WorkspaceId(Uuid::new_v4()),
            PrincipalId(Uuid::new_v4()),
        )
    }

    fn proof(keys: &Keys, nonce: &str) -> Event {
        EventBuilder::auth(nonce, RelayUrl::parse(RELAY).unwrap())
            .sign_with_keys(keys)
            .unwrap()
    }

    fn binding(t: TenantId, p: PrincipalId, key: &Keys) -> ChatIdentityBinding {
        ChatIdentityBinding {
            binding_id: Uuid::new_v4(),
            tenant_id: t,
            principal_id: p,
            buzz_pubkey: key.public_key().to_string(),
            status: BindingStatus::Active,
            created_at: Utc::now(),
            verified_at: Some(Utc::now()),
            revoked_at: None,
            rotation_of: None,
            audit_metadata: serde_json::json!({}),
        }
    }

    #[test]
    fn nip42_proof_is_tenant_and_key_scoped_and_one_time() {
        let (tenant, workspace, principal) = ids();
        let keys = Keys::generate();
        let mut challenge = BindingChallenge::issue(
            tenant,
            principal,
            keys.public_key().to_string(),
            RELAY,
            None,
            Utc::now(),
            Duration::minutes(1),
        );
        assert!(challenge
            .verify_and_consume(
                tenant,
                principal,
                &proof(&keys, &challenge.nonce),
                Utc::now()
            )
            .is_ok());
        assert_eq!(
            challenge.verify_and_consume(
                tenant,
                principal,
                &proof(&keys, &challenge.nonce),
                Utc::now()
            ),
            Err(BindingError::ChallengeReplayed)
        );

        let map = WorkspaceCommunityMapping {
            tenant_id: tenant,
            workspace_id: workspace,
            community_id: "c1".into(),
            relay_url: RELAY.into(),
            active: true,
        };
        let admission = BuzzAdmission {
            tenant_id: tenant,
            workspace_id: workspace,
            community_id: "c1".into(),
            buzz_pubkey: keys.public_key().to_string(),
            active: true,
        };
        assert!(authorize_admission(
            true,
            true,
            &binding(tenant, principal, &keys),
            &map,
            &admission
        )
        .is_allowed());
    }

    #[test]
    fn bridge_commands_use_generic_nip43_membership_kinds() {
        assert_eq!(BuzzAdmissionOperation::Admit.buzz_kind(), 9030);
        assert_eq!(BuzzAdmissionOperation::Revoke.buzz_kind(), 9031);
    }

    #[test]
    fn bridge_command_is_signed_by_service_identity_and_targets_bound_key() {
        let bridge_keys = Keys::generate();
        let user_keys = Keys::generate();
        let event = build_buzz_admission_event(
            BuzzAdmissionOperation::Admit,
            &user_keys.public_key().to_hex(),
            &bridge_keys,
        )
        .unwrap();

        assert_eq!(event.kind, Kind::from(BUZZ_RELAY_ADD_MEMBER_KIND));
        assert_eq!(event.pubkey, bridge_keys.public_key());
        assert!(event.verify().is_ok());
        assert_eq!(event.tags.len(), 1);
        assert_eq!(
            event.tags.iter().next().unwrap().content().unwrap(),
            user_keys.public_key().to_hex()
        );
    }

    #[test]
    fn hostile_proofs_and_cross_tenant_admission_fail_closed() {
        let (tenant, workspace, principal) = ids();
        let other_tenant = TenantId(Uuid::new_v4());
        let keys = Keys::generate();
        let wrong = Keys::generate();
        let now = Utc::now();
        let mut expired = BindingChallenge::issue(
            tenant,
            principal,
            keys.public_key().to_string(),
            RELAY,
            None,
            now,
            Duration::seconds(-1),
        );
        assert_eq!(
            expired.verify_and_consume(tenant, principal, &proof(&keys, &expired.nonce), now),
            Err(BindingError::ChallengeExpired)
        );
        let mut wrong_key = BindingChallenge::issue(
            tenant,
            principal,
            keys.public_key().to_string(),
            RELAY,
            None,
            now,
            Duration::minutes(1),
        );
        assert_eq!(
            wrong_key.verify_and_consume(tenant, principal, &proof(&wrong, &wrong_key.nonce), now),
            Err(BindingError::PubkeyMismatch)
        );
        let mut wrong_challenge = BindingChallenge::issue(
            tenant,
            principal,
            keys.public_key().to_string(),
            RELAY,
            None,
            now,
            Duration::minutes(1),
        );
        assert_eq!(
            wrong_challenge.verify_and_consume(
                tenant,
                principal,
                &proof(&keys, "wrong-challenge"),
                now
            ),
            Err(BindingError::ChallengeMismatch)
        );
        let mut tampered = BindingChallenge::issue(
            tenant,
            principal,
            keys.public_key().to_string(),
            RELAY,
            None,
            now,
            Duration::minutes(1),
        );
        let mut tampered_event = proof(&keys, &tampered.nonce);
        tampered_event.content = "tampered after signing".into();
        assert_eq!(
            tampered.verify_and_consume(tenant, principal, &tampered_event, now),
            Err(BindingError::InvalidProof)
        );
        let mut wrong_relay = BindingChallenge::issue(
            tenant,
            principal,
            keys.public_key().to_string(),
            "wss://other.example.test",
            None,
            now,
            Duration::minutes(1),
        );
        assert_eq!(
            wrong_relay.verify_and_consume(
                tenant,
                principal,
                &proof(&keys, &wrong_relay.nonce),
                now
            ),
            Err(BindingError::RelayMismatch)
        );
        let mut wrong_scope = BindingChallenge::issue(
            tenant,
            principal,
            keys.public_key().to_string(),
            RELAY,
            None,
            now,
            Duration::minutes(1),
        );
        assert_eq!(
            wrong_scope.verify_and_consume(
                other_tenant,
                principal,
                &proof(&keys, &wrong_scope.nonce),
                now
            ),
            Err(BindingError::ChallengeScopeMismatch)
        );
        let map = WorkspaceCommunityMapping {
            tenant_id: other_tenant,
            workspace_id: workspace,
            community_id: "c1".into(),
            relay_url: RELAY.into(),
            active: true,
        };
        let admission = BuzzAdmission {
            tenant_id: other_tenant,
            workspace_id: workspace,
            community_id: "c1".into(),
            buzz_pubkey: keys.public_key().to_string(),
            active: true,
        };
        assert!(!authorize_admission(
            true,
            true,
            &binding(tenant, principal, &keys),
            &map,
            &admission
        )
        .is_allowed());
        assert!(!authorize_admission(
            false,
            true,
            &binding(tenant, principal, &keys),
            &WorkspaceCommunityMapping {
                tenant_id: tenant,
                workspace_id: workspace,
                community_id: "c1".into(),
                relay_url: RELAY.into(),
                active: true
            },
            &BuzzAdmission {
                tenant_id: tenant,
                workspace_id: workspace,
                community_id: "c1".into(),
                buzz_pubkey: keys.public_key().to_string(),
                active: true
            }
        )
        .is_allowed());
        let mut revoked = binding(tenant, principal, &keys);
        revoked.status = BindingStatus::Revoked;
        assert!(!authorize_admission(
            true,
            true,
            &revoked,
            &WorkspaceCommunityMapping {
                tenant_id: tenant,
                workspace_id: workspace,
                community_id: "c1".into(),
                relay_url: RELAY.into(),
                active: true
            },
            &BuzzAdmission {
                tenant_id: tenant,
                workspace_id: workspace,
                community_id: "c1".into(),
                buzz_pubkey: keys.public_key().to_string(),
                active: true
            }
        )
        .is_allowed());
    }
}
