//! Integration tests for the Chat observation store and the Memory catalog
//! store (ADR-0033/ADR-0034 storage layer).
//!
//! DB-backed and `#[ignore]`d; run against the dev database (migrations
//! applied, including `20260810000004`) with `--test-threads=1`:
//!
//!   set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true \
//!     cargo test --test chat_observation_memory_catalog_test \
//!     -- --ignored --test-threads=1
//!
//! The chat tables are tenant-scoped and the consumer receipts are
//! consumer-scoped, so every test takes a shared `SERIAL` guard and cleans up
//! exactly the rows it created (same convention as the outbox suite).

use rustshare_core::domain::{PrincipalId, TenantId, WorkspaceId};
use rustshare_integration_events::event_types::CHAT_BUZZ_EVENT_OBSERVED_V1;
use rustshare_integration_events::{ActorRef, IntegrationEvent};
use rustshare_memory::event::{
    BuzzEventMeta, ChatChannelKind, ChatContext, ObservedChatEventData, ObservedEventType,
    PrincipalMeta,
};
use rustshare_memory::observed::ChatObservedEvent;
use rustshare_memory::policy::ProjectionPolicy;
use rustshare_memory::project::project_record;
use rustshare_memory::record::IndexingStatus;
use rustshare_resource_auth::BindingStatus;
use rustshare_storage::{ChatIdentityStore, ChatObservationStore, MemoryCatalogStore};
use sqlx::PgPool;
use std::sync::LazyLock;
use uuid::Uuid;

/// Serializes the tests within this binary (same convention as the outbox
/// suite): the chat tables are tenant-scoped and receipts consumer-scoped, so
/// this is belt-and-suspenders against leaked rows between tests.
static SERIAL: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

/// Fully-enabled policy for the store-level lifecycle tests; policy handling
/// itself is exercised explicitly by
/// `memory_catalog_policy_disabled_produces_no_record` and the
/// never-eligible-channel cases.
const ENABLED_POLICY: ProjectionPolicy = ProjectionPolicy {
    memory_projection: true,
    content_indexing: true,
};

/// Shared pool over `DATABASE_URL` with the same fallback the storage-layer
/// tests use.
async fn pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    PgPool::connect(&database_url)
        .await
        .expect("failed to connect to the dev database")
}

/// Per-test cleanup: every chat table is tenant-scoped; receipts are
/// consumer-scoped.
async fn cleanup(pool: &PgPool, tenant_id: TenantId, consumer_id: &str) {
    for table in [
        "memory_catalog",
        "chat_observed_events",
        "chat_buzz_admissions",
        "chat_workspace_communities",
        "chat_identity_bindings",
        "application_enablements",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE tenant_id = $1"))
            .bind(tenant_id.0)
            .execute(pool)
            .await
            .unwrap();
    }
    sqlx::query("DELETE FROM integration_consumer_receipts WHERE consumer_id = $1")
        .bind(consumer_id)
        .execute(pool)
        .await
        .unwrap();
}

fn hex64(seed: u8) -> String {
    format!("{seed:02x}").repeat(32)
}

fn tenant() -> TenantId {
    TenantId::from(Uuid::new_v4())
}

fn event_data(
    event_type: ObservedEventType,
    event_id: &str,
    message_id: &str,
    created_at_ts: i64,
    channel_kind: ChatChannelKind,
) -> ObservedChatEventData {
    ObservedChatEventData {
        buzz: BuzzEventMeta {
            event_id: event_id.to_string(),
            message_id: message_id.to_string(),
            event_type,
            supersedes_event_id: None,
            created_at: chrono::DateTime::<chrono::Utc>::from_timestamp(created_at_ts, 0).unwrap(),
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
        observed_at: chrono::DateTime::<chrono::Utc>::from_timestamp(created_at_ts + 100, 0)
            .unwrap(),
    }
}

/// A valid `io.elembra.chat.buzz.event.observed.v1` envelope carrying `data`
/// (tenant == workspace, per the platform invariant).
fn integration_event(tenant_id: TenantId, data: &ObservedChatEventData) -> IntegrationEvent {
    IntegrationEvent::builder()
        .source("elembra://io.elembra.chat")
        .r#type(CHAT_BUZZ_EVENT_OBSERVED_V1)
        .tenant_id(tenant_id)
        .workspace_id(WorkspaceId(tenant_id.0))
        .actor(ActorRef::Principal(data.principal.principal_id))
        .data(serde_json::to_value(data).unwrap())
        .build()
        .unwrap()
}

async fn insert_binding(pool: &PgPool, tenant_id: TenantId, pubkey: &str) -> Uuid {
    let binding_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_identity_bindings
            (binding_id, tenant_id, principal_id, buzz_pubkey, status, verified_at, audit_metadata)
         VALUES ($1, $2, $3, $4, 'active', now(), '{}'::jsonb)",
    )
    .bind(binding_id)
    .bind(tenant_id.0)
    .bind(PrincipalId::from(Uuid::new_v4()).0)
    .bind(pubkey)
    .execute(pool)
    .await
    .unwrap();
    binding_id
}

