//! Pure projection functions: build and update `memory_catalog` records from
//! validated observed-event payloads.
//!
//! All functions here are deterministic given their inputs — they never call
//! `Utc::now()` or read shared state. Callers decide policy (via
//! [`ProjectionPolicy`]) and what content copy to store; these functions only
//! apply the rules.

use rustshare_core::domain::{TenantId, WorkspaceId};
use uuid::Uuid;

use crate::event::{ObservedChatEventData, ObservedEventType};
use crate::policy::ProjectionDecision;
use crate::policy::ProjectionPolicy;
use crate::record::{
    IndexingStatus, MemoryCatalogRecord, ProvenanceEntry, AUTHORIZATION_SOURCE_BUZZ,
    DEFAULT_CLASSIFICATION, SOURCE_APPLICATION, SOURCE_TYPE_MESSAGE,
};

/// Build the initial catalog record from a validated observed-event payload.
///
/// Returns `None` when the policy skips the event (projection disabled, or a
/// `dm`/`private`/`excluded` channel). `tenant_id`/`workspace_id` come from
/// the event envelope (the envelope's tenant/workspace are not part of
/// `data`). `content` is the indexing copy — `Some` only when
/// `policy.content_indexing` and the caller has a body (from the observation
/// index); pass `None` otherwise.
///
/// `content_indexing` on the record reflects whether an indexing copy is
/// stored (`content.is_some()`), not the tenant policy at projection time.
pub fn project_record(
    tenant_id: TenantId,
    workspace_id: WorkspaceId,
    event: &ObservedChatEventData,
    policy: &ProjectionPolicy,
    content: Option<String>,
) -> Option<MemoryCatalogRecord> {
    if matches!(
        policy.decision(event.context.channel_kind),
        ProjectionDecision::Skip(_)
    ) {
        return None;
    }
    // Fail closed: never store a body the tenant has not opted into.
    let content = if policy.content_indexing {
        content
    } else {
        None
    };
    let indexing_status = if content.is_some() {
        IndexingStatus::ContentStored
    } else {
        IndexingStatus::ReferenceOnly
    };

    Some(MemoryCatalogRecord {
        record_id: Uuid::new_v4(),
        tenant_id,
        workspace_id,
        source_application: SOURCE_APPLICATION.to_string(),
        source_type: SOURCE_TYPE_MESSAGE.to_string(),
        source_ref: MemoryCatalogRecord::source_ref_for(&event.buzz.message_id),
        message_id: event.buzz.message_id.clone(),
        latest_event_id: event.buzz.event_id.clone(),
        event_type: event.buzz.event_type,
        community_id: event.context.community_id.clone(),
        channel_id: event.context.channel_id.clone(),
        channel_kind: event.context.channel_kind,
        author_pubkey: event.buzz.pubkey.clone(),
        author_principal_id: Some(event.principal.principal_id),
        occurred_at: event.buzz.created_at,
        observed_at: event.observed_at,
        checksum: event.buzz.checksum.clone(),
        signature: event.buzz.signature.clone(),
        signature_verified: event.buzz.signature_verified,
        provenance: vec![ProvenanceEntry {
            event_id: event.buzz.event_id.clone(),
            event_type: event.buzz.event_type,
            occurred_at: event.buzz.created_at,
            observed_at: event.observed_at,
        }],
        classification: DEFAULT_CLASSIFICATION.to_string(),
        retention_policy_ref: None,
        legal_hold_ref: None,
        authorization_source: AUTHORIZATION_SOURCE_BUZZ.to_string(),
        authorization_ref: MemoryCatalogRecord::authorization_ref_for(
            &event.context.community_id,
            &event.buzz.pubkey,
        ),
        content_indexing: content.is_some(),
        content,
        indexing_status,
        tombstoned_at: None,
        created_at: event.observed_at,
        updated_at: event.observed_at,
    })
}

