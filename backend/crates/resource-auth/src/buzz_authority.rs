//! Buzz source-authorization contract and the coarse local fallback gate.
//!
//! Elembra never holds a human's Buzz signing key and never re-derives channel
//! membership itself. Channel/message visibility is decided by the community's
//! authoritative Buzz relay; this module defines the query shape and the
//! decision surface used by the Elembra-side authority client. The relay
//! remains the final authority and every failure mode fails closed.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rustshare_core::domain::TenantId;
use serde::{Deserialize, Serialize};

/// Buzz channel classification, wire-identical to
/// `rustshare_memory::event::ChatChannelKind` (`workspace|dm|private|excluded`).
/// The Memory crate must not leak into resource-auth, so this is a local copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuzzChannelKind {
    Workspace,
    Dm,
    Private,
    Excluded,
}

/// Ask whether `pubkey` may currently read `channel_id`/`message_id` in the
/// mapped community, as decided by the community's current Buzz authority.
#[derive(Debug, Clone)]
pub struct BuzzReadRequest {
    pub tenant_id: TenantId,
    pub community_id: String,
    pub relay_url: String,
    pub relay_pubkey: Option<String>,
    pub channel_id: String,
    pub channel_kind: BuzzChannelKind,
    pub message_id: Option<String>,
    pub pubkey: String,
    pub event_created_at: DateTime<Utc>,
}

/// Final read decision from the current Buzz authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuzzReadDecision {
    Allow,
    Deny,
    NotFound,
}

impl BuzzReadDecision {
    pub fn is_allow(&self) -> bool {
        matches!(self, Self::Allow)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BuzzAuthorityError {
    #[error("transport error: {0}")]
    Transport(String),
    #[error("upstream refused or rejected the request")]
    Unauthorized,
    #[error("invalid or unverifiable upstream response: {0}")]
    InvalidResponse(String),
    #[error("authority configuration error: {0}")]
    Config(String),
}

/// Source-authorization contract implemented by the authoritative Buzz
/// authority for a community. The final decision comes from CURRENT Buzz
/// state; any failure must fail closed (see the v1alpha1 upstream spec).
#[async_trait]
pub trait BuzzAuthority: Send + Sync {
    async fn can_read(&self, req: &BuzzReadRequest)
        -> Result<BuzzReadDecision, BuzzAuthorityError>;
}

/// Coarse community-level gate preserving today's behavior when no upstream
/// Buzz authority is configured: workspace channels are allowed, everything
/// else is denied.
///
/// This is NOT final per-channel authorization — an upstream
/// [`BuzzAuthority`] remains authoritative and must fail closed.
pub struct LocalFallbackAuthority;

#[async_trait]
impl BuzzAuthority for LocalFallbackAuthority {
    async fn can_read(
        &self,
        req: &BuzzReadRequest,
    ) -> Result<BuzzReadDecision, BuzzAuthorityError> {
        Ok(match req.channel_kind {
            BuzzChannelKind::Workspace => BuzzReadDecision::Allow,
            _ => BuzzReadDecision::Deny,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn read_request(channel_kind: BuzzChannelKind) -> BuzzReadRequest {
        BuzzReadRequest {
            tenant_id: TenantId(Uuid::new_v4()),
            community_id: "c1".into(),
            relay_url: "wss://chat.example.test".into(),
            relay_pubkey: None,
            channel_id: "ch1".into(),
            channel_kind,
            message_id: None,
            pubkey: "a".repeat(64),
            event_created_at: Utc::now(),
        }
    }

    #[test]
    fn channel_kind_serde_round_trip_is_snake_case() {
        for (kind, wire) in [
            (BuzzChannelKind::Workspace, "workspace"),
            (BuzzChannelKind::Dm, "dm"),
            (BuzzChannelKind::Private, "private"),
            (BuzzChannelKind::Excluded, "excluded"),
        ] {
            assert_eq!(serde_json::to_string(&kind).unwrap(), format!("\"{wire}\""));
            let parsed: BuzzChannelKind = serde_json::from_str(&format!("\"{wire}\"")).unwrap();
            assert_eq!(parsed, kind);
        }
    }

    #[test]
    fn channel_kind_rejects_unknown_wire_values() {
        assert!(serde_json::from_str::<BuzzChannelKind>("\"public\"").is_err());
    }

    #[tokio::test]
    async fn local_fallback_allows_only_workspace_channels() {
        let authority = LocalFallbackAuthority;
        let cases = [
            (BuzzChannelKind::Workspace, true),
            (BuzzChannelKind::Dm, false),
            (BuzzChannelKind::Private, false),
            (BuzzChannelKind::Excluded, false),
        ];
        for (kind, expected) in cases {
            let decision = authority.can_read(&read_request(kind)).await.unwrap();
            assert_eq!(
                decision.is_allow(),
                expected,
                "unexpected decision for {kind:?}: {decision:?}"
            );
        }
    }

    #[test]
    fn read_decision_is_allow_is_correct() {
        assert!(BuzzReadDecision::Allow.is_allow());
        assert!(!BuzzReadDecision::Deny.is_allow());
        assert!(!BuzzReadDecision::NotFound.is_allow());
    }
}