async fn insert_mapping(
    pool: &PgPool,
    tenant_id: TenantId,
    workspace_id: WorkspaceId,
    community_id: &str,
    active: bool,
) -> Uuid {
    let mapping_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_workspace_communities
            (mapping_id, tenant_id, workspace_id, community_id, relay_url, active)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(mapping_id)
    .bind(tenant_id.0)
    .bind(workspace_id.0)
    .bind(community_id)
    .bind("wss://relay.example.test")
    .bind(active)
    .execute(pool)
    .await
    .unwrap();
    mapping_id
}

async fn insert_admission(
    pool: &PgPool,
    tenant_id: TenantId,
    mapping_id: Uuid,
    binding_id: Uuid,
    pubkey: &str,
    active: bool,
) -> Uuid {
    let admission_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_buzz_admissions
            (admission_id, tenant_id, mapping_id, binding_id, buzz_pubkey, active)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(admission_id)
    .bind(tenant_id.0)
    .bind(mapping_id)
    .bind(binding_id)
    .bind(pubkey)
    .bind(active)
    .execute(pool)
    .await
    .unwrap();
    admission_id
}

// ---------------------------------------------------------------------------
// 1. ChatObservationStore::upsert_event_in_tx: idempotent by (tenant, event_id)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL (dev database with migrations applied)"]
async fn chat_observation_upsert_is_idempotent_by_event_id() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = tenant();
    let store = ChatObservationStore::new(pool.clone());
    let message_id = hex64(0xaa);

    let mut tx = pool.begin().await.unwrap();
    let created_data = event_data(
        ObservedEventType::Created,
        &message_id,
        &message_id,
        1_752_000_000,
        ChatChannelKind::Workspace,
    );
    let created = ChatObservedEvent::from_observed_data(
        tenant,
        WorkspaceId(tenant.0),
        &created_data,
        Some("v1".into()),
    );
    assert!(
        store.upsert_event_in_tx(&mut tx, &created).await.unwrap(),
        "first insert of an event_id must insert"
    );
    assert!(
        !store.upsert_event_in_tx(&mut tx, &created).await.unwrap(),
        "re-inserting the identical (tenant, event_id) must be a no-op"
    );

    let edit_data = event_data(
        ObservedEventType::Edited,
        &hex64(0xee),
        &message_id,
        1_752_000_010,
        ChatChannelKind::Workspace,
    );
    let edit = ChatObservedEvent::from_observed_data(
        tenant,
        WorkspaceId(tenant.0),
        &edit_data,
        Some("v2".into()),
    );
    assert!(
        store.upsert_event_in_tx(&mut tx, &edit).await.unwrap(),
        "a different event_id inserts"
    );
    tx.commit().await.unwrap();

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM chat_observed_events WHERE tenant_id = $1",
    )
    .bind(tenant.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        count, 2,
        "created + edited rows, never a duplicate of the created event"
    );

    cleanup(&pool, tenant, "unused-consumer").await;
}