/// Apply an edited/replaced event to an existing record (same message).
///
/// Guards:
/// * a Deleted event is never applied here — it must route through
///   [`apply_tombstone`] — `apply_event` returns the existing record
///   unchanged;
/// * a tombstoned record is never edited (`apply_event` returns it unchanged;
///   only [`apply_tombstone`] transitions to tombstoned) — likewise a record
///   whose `event_type` is already `Deleted`, regardless of
///   `indexing_status` (defense in depth against resurrecting a deletion);
/// * an edit applies only when the new event's `created_at` is `>=` the
///   record's `occurred_at` — never regress to an older event after a newer
///   one (out-of-order delivery guard); otherwise the record is returned
///   unchanged.
///
/// On apply: appends a provenance entry and updates the latest-event fields
/// (latest_event_id, event_type, checksum, signature, occurred_at, observed_at)
/// plus `content`/`content_indexing`/`indexing_status` per the content rule.
/// Identity fields (record_id, tenant, workspace, source_ref, message_id,
/// community/channel, author, classification, retention/legal-hold,
/// authorization, created_at, tombstoned_at) are preserved.
///
/// `content_indexing` on the record reflects whether an indexing copy is
/// stored (`content.is_some()`), not the tenant policy at projection time.
pub fn apply_event(
    existing: &MemoryCatalogRecord,
    event: &ObservedChatEventData,
    content: Option<String>,
) -> MemoryCatalogRecord {
    if event.buzz.event_type == ObservedEventType::Deleted {
        return existing.clone();
    }
    if existing.event_type == ObservedEventType::Deleted
        || existing.indexing_status == IndexingStatus::Tombstoned
    {
        // A deleted/tombstoned record is never edited; only `apply_tombstone`
        // transitions to tombstoned. The `event_type` check is defense in
        // depth: a record whose event_type is Deleted must not be resurrected
        // even if its indexing_status is not (yet) Tombstoned.
        return existing.clone();
    }
    if event.buzz.created_at < existing.occurred_at {
        return existing.clone();
    }

    let content_indexing = content.is_some();
    let indexing_status = if content.is_some() {
        IndexingStatus::ContentStored
    } else {
        IndexingStatus::ReferenceOnly
    };

    let mut updated = existing.clone();
    updated.latest_event_id = event.buzz.event_id.clone();
    updated.event_type = event.buzz.event_type;
    updated.occurred_at = event.buzz.created_at;
    updated.observed_at = event.observed_at;
    updated.checksum = event.buzz.checksum.clone();
    updated.signature = event.buzz.signature.clone();
    updated.signature_verified = event.buzz.signature_verified;
    updated.content_indexing = content_indexing;
    updated.content = content;
    updated.indexing_status = indexing_status;
    updated.updated_at = event.observed_at;
    updated.provenance.push(ProvenanceEntry {
        event_id: event.buzz.event_id.clone(),
        event_type: event.buzz.event_type,
        occurred_at: event.buzz.created_at,
        observed_at: event.observed_at,
    });
    updated
}

