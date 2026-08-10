//! The parsed payload of the durable Integration Event
//! `io.elembra.chat.buzz.event.observed.v1` — the event Memory consumes.
//!
//! This module only *models* the payload; the bridge publishes it later. The
//! payload carries the signed Buzz event, its Chat context, the Elembra
//! Principal it was admitted under, and bridge observation metadata.
//!
//! Validation is fail-closed: a payload that fails [`ObservedChatEventData::validate`]
//! must never be projected into the Memory catalog.

use chrono::{DateTime, Utc};
use rustshare_core::domain::PrincipalId;
use serde::{Deserialize, Serialize};

/// Payload `data` of `io.elembra.chat.buzz.event.observed.v1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedChatEventData {
    pub buzz: BuzzEventMeta,
    pub context: ChatContext,
    pub principal: PrincipalMeta,
    pub observed_at: DateTime<Utc>,
}

/// The signed Buzz event as observed by the bridge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuzzEventMeta {
    /// Buzz event id (64-hex).
    pub event_id: String,
    /// Stable Buzz message id (root event id, 64-hex).
    pub message_id: String,
    pub event_type: ObservedEventType,
    pub supersedes_event_id: Option<String>,
    pub created_at: DateTime<Utc>,
    /// 64-hex author pubkey.
    pub pubkey: String,
    /// 128-hex Schnorr signature.
    pub signature: String,
    /// "sha256:<64hex>" of the canonical signed event json.
    pub checksum: String,
    pub signature_verified: bool,
}

/// The kind of Buzz event observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedEventType {
    Created,
    Edited,
    Deleted,
}

/// Chat context of the observed event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatContext {
    pub community_id: String,
    pub channel_id: String,
    pub channel_kind: ChatChannelKind,
    pub thread_root_id: Option<String>,
}

/// Buzz channel classification. `Workspace` channels are eligible for Memory
/// projection; the others are never projected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatChannelKind {
    Workspace,
    Dm,
    Private,
    Excluded,
}

/// The Elembra Principal the Buzz identity was admitted under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrincipalMeta {
    pub principal_id: PrincipalId,
}

/// Fail-closed validation failure for an observed chat event payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MemoryValidationError {
    #[error("event_id must be 64 lowercase hex characters")]
    InvalidEventId,
    #[error("message_id must be 64 lowercase hex characters")]
    InvalidMessageId,
    #[error("supersedes_event_id must be 64 lowercase hex characters")]
    InvalidSupersedesEventId,
    #[error("thread_root_id must be 64 lowercase hex characters")]
    InvalidThreadRootId,
    #[error("pubkey must be 64 lowercase hex characters")]
    InvalidPubkey,
    #[error("signature must be 128 lowercase hex characters")]
    InvalidSignature,
    #[error("checksum must be `sha256:` followed by 64 lowercase hex characters")]
    InvalidChecksum,
    #[error("event signature was not verified; unverified events must never be projected")]
    UnverifiedSignature,
    #[error("created event_id must equal its message_id (the first event of a message IS the message id)")]
    CreatedEventIdMismatch,
}

impl ObservedChatEventData {
    /// Fail-closed structural validation. Returns the first failing check.
    ///
    /// An event must be cryptographically verified by the publisher before
    /// publication; `signature_verified == false` is rejected outright so an
    /// unverified event can never be projected.
    pub fn validate(&self) -> Result<(), MemoryValidationError> {
        if !is_lower_hex(&self.buzz.event_id, 64) {
            return Err(MemoryValidationError::InvalidEventId);
        }
        if !is_lower_hex(&self.buzz.message_id, 64) {
            return Err(MemoryValidationError::InvalidMessageId);
        }
        if let Some(supersedes) = &self.buzz.supersedes_event_id {
            if !is_lower_hex(supersedes, 64) {
                return Err(MemoryValidationError::InvalidSupersedesEventId);
            }
        }
        if let Some(thread_root) = &self.context.thread_root_id {
            if !is_lower_hex(thread_root, 64) {
                return Err(MemoryValidationError::InvalidThreadRootId);
            }
        }
        if !is_lower_hex(&self.buzz.pubkey, 64) {
            return Err(MemoryValidationError::InvalidPubkey);
        }
        if !is_lower_hex(&self.buzz.signature, 128) {
            return Err(MemoryValidationError::InvalidSignature);
        }
        if !self
            .buzz
            .checksum
            .strip_prefix("sha256:")
            .is_some_and(|digest| is_lower_hex(digest, 64))
        {
            return Err(MemoryValidationError::InvalidChecksum);
        }
        if !self.buzz.signature_verified {
            return Err(MemoryValidationError::UnverifiedSignature);
        }
        if self.buzz.event_type == ObservedEventType::Created
            && self.buzz.event_id != self.buzz.message_id
        {
            return Err(MemoryValidationError::CreatedEventIdMismatch);
        }
        Ok(())
    }
}