// ---------------------------------------------------------------------------
// 2. ChatObservationStore::lookup_for_auth: latest event, None for unknown
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn chat_observation_lookup_for_auth_returns_latest_event() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = tenant();
    let store = ChatObservationStore::new(pool.clone());
    let message_id = hex64(0xaa);

    let mut tx = pool.begin().await.unwrap();
    for (event_type, event_id, ts) in [
        (
            ObservedEventType::Created,
            message_id.clone(),
            1_752_000_000i64,
        ),
        (ObservedEventType::Edited, hex64(0xee), 1_752_000_010),
    ] {
        let data = event_data(
            event_type,
            &event_id,
            &message_id,
            ts,
            ChatChannelKind::Workspace,
        );
        let observed = ChatObservedEvent::from_observed_data(
            tenant,
            WorkspaceId(tenant.0),
            &data,
            if event_type == ObservedEventType::Created {
                Some("v1".into())
            } else {
                Some("v2".into())
            },
        );
        store.upsert_event_in_tx(&mut tx, &observed).await.unwrap();
    }
    tx.commit().await.unwrap();

    let latest = store
        .lookup_for_auth(tenant, &message_id)
        .await
        .unwrap()
        .expect("known message must have a latest row");
    assert_eq!(latest.event_id, hex64(0xee), "latest by event_created_at");
    assert_eq!(latest.event_type, ObservedEventType::Edited);
    assert_eq!(latest.body.as_deref(), Some("v2"));
    assert!(latest.active);

    assert!(
        store
            .lookup_for_auth(tenant, &hex64(0xff))
            .await
            .unwrap()
            .is_none(),
        "unknown message must yield None"
    );

    cleanup(&pool, tenant, "unused-consumer").await;
}

// ---------------------------------------------------------------------------
// 3. ChatObservationStore::list_for_reconcile: since filter + ordering
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn chat_observation_list_for_reconcile_respects_since_and_order() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = tenant();
    let store = ChatObservationStore::new(pool.clone());
    let message_id = hex64(0xaa);
    let t0 = 1_752_000_000i64;

    let mut tx = pool.begin().await.unwrap();
    let events = [
        (ObservedEventType::Created, message_id.clone(), t0),
        (ObservedEventType::Edited, hex64(0xee), t0 + 10),
        (ObservedEventType::Edited, hex64(0xef), t0 + 20),
    ];
    for (event_type, event_id, ts) in events {
        let data = event_data(
            event_type,
            &event_id,
            &message_id,
            ts,
            ChatChannelKind::Workspace,
        );
        let observed =
            ChatObservedEvent::from_observed_data(tenant, WorkspaceId(tenant.0), &data, None);
        store.upsert_event_in_tx(&mut tx, &observed).await.unwrap();
    }
    tx.commit().await.unwrap();

    let all = store.list_for_reconcile(tenant, None).await.unwrap();
    let ids: Vec<&str> = all.iter().map(|e| e.event_id.as_str()).collect();
    assert_eq!(
        ids,
        [
            message_id.as_str(),
            hex64(0xee).as_str(),
            hex64(0xef).as_str()
        ],
        "oldest first"
    );

    let since = store
        .list_for_reconcile(
            tenant,
            Some(chrono::DateTime::<chrono::Utc>::from_timestamp(t0 + 10, 0).unwrap()),
        )
        .await
        .unwrap();
    let since_ids: Vec<&str> = since.iter().map(|e| e.event_id.as_str()).collect();
    assert_eq!(
        since_ids,
        [hex64(0xee).as_str(), hex64(0xef).as_str()],
        "event_created_at >= since"
    );

    cleanup(&pool, tenant, "unused-consumer").await;
}

// ---------------------------------------------------------------------------
// 3b. ChatObservationStore::get_by_event_id: point lookup by (tenant, event_id)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn chat_observation_get_by_event_id_returns_specific_event() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = tenant();
    let store = ChatObservationStore::new(pool.clone());
    let message_id = hex64(0xaa);

    let mut tx = pool.begin().await.unwrap();
    let created_id = message_id.clone();
    let created_data = event_data(
        ObservedEventType::Created,
        &created_id,
        &message_id,
        1_752_000_000,
        ChatChannelKind::Workspace,
    );
    let created = ChatObservedEvent::from_observed_data(
        tenant,
        WorkspaceId(tenant.0),
        &created_data,
        Some("v1".into()),
    );
    store.upsert_event_in_tx(&mut tx, &created).await.unwrap();
    let edit_id = hex64(0xee);
    let edit_data = event_data(
        ObservedEventType::Edited,
        &edit_id,
        &message_id,
        1_752_000_010,
        ChatChannelKind::Workspace,
    );
    let edit = ChatObservedEvent::from_observed_data(
        tenant,
        WorkspaceId(tenant.0),
        &edit_data,
        Some("v2".into()),
    );
    store.upsert_event_in_tx(&mut tx, &edit).await.unwrap();
    tx.commit().await.unwrap();

    let found = store
        .get_by_event_id(tenant, &edit_id)
        .await
        .unwrap()
        .expect("an observed event must be found by id");
    assert_eq!(found.event_id, edit_id);
    assert_eq!(found.message_id, message_id);
    assert_eq!(found.event_type, ObservedEventType::Edited);
    assert_eq!(found.body.as_deref(), Some("v2"), "body round-trips");

    assert!(
        store
            .get_by_event_id(tenant, &hex64(0xff))
            .await
            .unwrap()
            .is_none(),
        "unknown event id must yield None"
    );
    assert!(
        store
            .get_by_event_id(TenantId::from(Uuid::new_v4()), &edit_id)
            .await
            .unwrap()
            .is_none(),
        "rows are tenant-scoped"
    );

    cleanup(&pool, tenant, "unused-consumer").await;
}