/// Apply a deleted/tombstoned event: sets `event_type = Deleted`,
/// `indexing_status = Tombstoned`, `tombstoned_at = event.observed_at`,
/// updates the latest-event fields and appends a provenance entry.
/// Always applies (a tombstone supersedes any newer-looking edit).
///
/// Stored content is preserved as-is: retention and legal-hold policy own
/// body erasure, not the projection.
pub fn apply_tombstone(
    existing: &MemoryCatalogRecord,
    event: &ObservedChatEventData,
) -> MemoryCatalogRecord {
    let mut updated = existing.clone();
    updated.latest_event_id = event.buzz.event_id.clone();
    updated.event_type = ObservedEventType::Deleted;
    updated.occurred_at = event.buzz.created_at;
    updated.observed_at = event.observed_at;
    updated.checksum = event.buzz.checksum.clone();
    updated.signature = event.buzz.signature.clone();
    updated.signature_verified = event.buzz.signature_verified;
    updated.indexing_status = IndexingStatus::Tombstoned;
    updated.tombstoned_at = Some(event.observed_at);
    updated.updated_at = event.observed_at;
    updated.provenance.push(ProvenanceEntry {
        event_id: event.buzz.event_id.clone(),
        event_type: ObservedEventType::Deleted,
        occurred_at: event.buzz.created_at,
        observed_at: event.observed_at,
    });
    updated
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use rustshare_core::domain::PrincipalId;
    use std::collections::HashSet;
    use uuid::Uuid;

    use crate::event::{BuzzEventMeta, ChatChannelKind, ChatContext, PrincipalMeta};

    const T0: i64 = 1_752_000_000;

    fn hex64(seed: u8) -> String {
        format!("{seed:02x}").repeat(32)
    }

    fn event_data(
        event_type: ObservedEventType,
        event_id: &str,
        created_at_ts: i64,
        channel_kind: ChatChannelKind,
    ) -> ObservedChatEventData {
        ObservedChatEventData {
            buzz: BuzzEventMeta {
                event_id: event_id.to_string(),
                message_id: hex64(0xaa),
                event_type,
                supersedes_event_id: None,
                created_at: DateTime::<Utc>::from_timestamp(created_at_ts, 0).unwrap(),
                pubkey: hex64(0xbb),
                signature: "c".repeat(128),
                checksum: format!("sha256:{}", "d".repeat(64)),
                signature_verified: true,
            },
            context: ChatContext {
                community_id: "community-1".into(),
                channel_id: "channel-1".into(),
                channel_kind,
                thread_root_id: None,
            },
            principal: PrincipalMeta {
                principal_id: PrincipalId::from(Uuid::new_v4()),
            },
            observed_at: DateTime::<Utc>::from_timestamp(created_at_ts + 100, 0).unwrap(),
        }
    }

    fn tenant() -> TenantId {
        TenantId::from(Uuid::new_v4())
    }

    fn workspace() -> WorkspaceId {
        WorkspaceId::from(Uuid::new_v4())
    }

    fn policy(content_indexing: bool) -> ProjectionPolicy {
        ProjectionPolicy {
            memory_projection: true,
            content_indexing,
        }
    }

    #[test]
    fn project_record_returns_none_when_policy_skips() {
        let event = event_data(
            ObservedEventType::Created,
            &hex64(0xaa),
            T0,
            ChatChannelKind::Workspace,
        );
        // Projection disabled.
        let off = ProjectionPolicy::default();
        assert_eq!(
            project_record(tenant(), workspace(), &event, &off, None),
            None
        );
        // Never-eligible channel, even with projection on.
        let dm = event_data(
            ObservedEventType::Created,
            &hex64(0xaa),
            T0,
            ChatChannelKind::Dm,
        );
        assert_eq!(
            project_record(tenant(), workspace(), &dm, &policy(true), None),
            None
        );
    }

    #[test]
    fn project_record_builds_reference_only_record_field_by_field() {
        let t = tenant();
        let w = workspace();
        let event = event_data(
            ObservedEventType::Created,
            &hex64(0xaa),
            T0,
            ChatChannelKind::Workspace,
        );
        let record = project_record(t, w, &event, &policy(false), None).unwrap();

        assert_eq!(record.tenant_id, t);
        assert_eq!(record.workspace_id, w);
        assert_eq!(record.source_application, "io.elembra.chat");
        assert_eq!(record.source_type, "message");
        assert_eq!(
            record.source_ref,
            format!("elembra://io.elembra.chat/message/{}", hex64(0xaa))
        );
        assert_eq!(record.message_id, hex64(0xaa));
        assert_eq!(record.latest_event_id, event.buzz.event_id);
        assert_eq!(record.event_type, ObservedEventType::Created);
        assert_eq!(record.community_id, "community-1");
        assert_eq!(record.channel_id, "channel-1");
        assert_eq!(record.channel_kind, ChatChannelKind::Workspace);
        assert_eq!(record.author_pubkey, hex64(0xbb));
        assert_eq!(
            record.author_principal_id,
            Some(event.principal.principal_id)
        );
        assert_eq!(record.occurred_at, event.buzz.created_at);
        assert_eq!(record.observed_at, event.observed_at);
        assert_eq!(record.checksum, event.buzz.checksum);
        assert_eq!(record.signature, event.buzz.signature);
        assert!(record.signature_verified);
        assert_eq!(record.provenance.len(), 1);
        assert_eq!(record.provenance[0].event_id, event.buzz.event_id);
        assert_eq!(record.provenance[0].event_type, ObservedEventType::Created);
        assert_eq!(record.provenance[0].occurred_at, event.buzz.created_at);
        assert_eq!(record.provenance[0].observed_at, event.observed_at);
        assert_eq!(record.classification, "general");
        assert_eq!(record.retention_policy_ref, None);
        assert_eq!(record.legal_hold_ref, None);
        assert_eq!(record.authorization_source, "buzz");
        assert_eq!(
            record.authorization_ref,
            format!("community:community-1:pubkey:{}", hex64(0xbb))
        );
        assert!(!record.content_indexing);
        assert_eq!(record.content, None);
        assert_eq!(record.indexing_status, IndexingStatus::ReferenceOnly);
        assert_eq!(record.tombstoned_at, None);
        assert_eq!(record.created_at, event.observed_at);
        assert_eq!(record.updated_at, event.observed_at);
    }

    #[test]
    fn project_record_stores_content_only_when_policy_allows() {
        let event = event_data(
            ObservedEventType::Created,
            &hex64(0xaa),
            T0,
            ChatChannelKind::Workspace,
        );
        let indexed = project_record(
            tenant(),
            workspace(),
            &event,
            &policy(true),
            Some("body".into()),
        )
        .unwrap();
        assert!(indexed.content_indexing);
        assert_eq!(indexed.content.as_deref(), Some("body"));
        assert_eq!(indexed.indexing_status, IndexingStatus::ContentStored);

        // Indexing enabled but no body available: reference-only record. The
        // record's `content_indexing` reflects the stored copy, so it is off.
        let no_body = project_record(tenant(), workspace(), &event, &policy(true), None).unwrap();
        assert!(!no_body.content_indexing);
        assert_eq!(no_body.content, None);
        assert_eq!(no_body.indexing_status, IndexingStatus::ReferenceOnly);

        // Indexing disabled: a caller-provided body is dropped (fail closed).
        let dropped = project_record(
            tenant(),
            workspace(),
            &event,
            &policy(false),
            Some("body".into()),
        )
        .unwrap();
        assert!(!dropped.content_indexing);
        assert_eq!(dropped.content, None);
        assert_eq!(dropped.indexing_status, IndexingStatus::ReferenceOnly);
    }

    #[test]
    fn project_record_is_deterministic_except_record_id() {
        let event = event_data(
            ObservedEventType::Created,
            &hex64(0xaa),
            T0,
            ChatChannelKind::Workspace,
        );
        let t = tenant();
        let w = workspace();
        let a = project_record(t, w, &event, &policy(true), Some("body".into())).unwrap();
        let b = project_record(t, w, &event, &policy(true), Some("body".into())).unwrap();
        assert_ne!(a.record_id, b.record_id);
        let mut with_a_id = a.clone();
        with_a_id.record_id = b.record_id;
        assert_eq!(with_a_id, b);
    }

    #[test]
    fn apply_event_applies_newer_edit() {
        let created = event_data(
            ObservedEventType::Created,
            &hex64(0xaa),
            T0,
            ChatChannelKind::Workspace,
        );
        let original = project_record(
            tenant(),
            workspace(),
            &created,
            &policy(true),
            Some("v1".into()),
        )
        .unwrap();

        let mut edit = event_data(
            ObservedEventType::Edited,
            &hex64(0xee),
            T0 + 10,
            ChatChannelKind::Workspace,
        );
        edit.buzz.supersedes_event_id = Some(hex64(0xaa));
        let updated = apply_event(&original, &edit, Some("v2".into()));

        assert_eq!(updated.record_id, original.record_id);
        assert_eq!(updated.latest_event_id, hex64(0xee));
        assert_eq!(updated.event_type, ObservedEventType::Edited);
        assert_eq!(updated.occurred_at, edit.buzz.created_at);
        assert_eq!(updated.observed_at, edit.observed_at);
        assert_eq!(updated.checksum, edit.buzz.checksum);
        assert_eq!(updated.signature, edit.buzz.signature);
        assert_eq!(updated.updated_at, edit.observed_at);
        assert_eq!(updated.provenance.len(), 2);
        assert_eq!(updated.provenance[1].event_id, hex64(0xee));
        assert_eq!(updated.content.as_deref(), Some("v2"));
        assert_eq!(updated.indexing_status, IndexingStatus::ContentStored);
        assert!(updated.content_indexing);
        // Identity fields preserved.
        assert_eq!(updated.tenant_id, original.tenant_id);
        assert_eq!(updated.workspace_id, original.workspace_id);
        assert_eq!(updated.source_ref, original.source_ref);
        assert_eq!(updated.message_id, original.message_id);
        assert_eq!(updated.community_id, original.community_id);
        assert_eq!(updated.channel_id, original.channel_id);
        assert_eq!(updated.channel_kind, original.channel_kind);
        assert_eq!(updated.author_pubkey, original.author_pubkey);
        assert_eq!(updated.author_principal_id, original.author_principal_id);
        assert_eq!(updated.classification, original.classification);
        assert_eq!(updated.authorization_ref, original.authorization_ref);
        assert_eq!(updated.created_at, original.created_at);
        assert_eq!(updated.tombstoned_at, None);
    }

    #[test]
    fn apply_event_equal_timestamp_applies() {
        let created = event_data(
            ObservedEventType::Created,
            &hex64(0xaa),
            T0,
            ChatChannelKind::Workspace,
        );
        let original =
            project_record(tenant(), workspace(), &created, &policy(false), None).unwrap();
        let same_ts = event_data(
            ObservedEventType::Edited,
            &hex64(0xee),
            T0,
            ChatChannelKind::Workspace,
        );
        let updated = apply_event(&original, &same_ts, None);
        assert_eq!(updated.latest_event_id, hex64(0xee));
        assert_eq!(updated.provenance.len(), 2);
    }

    #[test]
    fn apply_event_ignores_out_of_order_older_event() {
        let created = event_data(
            ObservedEventType::Created,
            &hex64(0xaa),
            T0,
            ChatChannelKind::Workspace,
        );
        let original =
            project_record(tenant(), workspace(), &created, &policy(false), None).unwrap();
        let older = event_data(
            ObservedEventType::Edited,
            &hex64(0xee),
            T0 - 50,
            ChatChannelKind::Workspace,
        );
        assert_eq!(apply_event(&original, &older, None), original);
    }

    #[test]
    fn apply_event_ignores_deleted_events() {
        let created = event_data(
            ObservedEventType::Created,
            &hex64(0xaa),
            T0,
            ChatChannelKind::Workspace,
        );
        let original = project_record(
            tenant(),
            workspace(),
            &created,
            &policy(true),
            Some("v1".into()),
        )
        .unwrap();
        let deleted = event_data(
            ObservedEventType::Deleted,
            &hex64(0xdd),
            T0 + 20,
            ChatChannelKind::Workspace,
        );
        // Deletions must route through `apply_tombstone`; `apply_event`
        // returns the record unchanged (no state change, no provenance entry).
        assert_eq!(apply_event(&original, &deleted, None), original);
    }

    #[test]
    fn apply_event_never_un_tombstones() {
        let created = event_data(
            ObservedEventType::Created,
            &hex64(0xaa),
            T0,
            ChatChannelKind::Workspace,
        );
        let original =
            project_record(tenant(), workspace(), &created, &policy(false), None).unwrap();
        let tombstone = event_data(
            ObservedEventType::Deleted,
            &hex64(0xdd),
            T0 + 20,
            ChatChannelKind::Workspace,
        );
        let tombstones = apply_tombstone(&original, &tombstone);
        assert_eq!(tombstones.indexing_status, IndexingStatus::Tombstoned);

        // A newer edit must not resurrect a tombstoned record.
        let newer_edit = event_data(
            ObservedEventType::Edited,
            &hex64(0xee),
            T0 + 30,
            ChatChannelKind::Workspace,
        );
        assert_eq!(
            apply_event(&tombstones, &newer_edit, Some("resurrected".into())),
            tombstones
        );
    }

    #[test]
    fn apply_event_never_resurrects_deleted_record_without_tombstone_status() {
        let created = event_data(
            ObservedEventType::Created,
            &hex64(0xaa),
            T0,
            ChatChannelKind::Workspace,
        );
        let mut deleted =
            project_record(tenant(), workspace(), &created, &policy(false), None).unwrap();
        // The inconsistent state the tombstone fix prevents: event_type is
        // Deleted but indexing_status is not Tombstoned. An edit must still
        // never resurrect it (defense in depth).
        deleted.event_type = ObservedEventType::Deleted;
        assert_eq!(deleted.indexing_status, IndexingStatus::ReferenceOnly);

        let newer_edit = event_data(
            ObservedEventType::Edited,
            &hex64(0xee),
            T0 + 30,
            ChatChannelKind::Workspace,
        );
        assert_eq!(
            apply_event(&deleted, &newer_edit, Some("resurrected".into())),
            deleted,
            "a Deleted record must never be resurrected, even without Tombstoned status"
        );
    }

    #[test]
    fn apply_event_replaces_content_per_content_rule() {
        let created = event_data(
            ObservedEventType::Created,
            &hex64(0xaa),
            T0,
            ChatChannelKind::Workspace,
        );
        let original = project_record(
            tenant(),
            workspace(),
            &created,
            &policy(true),
            Some("v1".into()),
        )
        .unwrap();

        // New edit without a body copy: content is dropped, status downgrades.
        let edit = event_data(
            ObservedEventType::Edited,
            &hex64(0xee),
            T0 + 10,
            ChatChannelKind::Workspace,
        );
        let updated = apply_event(&original, &edit, None);
        assert_eq!(updated.content, None);
        assert_eq!(updated.indexing_status, IndexingStatus::ReferenceOnly);
        // content_indexing reflects the stored copy; an edit without a body
        // copy downgrades it to false.
        assert!(!updated.content_indexing);

        // Reference-only record gaining a body on edit.
        let reference_only =
            project_record(tenant(), workspace(), &created, &policy(false), None).unwrap();
        let with_body = apply_event(&reference_only, &edit, Some("v2".into()));
        assert!(with_body.content_indexing);
        assert_eq!(with_body.content.as_deref(), Some("v2"));
        assert_eq!(with_body.indexing_status, IndexingStatus::ContentStored);
    }

    #[test]
    fn apply_tombstone_sets_tombstone_state() {
        let created = event_data(
            ObservedEventType::Created,
            &hex64(0xaa),
            T0,
            ChatChannelKind::Workspace,
        );
        let original = project_record(
            tenant(),
            workspace(),
            &created,
            &policy(true),
            Some("v1".into()),
        )
        .unwrap();
        let deleted = event_data(
            ObservedEventType::Deleted,
            &hex64(0xdd),
            T0 + 20,
            ChatChannelKind::Workspace,
        );
        let tombstoned = apply_tombstone(&original, &deleted);

        assert_eq!(tombstoned.event_type, ObservedEventType::Deleted);
        assert_eq!(tombstoned.indexing_status, IndexingStatus::Tombstoned);
        assert_eq!(tombstoned.tombstoned_at, Some(deleted.observed_at));
        assert_eq!(tombstoned.latest_event_id, hex64(0xdd));
        assert_eq!(tombstoned.checksum, deleted.buzz.checksum);
        assert_eq!(tombstoned.signature, deleted.buzz.signature);
        assert_eq!(tombstoned.occurred_at, deleted.buzz.created_at);
        assert_eq!(tombstoned.observed_at, deleted.observed_at);
        assert_eq!(tombstoned.updated_at, deleted.observed_at);
        assert_eq!(tombstoned.provenance.len(), 2);
        assert_eq!(
            tombstoned.provenance[1].event_type,
            ObservedEventType::Deleted
        );
        // Identity fields preserved; stored content retained for retention policy.
        assert_eq!(tombstoned.record_id, original.record_id);
        assert_eq!(tombstoned.tenant_id, original.tenant_id);
        assert_eq!(tombstoned.content.as_deref(), Some("v1"));
        assert_eq!(tombstoned.created_at, original.created_at);
    }

    #[test]
    fn apply_tombstone_is_deterministic_and_always_applies() {
        let created = event_data(
            ObservedEventType::Created,
            &hex64(0xaa),
            T0,
            ChatChannelKind::Workspace,
        );
        let original =
            project_record(tenant(), workspace(), &created, &policy(false), None).unwrap();
        let deleted = event_data(
            ObservedEventType::Deleted,
            &hex64(0xdd),
            T0 + 20,
            ChatChannelKind::Workspace,
        );
        let a = apply_tombstone(&original, &deleted);
        let b = apply_tombstone(&original, &deleted);
        assert_eq!(a, b);

        // Applying a second tombstone always applies (provenance grows).
        let second = event_data(
            ObservedEventType::Deleted,
            &hex64(0xd2),
            T0 + 40,
            ChatChannelKind::Workspace,
        );
        let re_tombstoned = apply_tombstone(&a, &second);
        assert_eq!(re_tombstoned.provenance.len(), 3);
        assert_eq!(re_tombstoned.latest_event_id, hex64(0xd2));
        assert_eq!(re_tombstoned.indexing_status, IndexingStatus::Tombstoned);
        assert_eq!(re_tombstoned.tombstoned_at, Some(second.observed_at));
    }

    #[test]
    fn project_record_ids_are_unique_across_calls() {
        let event = event_data(
            ObservedEventType::Created,
            &hex64(0xaa),
            T0,
            ChatChannelKind::Workspace,
        );
        let mut ids = HashSet::new();
        for _ in 0..16 {
            let record =
                project_record(tenant(), workspace(), &event, &policy(false), None).unwrap();
            assert!(ids.insert(record.record_id), "record_id must be unique");
        }
    }
}
