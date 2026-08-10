//! The Memory catalog record (`memory_catalog`): exactly one per Buzz message
//! per tenant, mirroring the latest signed event.

use chrono::{DateTime, Utc};
use rustshare_core::domain::{PrincipalId, TenantId, WorkspaceId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::event::{ChatChannelKind, ObservedEventType};

/// Canonical source Application of Memory records projected from Buzz.
pub const SOURCE_APPLICATION: &str = "io.elembra.chat";
/// Canonical source type of a message record.
pub const SOURCE_TYPE_MESSAGE: &str = "message";
/// Canonical authorization source of a Buzz-projected record.
pub const AUTHORIZATION_SOURCE_BUZZ: &str = "buzz";
/// Default classification assigned to projected records.
pub const DEFAULT_CLASSIFICATION: &str = "general";

/// One durable Memory catalog record for a Buzz message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryCatalogRecord {
    pub record_id: Uuid,
    pub tenant_id: TenantId,
    pub workspace_id: WorkspaceId,
    /// "io.elembra.chat".
    pub source_application: String,
    /// "message".
    pub source_type: String,
    /// "elembra://io.elembra.chat/message/<message_id>".
    pub source_ref: String,
    pub message_id: String,
    pub latest_event_id: String,
    pub event_type: ObservedEventType,
    pub community_id: String,
    pub channel_id: String,
    pub channel_kind: ChatChannelKind,
    pub author_pubkey: String,
    pub author_principal_id: Option<PrincipalId>,
    pub occurred_at: DateTime<Utc>,
    pub observed_at: DateTime<Utc>,
    pub checksum: String,
    pub signature: String,
    pub signature_verified: bool,
    pub provenance: Vec<ProvenanceEntry>,
    /// Default "general".
    pub classification: String,
    pub retention_policy_ref: Option<String>,
    pub legal_hold_ref: Option<String>,
    /// "buzz".
    pub authorization_source: String,
    /// "community:<community_id>:pubkey:<pubkey>".
    pub authorization_ref: String,
    pub content_indexing: bool,
    pub content: Option<String>,
    pub indexing_status: IndexingStatus,
    pub tombstoned_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MemoryCatalogRecord {
    /// `elembra://io.elembra.chat/message/<message_id>`
    pub fn source_ref_for(message_id: &str) -> String {
        format!("elembra://{SOURCE_APPLICATION}/message/{message_id}")
    }

    /// `community:<community_id>:pubkey:<pubkey>`
    pub fn authorization_ref_for(community_id: &str, pubkey: &str) -> String {
        format!("community:{community_id}:pubkey:{pubkey}")
    }
}

/// A signed Buzz event that was projected into the record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceEntry {
    pub event_id: String,
    pub event_type: ObservedEventType,
    pub occurred_at: DateTime<Utc>,
    pub observed_at: DateTime<Utc>,
}

/// Storage state of the record's content copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexingStatus {
    ReferenceOnly,
    ContentStored,
    Tombstoned,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_ref_matches_spec() {
        assert_eq!(
            MemoryCatalogRecord::source_ref_for("a".repeat(64).as_str()),
            format!("elembra://io.elembra.chat/message/{}", "a".repeat(64))
        );
    }

    #[test]
    fn authorization_ref_matches_spec() {
        assert_eq!(
            MemoryCatalogRecord::authorization_ref_for("community-1", "b".repeat(64).as_str()),
            format!("community:community-1:pubkey:{}", "b".repeat(64))
        );
    }

    #[test]
    fn indexing_status_serde_snake_case_round_trips() {
        let cases = [
            (IndexingStatus::ReferenceOnly, "reference_only"),
            (IndexingStatus::ContentStored, "content_stored"),
            (IndexingStatus::Tombstoned, "tombstoned"),
        ];
        for (variant, name) in cases {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(json, format!("\"{name}\""));
            let back: IndexingStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(back, variant);
        }
    }
}