fn is_lower_hex(s: &str, len: usize) -> bool {
    s.len() == len
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn hex64() -> String {
        "a".repeat(64)
    }

    fn event_data(event_type: ObservedEventType) -> ObservedChatEventData {
        ObservedChatEventData {
            buzz: BuzzEventMeta {
                event_id: hex64(),
                message_id: hex64(),
                event_type,
                supersedes_event_id: None,
                created_at: DateTime::<Utc>::from_timestamp(1_752_000_000, 0).unwrap(),
                pubkey: "b".repeat(64),
                signature: "c".repeat(128),
                checksum: format!("sha256:{}", "d".repeat(64)),
                signature_verified: true,
            },
            context: ChatContext {
                community_id: "community-1".into(),
                channel_id: "channel-1".into(),
                channel_kind: ChatChannelKind::Workspace,
                thread_root_id: None,
            },
            principal: PrincipalMeta {
                principal_id: PrincipalId::from(Uuid::new_v4()),
            },
            observed_at: DateTime::<Utc>::from_timestamp(1_752_000_100, 0).unwrap(),
        }
    }

    #[test]
    fn event_type_serde_snake_case_round_trips() {
        let cases = [
            (ObservedEventType::Created, "created"),
            (ObservedEventType::Edited, "edited"),
            (ObservedEventType::Deleted, "deleted"),
        ];
        for (variant, name) in cases {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(json, format!("\"{name}\""));
            let back: ObservedEventType = serde_json::from_str(&json).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn channel_kind_serde_snake_case_round_trips() {
        let cases = [
            (ChatChannelKind::Workspace, "workspace"),
            (ChatChannelKind::Dm, "dm"),
            (ChatChannelKind::Private, "private"),
            (ChatChannelKind::Excluded, "excluded"),
        ];
        for (variant, name) in cases {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(json, format!("\"{name}\""));
            let back: ChatChannelKind = serde_json::from_str(&json).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn full_payload_serde_round_trip() {
        let data = event_data(ObservedEventType::Edited);
        let json = serde_json::to_string(&data).unwrap();
        let back: ObservedChatEventData = serde_json::from_str(&json).unwrap();
        assert_eq!(back, data);
    }

    #[test]
    fn validate_accepts_valid_payloads() {
        // Created: first event of a message IS the message id.
        let created = event_data(ObservedEventType::Created);
        assert_eq!(created.validate(), Ok(()));
        // Edited with supersedes and thread context.
        let mut edited = event_data(ObservedEventType::Edited);
        edited.buzz.supersedes_event_id = Some("e".repeat(64));
        edited.context.thread_root_id = Some("f".repeat(64));
        assert_eq!(edited.validate(), Ok(()));
        // Deleted supersedes a message event.
        let mut deleted = event_data(ObservedEventType::Deleted);
        deleted.buzz.supersedes_event_id = Some("e".repeat(64));
        assert_eq!(deleted.validate(), Ok(()));
    }

    #[test]
    fn validate_rejects_bad_event_id() {
        let mut data = event_data(ObservedEventType::Created);
        data.buzz.event_id = "0".repeat(63);
        assert_eq!(data.validate(), Err(MemoryValidationError::InvalidEventId));
        data.buzz.event_id = "A".repeat(64); // uppercase
        assert_eq!(data.validate(), Err(MemoryValidationError::InvalidEventId));
        data.buzz.event_id = "z".repeat(64); // non-hex
        assert_eq!(data.validate(), Err(MemoryValidationError::InvalidEventId));
    }

    #[test]
    fn validate_rejects_bad_message_id() {
        let mut data = event_data(ObservedEventType::Created);
        data.buzz.message_id = "0".repeat(65);
        assert_eq!(
            data.validate(),
            Err(MemoryValidationError::InvalidMessageId)
        );
        data.buzz.message_id = "A".repeat(64);
        assert_eq!(
            data.validate(),
            Err(MemoryValidationError::InvalidMessageId)
        );
    }

    #[test]
    fn validate_rejects_bad_supersedes_event_id() {
        let mut data = event_data(ObservedEventType::Edited);
        data.buzz.supersedes_event_id = Some("x".repeat(64));
        assert_eq!(
            data.validate(),
            Err(MemoryValidationError::InvalidSupersedesEventId)
        );
    }

    #[test]
    fn validate_rejects_bad_thread_root_id() {
        let mut data = event_data(ObservedEventType::Created);
        data.context.thread_root_id = Some("y".repeat(63));
        assert_eq!(
            data.validate(),
            Err(MemoryValidationError::InvalidThreadRootId)
        );
    }

    #[test]
    fn validate_rejects_bad_pubkey() {
        let mut data = event_data(ObservedEventType::Created);
        data.buzz.pubkey = "1".repeat(63);
        assert_eq!(data.validate(), Err(MemoryValidationError::InvalidPubkey));
        data.buzz.pubkey = "B".repeat(64);
        assert_eq!(data.validate(), Err(MemoryValidationError::InvalidPubkey));
    }

    #[test]
    fn validate_rejects_bad_signature() {
        let mut data = event_data(ObservedEventType::Created);
        data.buzz.signature = "2".repeat(127);
        assert_eq!(
            data.validate(),
            Err(MemoryValidationError::InvalidSignature)
        );
        data.buzz.signature = "C".repeat(128);
        assert_eq!(
            data.validate(),
            Err(MemoryValidationError::InvalidSignature)
        );
        data.buzz.signature = "3".repeat(129);
        assert_eq!(
            data.validate(),
            Err(MemoryValidationError::InvalidSignature)
        );
    }

    #[test]
    fn validate_rejects_bad_checksum() {
        let mut data = event_data(ObservedEventType::Created);
        data.buzz.checksum = "deadbeef".into();
        assert_eq!(data.validate(), Err(MemoryValidationError::InvalidChecksum));
        data.buzz.checksum = format!("md5:{}", "d".repeat(64));
        assert_eq!(data.validate(), Err(MemoryValidationError::InvalidChecksum));
        data.buzz.checksum = format!("sha256:{}", "D".repeat(64));
        assert_eq!(data.validate(), Err(MemoryValidationError::InvalidChecksum));
        data.buzz.checksum = format!("sha256:{}", "d".repeat(63));
        assert_eq!(data.validate(), Err(MemoryValidationError::InvalidChecksum));
    }

    #[test]
    fn validate_rejects_unverified_signature() {
        let mut data = event_data(ObservedEventType::Created);
        data.buzz.signature_verified = false;
        assert_eq!(
            data.validate(),
            Err(MemoryValidationError::UnverifiedSignature)
        );
    }

    #[test]
    fn validate_rejects_created_with_mismatched_message_id() {
        let mut data = event_data(ObservedEventType::Created);
        data.buzz.event_id = "a".repeat(63) + "b";
        assert_eq!(
            data.validate(),
            Err(MemoryValidationError::CreatedEventIdMismatch)
        );
        // Non-created types are not subject to the identity rule.
        let edited = event_data(ObservedEventType::Edited);
        assert_eq!(edited.validate(), Ok(()));
    }

    #[test]
    fn validate_rejects_unverified_before_identity_rule() {
        // An unverified Created event with a mismatched id fails on verification
        // first: verification is the earlier, more fundamental gate.
        let mut data = event_data(ObservedEventType::Created);
        data.buzz.signature_verified = false;
        data.buzz.event_id = "a".repeat(63) + "b";
        assert_eq!(
            data.validate(),
            Err(MemoryValidationError::UnverifiedSignature)
        );
    }

    #[test]
    fn is_lower_hex_rejects_mixed_case_and_short_input() {
        assert!(!is_lower_hex(&"A".repeat(64), 64));
        assert!(!is_lower_hex(&"a".repeat(63), 64));
        assert!(!is_lower_hex(&"g".repeat(64), 64));
        assert!(is_lower_hex(&"0a9f".repeat(16), 64));
    }
}
