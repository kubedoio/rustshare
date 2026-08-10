//! Bridge-owned view of an observed Buzz chat event.
//!
//! [`ChatObservedEvent`] is the durable row shape of the bridge's observation
//! index (`chat_observed_events`): one row per signed Buzz event observed,
//! content-addressed by `(tenant_id, event_id)`. Buzz remains authoritative;
//! this is reference/provenance metadata only.

use chrono::{DateTime, Utc};
use rustshare_core::domain::{PrincipalId, TenantId, WorkspaceId};
use serde::{Deserialize, Serialize};

use crate::event::{ChatChannelKind, ObservedChatEventData, ObservedEventType};

/// One observed Buzz chat event as stored in the bridge observation index.
///
/// `body` is the indexing copy — `Some` only when the tenant has
/// `content_indexing` enabled at observation time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatObservedEvent {
    pub tenant_id: TenantId,
    pub workspace_id: WorkspaceId,
    pub event_id: String,
    pub message_id: String,
    pub event_type: ObservedEventType,
    pub supersedes_event_id: Option<String>,
    pub community_id: String,
    pub channel_id: String,
    pub channel_kind: ChatChannelKind,
    pub thread_root_id: Option<String>,
    pub author_pubkey: String,
    pub author_principal_id: Option<PrincipalId>,
    pub event_created_at: DateTime<Utc>,
    pub observed_at: DateTime<Utc>,
    pub checksum: String,
    pub signature: String,
    pub signature_verified: bool,
    pub body: Option<String>,
    pub active: bool,
}

impl ChatObservedEvent {
    /// Build from the parsed durable-event payload plus envelope tenant/workspace.
    /// `body` is the indexing copy — Some only when the tenant has
    /// content_indexing enabled at observation time.
    pub fn from_observed_data(
        tenant_id: TenantId,
        workspace_id: WorkspaceId,
        data: &ObservedChatEventData,
        body: Option<String>,
    ) -> Self {
        let event_type = data.buzz.event_type;
        Self {
            tenant_id,
            workspace_id,
            event_id: data.buzz.event_id.clone(),
            message_id: data.buzz.message_id.clone(),
            event_type,
            supersedes_event_id: data.buzz.supersedes_event_id.clone(),
            community_id: data.context.community_id.clone(),
            channel_id: data.context.channel_id.clone(),
            channel_kind: data.context.channel_kind,
            thread_root_id: data.context.thread_root_id.clone(),
            author_pubkey: data.buzz.pubkey.clone(),
            author_principal_id: Some(data.principal.principal_id),
            event_created_at: data.buzz.created_at,
            observed_at: data.observed_at,
            checksum: data.buzz.checksum.clone(),
            signature: data.buzz.signature.clone(),
            signature_verified: data.buzz.signature_verified,
            body,
            active: event_type != ObservedEventType::Deleted,
        }
    }

    /// Whether this observed event is a tombstone (`event_type == Deleted`).
    pub fn is_tombstone(&self) -> bool {
        self.event_type == ObservedEventType::Deleted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{BuzzEventMeta, ChatContext, PrincipalMeta};
    use uuid::Uuid;

    fn hex64(seed: u8) -> String {
        format!("{seed:02x}").repeat(32)
    }

    fn data(event_type: ObservedEventType) -> ObservedChatEventData {
        ObservedChatEventData {
            buzz: BuzzEventMeta {
                event_id: hex64(0xaa),
                message_id: hex64(0xaa),
                event_type,
                supersedes_event_id: Some(hex64(0x01)),
                created_at: DateTime::<Utc>::from_timestamp(1_752_000_000, 0).unwrap(),
                pubkey: hex64(0xbb),
                signature: "c".repeat(128),
                checksum: format!("sha256:{}", "d".repeat(64)),
                signature_verified: true,
            },
            context: ChatContext {
                community_id: "community-1".into(),
                channel_id: "channel-1".into(),
                channel_kind: ChatChannelKind::Workspace,
                thread_root_id: Some(hex64(0x0f)),
            },
            principal: PrincipalMeta {
                principal_id: PrincipalId::from(Uuid::new_v4()),
            },
            observed_at: DateTime::<Utc>::from_timestamp(1_752_000_100, 0).unwrap(),
        }
    }

    #[test]
    fn from_observed_data_maps_field_by_field() {
        let tenant = TenantId::from(Uuid::new_v4());
        let workspace = WorkspaceId::from(Uuid::new_v4());
        let payload = data(ObservedEventType::Edited);
        let event =
            ChatObservedEvent::from_observed_data(tenant, workspace, &payload, Some("body".into()));

        assert_eq!(event.tenant_id, tenant);
        assert_eq!(event.workspace_id, workspace);
        assert_eq!(event.event_id, payload.buzz.event_id);
        assert_eq!(event.message_id, payload.buzz.message_id);
        assert_eq!(event.event_type, ObservedEventType::Edited);
        assert_eq!(event.supersedes_event_id, payload.buzz.supersedes_event_id);
        assert_eq!(event.community_id, payload.context.community_id);
        assert_eq!(event.channel_id, payload.context.channel_id);
        assert_eq!(event.channel_kind, ChatChannelKind::Workspace);
        assert_eq!(event.thread_root_id, payload.context.thread_root_id);
        assert_eq!(event.author_pubkey, payload.buzz.pubkey);
        assert_eq!(
            event.author_principal_id,
            Some(payload.principal.principal_id)
        );
        assert_eq!(event.event_created_at, payload.buzz.created_at);
        assert_eq!(event.observed_at, payload.observed_at);
        assert_eq!(event.checksum, payload.buzz.checksum);
        assert_eq!(event.signature, payload.buzz.signature);
        assert!(event.signature_verified);
        assert_eq!(event.body.as_deref(), Some("body"));
        assert!(event.active, "non-deleted events are active");
        assert!(!event.is_tombstone());
    }

    #[test]
    fn deleted_events_are_inactive_tombstones() {
        let tenant = TenantId::from(Uuid::new_v4());
        let workspace = WorkspaceId::from(Uuid::new_v4());
        let payload = data(ObservedEventType::Deleted);
        let event = ChatObservedEvent::from_observed_data(tenant, workspace, &payload, None);
        assert!(!event.active, "deleted events are inactive");
        assert!(event.is_tombstone());
    }

    #[test]
    fn body_is_preserved_verbatim() {
        let tenant = TenantId::from(Uuid::new_v4());
        let workspace = WorkspaceId::from(Uuid::new_v4());
        let event = ChatObservedEvent::from_observed_data(
            tenant,
            workspace,
            &data(ObservedEventType::Created),
            None,
        );
        assert_eq!(event.body, None, "absent body stays None");
    }

    #[test]
    fn serde_round_trip() {
        let tenant = TenantId::from(Uuid::new_v4());
        let workspace = WorkspaceId::from(Uuid::new_v4());
        let event = ChatObservedEvent::from_observed_data(
            tenant,
            workspace,
            &data(ObservedEventType::Created),
            Some("v1".into()),
        );
        let json = serde_json::to_string(&event).unwrap();
        let back: ChatObservedEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, event);
    }
}