// ---------------------------------------------------------------------------
// 4. ChatIdentityStore::binding_by_pubkey: live binding, None after revoke
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn chat_identity_binding_by_pubkey_live_and_after_revoke() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = tenant();
    let store = ChatIdentityStore::new(pool.clone());
    let pubkey = hex64(0xbb);

    let binding_id = insert_binding(&pool, tenant, &pubkey).await;

    let found = store
        .binding_by_pubkey(tenant, &pubkey)
        .await
        .unwrap()
        .expect("live binding must be found by pubkey");
    assert_eq!(found.binding_id, binding_id);
    assert_eq!(found.buzz_pubkey, pubkey);
    assert_eq!(found.status, BindingStatus::Active);
    assert!(found.revoked_at.is_none());

    assert!(
        store
            .binding_by_pubkey(TenantId::from(Uuid::new_v4()), &pubkey)
            .await
            .unwrap()
            .is_none(),
        "bindings are tenant-scoped"
    );

    // Revoke: the live index excludes the row, so the lookup returns None.
    sqlx::query(
        "UPDATE chat_identity_bindings SET status = 'revoked', revoked_at = now() WHERE binding_id = $1",
    )
    .bind(binding_id)
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        store
            .binding_by_pubkey(tenant, &pubkey)
            .await
            .unwrap()
            .is_none(),
        "revoked binding must not be returned"
    );

    cleanup(&pool, tenant, "unused-consumer").await;
}

// ---------------------------------------------------------------------------
// 5. ChatIdentityStore::active_admission: admission + mapping both active
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn chat_identity_active_admission_requires_active_admission_and_mapping() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = tenant();
    let workspace = WorkspaceId(tenant.0);
    let store = ChatIdentityStore::new(pool.clone());
    let pubkey = hex64(0xbb);

    let mapping_id = insert_mapping(&pool, tenant, workspace, "community-1", true).await;
    let binding_id = insert_binding(&pool, tenant, &pubkey).await;
    let admission_id = insert_admission(&pool, tenant, mapping_id, binding_id, &pubkey, true).await;

    assert!(store
        .active_admission(tenant, "community-1", &pubkey)
        .await
        .unwrap());
    assert!(
        !store
            .active_admission(tenant, "other-community", &pubkey)
            .await
            .unwrap(),
        "wrong community must fail"
    );
    assert!(
        !store
            .active_admission(tenant, "community-1", &hex64(0xcc))
            .await
            .unwrap(),
        "unadmitted pubkey must fail"
    );

    // Inactive admission → false.
    sqlx::query(
        "UPDATE chat_buzz_admissions SET active = false WHERE tenant_id = $1 AND admission_id = $2",
    )
    .bind(tenant.0)
    .bind(admission_id)
    .execute(&pool)
    .await
    .unwrap();
    assert!(!store
        .active_admission(tenant, "community-1", &pubkey)
        .await
        .unwrap());

    // Reactivate the admission, deactivate the mapping → false.
    sqlx::query(
        "UPDATE chat_buzz_admissions SET active = true WHERE tenant_id = $1 AND admission_id = $2",
    )
    .bind(tenant.0)
    .bind(admission_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE chat_workspace_communities SET active = false WHERE tenant_id = $1 AND mapping_id = $2",
    )
    .bind(tenant.0)
    .bind(mapping_id)
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        !store
            .active_admission(tenant, "community-1", &pubkey)
            .await
            .unwrap(),
        "inactive mapping must fail even with an active admission"
    );

    cleanup(&pool, tenant, "unused-consumer").await;
}

// ---------------------------------------------------------------------------
// 6. MemoryCatalogStore::upsert_from_event_in_tx: full event lifecycle
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn memory_catalog_upsert_from_event_full_lifecycle() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = tenant();
    let store = MemoryCatalogStore::new(pool.clone());
    let consumer_id = format!("io.elembra.test.memory-catalog-{}", Uuid::new_v4());
    let message_id = hex64(0xaa);

    // (a) First event → creates exactly one record.
    let created_data = event_data(
        ObservedEventType::Created,
        &message_id,
        &message_id,
        1_752_000_000,
        ChatChannelKind::Workspace,
    );
    let created_event = integration_event(tenant, &created_data);
    let record = {
        let mut tx = pool.begin().await.unwrap();
        let record = store
            .upsert_from_event_in_tx(
                &mut tx,
                &consumer_id,
                &created_event,
                &created_data,
                &ENABLED_POLICY,
                Some("v1".into()),
            )
            .await
            .unwrap()
            .expect("first event must create a record");
        tx.commit().await.unwrap();
        record
    };
    assert_eq!(record.message_id, message_id);
    assert_eq!(record.event_type, ObservedEventType::Created);
    assert_eq!(record.content.as_deref(), Some("v1"));
    assert!(record.content_indexing);
    assert_eq!(record.indexing_status, IndexingStatus::ContentStored);
    assert_eq!(record.provenance.len(), 1);
    assert_eq!(record.latest_event_id, message_id);
    assert_eq!(store.count_for_tenant(tenant).await.unwrap(), 1);

    // (b) Duplicate delivery of the SAME integration event → None, no change.
    let duplicate = {
        let mut tx = pool.begin().await.unwrap();
        let result = store
            .upsert_from_event_in_tx(
                &mut tx,
                &consumer_id,
                &created_event,
                &created_data,
                &ENABLED_POLICY,
                Some("v1".into()),
            )
            .await
            .unwrap();
        tx.commit().await.unwrap();
        result
    };
    assert!(duplicate.is_none(), "duplicate delivery must return None");
    assert_eq!(store.count_for_tenant(tenant).await.unwrap(), 1);
    let after_duplicate = store.get(tenant, &message_id).await.unwrap().unwrap();
    assert_eq!(after_duplicate.provenance.len(), 1);
    assert_eq!(after_duplicate.latest_event_id, message_id);

    // (c) Edit (later created_at, new event_id) → same record updated.
    let edit_id = hex64(0xee);
    let edit_data = event_data(
        ObservedEventType::Edited,
        &edit_id,
        &message_id,
        1_752_000_010,
        ChatChannelKind::Workspace,
    );
    let edit_event = integration_event(tenant, &edit_data);
    let edited = {
        let mut tx = pool.begin().await.unwrap();
        let edited = store
            .upsert_from_event_in_tx(
                &mut tx,
                &consumer_id,
                &edit_event,
                &edit_data,
                &ENABLED_POLICY,
                Some("v2".into()),
            )
            .await
            .unwrap()
            .expect("edit must apply");
        tx.commit().await.unwrap();
        edited
    };
    assert_eq!(edited.record_id, record.record_id, "one record per message");
    assert_eq!(edited.latest_event_id, edit_id);
    assert_eq!(edited.provenance.len(), 2);
    assert_eq!(edited.content.as_deref(), Some("v2"));
    assert_eq!(store.count_for_tenant(tenant).await.unwrap(), 1);

    // (d) Out-of-order edit (earlier created_at) → record unchanged.
    let oob_id = hex64(0x0b);
    let oob_data = event_data(
        ObservedEventType::Edited,
        &oob_id,
        &message_id,
        1_752_000_005,
        ChatChannelKind::Workspace,
    );
    let oob_event = integration_event(tenant, &oob_data);
    let unchanged = {
        let mut tx = pool.begin().await.unwrap();
        let result = store
            .upsert_from_event_in_tx(
                &mut tx,
                &consumer_id,
                &oob_event,
                &oob_data,
                &ENABLED_POLICY,
                Some("oob".into()),
            )
            .await
            .unwrap()
            .expect("out-of-order edit still consumes the event but must not regress the record");
        tx.commit().await.unwrap();
        result
    };
    assert_eq!(
        unchanged, edited,
        "an older edit must never regress the record"
    );
    assert_eq!(unchanged.latest_event_id, edit_id);
    assert_eq!(unchanged.provenance.len(), 2);
    assert_eq!(unchanged.content.as_deref(), Some("v2"));

    // (e) Deleted → record tombstoned.
    let deleted_id = hex64(0xdd);
    let deleted_data = event_data(
        ObservedEventType::Deleted,
        &deleted_id,
        &message_id,
        1_752_000_020,
        ChatChannelKind::Workspace,
    );
    let deleted_event = integration_event(tenant, &deleted_data);
    let tombstoned = {
        let mut tx = pool.begin().await.unwrap();
        let tombstoned = store
            .upsert_from_event_in_tx(
                &mut tx,
                &consumer_id,
                &deleted_event,
                &deleted_data,
                &ENABLED_POLICY,
                None,
            )
            .await
            .unwrap()
            .expect("deletion must apply");
        tx.commit().await.unwrap();
        tombstoned
    };
    assert_eq!(tombstoned.event_type, ObservedEventType::Deleted);
    assert_eq!(tombstoned.indexing_status, IndexingStatus::Tombstoned);
    assert!(tombstoned.tombstoned_at.is_some());
    assert_eq!(tombstoned.latest_event_id, deleted_id);
    assert_eq!(tombstoned.provenance.len(), 3);
    assert_eq!(store.count_for_tenant(tenant).await.unwrap(), 1);

    // The persisted row matches what was returned.
    let persisted = store.get(tenant, &message_id).await.unwrap().unwrap();
    assert_eq!(persisted, tombstoned);

    cleanup(&pool, tenant, &consumer_id).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn memory_catalog_deleted_without_prior_record_is_no_op() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = tenant();
    let store = MemoryCatalogStore::new(pool.clone());
    let consumer_id = format!("io.elembra.test.memory-catalog-{}", Uuid::new_v4());
    let message_id = hex64(0xaa);
    let deleted_id = hex64(0xdd);

    // A Deleted event for a message that was never projected: consumed, but
    // must not materialize a catalog record (no row, no effect).
    let deleted_data = event_data(
        ObservedEventType::Deleted,
        &deleted_id,
        &message_id,
        1_752_000_020,
        ChatChannelKind::Workspace,
    );
    let deleted_event = integration_event(tenant, &deleted_data);
    let result = {
        let mut tx = pool.begin().await.unwrap();
        let result = store
            .upsert_from_event_in_tx(
                &mut tx,
                &consumer_id,
                &deleted_event,
                &deleted_data,
                &ENABLED_POLICY,
                None,
            )
            .await
            .unwrap();
        tx.commit().await.unwrap();
        result
    };
    assert!(
        result.is_none(),
        "a tombstone for an unprojected message must be a no-op"
    );
    assert_eq!(
        store.count_for_tenant(tenant).await.unwrap(),
        0,
        "no catalog row may be created"
    );
    assert!(
        store.get(tenant, &message_id).await.unwrap().is_none(),
        "no record exists for the message"
    );

    // The receipt was still written (the event was processed; its effect is
    // "nothing"), so a redelivery is a duplicate — still None, still no row.
    let redelivery = {
        let mut tx = pool.begin().await.unwrap();
        let result = store
            .upsert_from_event_in_tx(
                &mut tx,
                &consumer_id,
                &deleted_event,
                &deleted_data,
                &ENABLED_POLICY,
                None,
            )
            .await
            .unwrap();
        tx.commit().await.unwrap();
        result
    };
    assert!(redelivery.is_none(), "redelivery is a duplicate no-op");
    assert_eq!(store.count_for_tenant(tenant).await.unwrap(), 0);

    let receipts: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM integration_consumer_receipts WHERE consumer_id = $1",
    )
    .bind(&consumer_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(receipts, 1, "exactly one receipt for the tombstone");

    cleanup(&pool, tenant, &consumer_id).await;
}

// ---------------------------------------------------------------------------
// 6b. MemoryCatalogStore::upsert_from_event_in_tx: per-tenant policy gate
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn memory_catalog_policy_disabled_produces_no_record() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = tenant();
    let store = MemoryCatalogStore::new(pool.clone());
    let consumer_id = format!("io.elembra.test.memory-catalog-{}", Uuid::new_v4());
    let message_id = hex64(0xaa);

    // Projection disabled (`memory_projection: false`): a Workspace created
    // event is consumed but produces no catalog record.
    let created_data = event_data(
        ObservedEventType::Created,
        &message_id,
        &message_id,
        1_752_000_000,
        ChatChannelKind::Workspace,
    );
    let created_event = integration_event(tenant, &created_data);
    let disabled = ProjectionPolicy {
        memory_projection: false,
        content_indexing: false,
    };
    let result = {
        let mut tx = pool.begin().await.unwrap();
        let result = store
            .upsert_from_event_in_tx(
                &mut tx,
                &consumer_id,
                &created_event,
                &created_data,
                &disabled,
                Some("v1".into()),
            )
            .await
            .unwrap();
        tx.commit().await.unwrap();
        result
    };
    assert!(
        result.is_none(),
        "a disabled policy must consume the event without projecting it"
    );
    assert_eq!(
        store.count_for_tenant(tenant).await.unwrap(),
        0,
        "no catalog row may be created"
    );
    assert!(
        store.get(tenant, &message_id).await.unwrap().is_none(),
        "no record exists for the message"
    );

    // The receipt was still written: the event was durably processed, its
    // effect is "nothing" (that event will never produce a record).
    let receipts: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM integration_consumer_receipts WHERE consumer_id = $1",
    )
    .bind(&consumer_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        receipts, 1,
        "a policy-skipped event still leaves exactly one receipt"
    );

    cleanup(&pool, tenant, &consumer_id).await;
}

// ---------------------------------------------------------------------------
// 7. MemoryCatalogStore::upsert_records: reconciliation insert/update
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn memory_catalog_upsert_records_reconciles_without_duplicates() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = tenant();
    let store = MemoryCatalogStore::new(pool.clone());
    let policy = ProjectionPolicy {
        memory_projection: true,
        content_indexing: true,
    };

    let m1 = hex64(0xa1);
    let m2 = hex64(0xa2);
    let d1 = event_data(
        ObservedEventType::Created,
        &m1,
        &m1,
        1_752_000_000,
        ChatChannelKind::Workspace,
    );
    let d2 = event_data(
        ObservedEventType::Created,
        &m2,
        &m2,
        1_752_000_000,
        ChatChannelKind::Workspace,
    );
    let r1 = project_record(
        tenant,
        WorkspaceId(tenant.0),
        &d1,
        &policy,
        Some("m1".into()),
    )
    .unwrap();
    let r2 = project_record(
        tenant,
        WorkspaceId(tenant.0),
        &d2,
        &policy,
        Some("m2".into()),
    )
    .unwrap();

    let first = store
        .upsert_records(std::slice::from_ref(&r1))
        .await
        .unwrap();
    assert_eq!(
        first,
        rustshare_storage::ReconcileCounts {
            processed: 1,
            created: 1,
            updated: 0
        }
    );

    let second = store
        .upsert_records(std::slice::from_ref(&r1))
        .await
        .unwrap();
    assert_eq!(
        second,
        rustshare_storage::ReconcileCounts {
            processed: 1,
            created: 0,
            updated: 1
        }
    );

    let third = store
        .upsert_records(&[r1.clone(), r2.clone()])
        .await
        .unwrap();
    assert_eq!(
        third,
        rustshare_storage::ReconcileCounts {
            processed: 2,
            created: 1,
            updated: 1
        }
    );

    assert_eq!(
        store.count_for_tenant(tenant).await.unwrap(),
        2,
        "reconciliation must never duplicate a message"
    );
    assert_eq!(
        store.get(tenant, &m1).await.unwrap().unwrap(),
        r1,
        "record round-trips"
    );
    assert_eq!(store.get(tenant, &m2).await.unwrap().unwrap(), r2);

    cleanup(&pool, tenant, "unused-consumer").await;
}

// ---------------------------------------------------------------------------
// 8. MemoryCatalogStore::get / count_for_tenant scoping
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn memory_catalog_get_and_count_are_tenant_scoped() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant_a = tenant();
    let tenant_b = tenant();
    let store = MemoryCatalogStore::new(pool.clone());
    let policy = ProjectionPolicy {
        memory_projection: true,
        content_indexing: false,
    };

    let m1 = hex64(0xa1);
    let m2 = hex64(0xa2);
    let d1 = event_data(
        ObservedEventType::Created,
        &m1,
        &m1,
        1_752_000_000,
        ChatChannelKind::Workspace,
    );
    let d2 = event_data(
        ObservedEventType::Created,
        &m2,
        &m2,
        1_752_000_000,
        ChatChannelKind::Workspace,
    );
    let r1 = project_record(tenant_a, WorkspaceId(tenant_a.0), &d1, &policy, None).unwrap();
    let r2 = project_record(tenant_a, WorkspaceId(tenant_a.0), &d2, &policy, None).unwrap();
    store.upsert_records(&[r1, r2]).await.unwrap();

    assert_eq!(store.count_for_tenant(tenant_a).await.unwrap(), 2);
    assert_eq!(store.count_for_tenant(tenant_b).await.unwrap(), 0);

    let got = store
        .get(tenant_a, &m1)
        .await
        .unwrap()
        .expect("record exists in tenant_a");
    assert_eq!(got.message_id, m1);
    assert!(store.get(tenant_a, &hex64(0xff)).await.unwrap().is_none());
    assert!(
        store.get(tenant_b, &m1).await.unwrap().is_none(),
        "records are tenant-scoped"
    );

    cleanup(&pool, tenant_a, "unused-consumer").await;
    cleanup(&pool, tenant_b, "unused-consumer").await;
}

// ---------------------------------------------------------------------------
// 9. ChatIdentityStore::mapping_by_community: active mapping by community id
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn chat_identity_mapping_by_community_returns_active_mapping_only() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let store = ChatIdentityStore::new(pool.clone());
    let tenant_a = tenant();
    let tenant_b = tenant();

    let mapping_id_a = insert_mapping(
        &pool,
        tenant_a,
        WorkspaceId(tenant_a.0),
        "community-1",
        true,
    )
    .await;
    insert_mapping(
        &pool,
        tenant_b,
        WorkspaceId(tenant_b.0),
        "community-1",
        false,
    )
    .await;

    let found = store
        .mapping_by_community("community-1")
        .await
        .unwrap()
        .expect("the active mapping must be found");
    assert_eq!(found.tenant_id, tenant_a);
    assert_eq!(found.workspace_id, WorkspaceId(tenant_a.0));
    assert_eq!(found.community_id, "community-1");
    assert!(found.active);
    assert_eq!(found.relay_url, "wss://relay.example.test");

    // Deactivate tenant_a's mapping: no active mapping remains → None.
    sqlx::query(
        "UPDATE chat_workspace_communities SET active = false WHERE tenant_id = $1 AND mapping_id = $2",
    )
    .bind(tenant_a.0)
    .bind(mapping_id_a)
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        store
            .mapping_by_community("community-1")
            .await
            .unwrap()
            .is_none(),
        "an inactive mapping must never be returned"
    );

    cleanup(&pool, tenant_a, "unused-consumer").await;
    cleanup(&pool, tenant_b, "unused-consumer").await;
}

// ---------------------------------------------------------------------------
// 10. ChatIdentityStore::projection_policy: JSONB configuration flags
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn chat_identity_projection_policy_reads_configuration() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let store = ChatIdentityStore::new(pool.clone());
    let tenant = tenant();
    let workspace = WorkspaceId(tenant.0);

    // Absent configuration row ⇒ defaults (both flags false).
    let defaults = store.projection_policy(tenant, workspace).await.unwrap();
    assert!(!defaults.memory_projection);
    assert!(!defaults.content_indexing);

    // Explicit boolean flags in the chat Application's configuration JSONB.
    sqlx::query(
        "INSERT INTO application_enablements
            (tenant_id, workspace_id, application_id, enabled, configuration)
         VALUES ($1, $2, 'io.elembra.chat', true,
                 '{\"memory_projection\": true, \"content_indexing\": true}'::jsonb)",
    )
    .bind(tenant.0)
    .bind(workspace.0)
    .execute(&pool)
    .await
    .unwrap();
    let enabled = store.projection_policy(tenant, workspace).await.unwrap();
    assert!(enabled.memory_projection);
    assert!(enabled.content_indexing);

    // Non-boolean flag values fail closed to false.
    sqlx::query(
        "UPDATE application_enablements
         SET configuration = '{\"memory_projection\": \"yes\"}'::jsonb
         WHERE tenant_id = $1 AND workspace_id = $2 AND application_id = 'io.elembra.chat'",
    )
    .bind(tenant.0)
    .bind(workspace.0)
    .execute(&pool)
    .await
    .unwrap();
    let partial = store.projection_policy(tenant, workspace).await.unwrap();
    assert!(
        !partial.memory_projection,
        "non-boolean value must fail closed"
    );
    assert!(!partial.content_indexing);

    cleanup(&pool, tenant, "unused-consumer").await;
}
