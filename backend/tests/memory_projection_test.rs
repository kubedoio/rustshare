//! Integration tests for the Memory chat projection consumer
//! ([`rustshare_server::memory_projection::MemoryChatProjectionConsumer`])
//! end to end against the dev database: the real
//! [`BuzzObservationService`](rustshare_server::buzz_observation::BuzzObservationService)
//! publishes the durable `io.elembra.chat.buzz.event.observed.v1` event, and
//! the consumer projects it into `memory_catalog` idempotently under the
//! tenant's projection policy.
//!
//! DB-backed and `#[ignore]`d; run against the dev database (migrations
//! applied, including `20260810000005`) with `--test-threads=1`:
//!
//!   set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true \
//!     cargo test --test memory_projection_test -- --ignored --test-threads=1
//!
//! The chat tables are tenant-scoped and the outbox/delivery/receipt tables
//! are process-global, so every test takes a shared `SERIAL` guard and cleans
//! up exactly the rows it created (same convention as the outbox suite).

use chrono::Utc;
use nostr::{EventBuilder, Keys, Timestamp};
use rustshare_core::domain::{ApplicationRegistry, PrincipalId, TenantId};
use rustshare_crypto::WebhookSigner;
use rustshare_integration_events::{ConsumerOutcome, IntegrationEvent, OutboxConsumer};
use rustshare_memory::event::{ChatChannelKind, ObservedEventType};
use rustshare_server::buzz_observation::{
    BuzzEventPush, BuzzObservationService, BuzzPushContext, IngestOutcome,
};
use rustshare_server::memory_projection::{
    MemoryChatProjectionConsumer, MEMORY_CHAT_PROJECTION_CONSUMER_ID,
};
use rustshare_storage::{ChatIdentityStore, ChatObservationStore, MemoryCatalogStore, OutboxStore};
use sqlx::{PgPool, Row};
use std::sync::{Arc, LazyLock};
use uuid::Uuid;

/// Serializes the tests within this binary (same convention as the outbox and
/// chat-observation suites).
static SERIAL: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

const WEBHOOK_SECRET: &str = "test-buzz-webhook-secret";
const COMMUNITY_ID: &str = "community-1";
const CHANNEL_ID: &str = "channel-1";

/// Shared pool over `DATABASE_URL` with the same fallback the storage-layer
/// tests use.
async fn pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    PgPool::connect(&database_url)
        .await
        .expect("failed to connect to the dev database")
}

/// The observation service under test, backed by the canonical first-party
/// Application registry (the Chat manifest owns
/// `io.elembra.chat.buzz.event.observed.v1`).
fn service(pool: PgPool) -> BuzzObservationService {
    let registry = Arc::new(ApplicationRegistry::first_party().unwrap());
    BuzzObservationService::new(
        pool.clone(),
        ChatIdentityStore::new(pool.clone()),
        ChatObservationStore::new(pool.clone()),
        Arc::new(OutboxStore::new(pool.clone(), registry)),
        WebhookSigner::new(WEBHOOK_SECRET),
        300,
    )
}

/// The consumer under test over the same pool. The catalog is wired with the
/// observation index so the tombstone-before-create delivery guard is active.
fn consumer(pool: PgPool) -> MemoryChatProjectionConsumer {
    let observations = ChatObservationStore::new(pool.clone());
    MemoryChatProjectionConsumer::new(
        pool.clone(),
        ChatIdentityStore::new(pool.clone()),
        observations.clone(),
        MemoryCatalogStore::with_observation_store(pool, observations),
    )
}

/// Remove every row the tests create: the chat tables, the chat Application
/// enablement, the receipts for the consumer, and the outbox-side tables. The
/// deliveries FK-cascade from the outbox; receipts are NOT FK'd, so they are
/// deleted explicitly by consumer id.
async fn cleanup(pool: &PgPool, tenant_id: TenantId) {
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
        .bind(MEMORY_CHAT_PROJECTION_CONSUMER_ID)
        .execute(pool)
        .await
        .unwrap();
    for table in ["integration_deliveries", "integration_outbox"] {
        sqlx::query(&format!("DELETE FROM {table} WHERE tenant_id = $1"))
            .bind(tenant_id.0)
            .execute(pool)
            .await
            .unwrap();
    }
}

/// Insert the active mapping for `community_id` under `tenant_id`
/// (workspace == tenant, per the platform invariant).
async fn insert_mapping(pool: &PgPool, tenant_id: TenantId) {
    sqlx::query(
        "INSERT INTO chat_workspace_communities
            (mapping_id, tenant_id, workspace_id, community_id, relay_url, active)
         VALUES ($1, $2, $3, $4, $5, true)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id.0)
    .bind(tenant_id.0)
    .bind(COMMUNITY_ID)
    .bind("wss://relay.example.test")
    .execute(pool)
    .await
    .unwrap();
}

/// Insert an active binding for `pubkey` and return its principal id (the
/// projected record must carry it as `author_principal_id`).
async fn insert_binding(pool: &PgPool, tenant_id: TenantId, pubkey: &str) -> PrincipalId {
    let principal_id = PrincipalId::from(Uuid::new_v4());
    sqlx::query(
        "INSERT INTO chat_identity_bindings
            (binding_id, tenant_id, principal_id, buzz_pubkey, status, verified_at, audit_metadata)
         VALUES ($1, $2, $3, $4, 'active', now(), '{}'::jsonb)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id.0)
    .bind(principal_id.0)
    .bind(pubkey)
    .execute(pool)
    .await
    .unwrap();
    principal_id
}

/// Enable the chat Application with the given projection configuration JSONB.
async fn enable_chat_application(
    pool: &PgPool,
    tenant_id: TenantId,
    configuration: serde_json::Value,
) {
    sqlx::query(
        "INSERT INTO application_enablements
            (tenant_id, workspace_id, application_id, enabled, configuration)
         VALUES ($1, $2, 'io.elembra.chat', true, $3)",
    )
    .bind(tenant_id.0)
    .bind(tenant_id.0)
    .bind(configuration)
    .execute(pool)
    .await
    .unwrap();
}

/// Standard tenant setup: active mapping, active binding for `keys`, chat
/// Application enabled with the given projection configuration.
async fn setup_tenant(
    pool: &PgPool,
    tenant_id: TenantId,
    keys: &Keys,
    configuration: serde_json::Value,
) -> PrincipalId {
    insert_mapping(pool, tenant_id).await;
    let principal = insert_binding(pool, tenant_id, &keys.public_key().to_hex()).await;
    enable_chat_application(pool, tenant_id, configuration).await;
    principal
}

/// Build a valid signed text-note push for `keys` with the given message
/// context. `message_id` is the stable message id: for a created event it
/// must equal the event id (the first event of a message IS the message id,
/// see [`signed_created_push`]); for later events it stays stable while the
/// event is a new signed event. An edit/delete supersedes the message root,
/// so `supersedes_event_id == message_id` for the first edit/delete — the
/// real contract the bridge validates.
fn signed_push(
    keys: &Keys,
    content: &str,
    message_id: &str,
    event_type: ObservedEventType,
    channel_kind: ChatChannelKind,
) -> (BuzzEventPush, nostr::Event) {
    let event = EventBuilder::text_note(content)
        .sign_with_keys(keys)
        .expect("sign text note");
    let push = BuzzEventPush {
        event: serde_json::to_value(&event).unwrap(),
        context: BuzzPushContext {
            community_id: COMMUNITY_ID.to_string(),
            channel_id: CHANNEL_ID.to_string(),
            channel_kind,
            thread_root_id: None,
            message_id: message_id.to_string(),
            event_type,
            // A first edit/delete supersedes the message root, whose event id
            // IS the message id — never the event itself.
            supersedes_event_id: (event_type != ObservedEventType::Created)
                .then(|| message_id.to_string()),
        },
    };
    (push, event)
}

/// A created event: message id == event id (the first event of a message IS
/// the message id). `channel_kind` is explicit so the never-eligible-channel
/// cases are representable.
fn signed_created_push_with_kind(
    keys: &Keys,
    content: &str,
    channel_kind: ChatChannelKind,
) -> (BuzzEventPush, nostr::Event) {
    let event = EventBuilder::text_note(content)
        .sign_with_keys(keys)
        .expect("sign text note");
    let event_id = event.id.to_hex();
    let push = BuzzEventPush {
        event: serde_json::to_value(&event).unwrap(),
        context: BuzzPushContext {
            community_id: COMMUNITY_ID.to_string(),
            channel_id: CHANNEL_ID.to_string(),
            channel_kind,
            thread_root_id: None,
            message_id: event_id.clone(),
            event_type: ObservedEventType::Created,
            supersedes_event_id: None,
        },
    };
    (push, event)
}

fn signed_created_push(keys: &Keys, content: &str) -> (BuzzEventPush, nostr::Event) {
    signed_created_push_with_kind(keys, content, ChatChannelKind::Workspace)
}

fn sign_payload(signer: &WebhookSigner, payload: &[u8], timestamp: i64) -> String {
    signer
        .sign_with_timestamp(timestamp, payload)
        .expect("sign payload")
}

/// Ingest one push through the real service and reload the durable envelope
/// from the outbox by its deterministic event id (UUIDv5 of the Buzz event
/// id), exactly as the dispatcher would hand it to the consumer.
async fn ingest_envelope(
    pool: &PgPool,
    service: &BuzzObservationService,
    push: &BuzzEventPush,
    buzz_event_id: &str,
) -> IntegrationEvent {
    let payload = serde_json::to_vec(push).unwrap();
    let signature = sign_payload(
        &WebhookSigner::new(WEBHOOK_SECRET),
        &payload,
        Utc::now().timestamp(),
    );
    assert_eq!(
        service
            .verify_and_ingest(&payload, &signature)
            .await
            .expect("ingest must succeed"),
        IngestOutcome::FirstObservation,
        "each push must be a first observation"
    );
    let event_id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("elembra://io.elembra.chat/event/{buzz_event_id}").as_bytes(),
    );
    let event_json: serde_json::Value = sqlx::query_scalar(
        "SELECT event_json FROM integration_outbox
         WHERE source = 'elembra://io.elembra.chat' AND event_id = $1",
    )
    .bind(event_id)
    .fetch_one(pool)
    .await
    .expect("the durable outbox event must exist");
    serde_json::from_value(event_json).expect("the stored envelope must deserialize")
}

/// The single catalog row for `tenant_id`, if any.
async fn catalog_row(pool: &PgPool, tenant_id: TenantId) -> Option<sqlx::postgres::PgRow> {
    sqlx::query(
        "SELECT record_id, message_id, latest_event_id, event_type, community_id, channel_id,
                channel_kind, author_pubkey, author_principal_id, occurred_at, observed_at,
                checksum, signature, signature_verified, provenance, content, indexing_status,
                tombstoned_at
         FROM memory_catalog WHERE tenant_id = $1",
    )
    .bind(tenant_id.0)
    .fetch_optional(pool)
    .await
    .expect("catalog lookup must succeed")
}

async fn catalog_count(pool: &PgPool, tenant_id: TenantId) -> i64 {
    sqlx::query_scalar("SELECT count(*)::bigint FROM memory_catalog WHERE tenant_id = $1")
        .bind(tenant_id.0)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn receipt_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*)::bigint FROM integration_consumer_receipts WHERE consumer_id = $1",
    )
    .bind(MEMORY_CHAT_PROJECTION_CONSUMER_ID)
    .fetch_one(pool)
    .await
    .unwrap()
}

// ---------------------------------------------------------------------------
// 1. Created event → exactly one record; duplicate delivery is idempotent
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn projection_creates_exactly_one_record_and_duplicate_delivery_is_idempotent() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    let keys = Keys::generate();
    let principal = setup_tenant(
        &pool,
        tenant,
        &keys,
        serde_json::json!({ "memory_projection": true, "content_indexing": false }),
    )
    .await;

    let service = service(pool.clone());
    let consumer = consumer(pool.clone());
    let (push, event) = signed_created_push(&keys, "hello buzz");
    let buzz_event_id = event.id.to_hex();
    let envelope = ingest_envelope(&pool, &service, &push, &buzz_event_id).await;

    let outcome = consumer.process(&envelope).await;
    assert_eq!(outcome, ConsumerOutcome::Processed);

    assert_eq!(catalog_count(&pool, tenant).await, 1, "exactly one record");
    let row = catalog_row(&pool, tenant).await.expect("record exists");
    assert_eq!(row.get::<String, _>("message_id"), buzz_event_id);
    assert_eq!(row.get::<String, _>("latest_event_id"), buzz_event_id);
    assert_eq!(row.get::<String, _>("event_type"), "created");
    assert_eq!(row.get::<String, _>("community_id"), COMMUNITY_ID);
    assert_eq!(row.get::<String, _>("channel_id"), CHANNEL_ID);
    assert_eq!(row.get::<String, _>("channel_kind"), "workspace");
    assert_eq!(
        row.get::<String, _>("author_pubkey"),
        keys.public_key().to_hex()
    );
    assert_eq!(
        row.get::<Option<Uuid>, _>("author_principal_id"),
        Some(principal.0),
        "author_principal_id is the bound principal"
    );
    assert_eq!(
        row.get::<String, _>("checksum"),
        format!("sha256:{buzz_event_id}")
    );
    assert_eq!(row.get::<String, _>("signature"), event.sig.to_string());
    assert!(row.get::<bool, _>("signature_verified"));
    let provenance: serde_json::Value = row.get("provenance");
    assert_eq!(
        provenance.as_array().expect("provenance is an array").len(),
        1,
        "a created event contributes exactly one provenance entry"
    );
    assert_eq!(row.get::<String, _>("indexing_status"), "reference_only");
    assert_eq!(
        row.get::<Option<String>, _>("content"),
        None,
        "content_indexing off ⇒ no body copy"
    );

    // Redelivery of the same envelope (at-least-once): Processed, still
    // exactly one record, still exactly one receipt.
    let outcome = consumer.process(&envelope).await;
    assert_eq!(outcome, ConsumerOutcome::Processed);
    assert_eq!(
        catalog_count(&pool, tenant).await,
        1,
        "idempotent: one record"
    );
    assert_eq!(receipt_count(&pool).await, 1, "exactly one receipt");
    let after = catalog_row(&pool, tenant)
        .await
        .expect("record still exists");
    assert_eq!(
        after.get::<String, _>("latest_event_id"),
        buzz_event_id,
        "redelivery must not change the record"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 2. Projection disabled (absent configuration) → consumed, no record
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn projection_disabled_produces_no_record() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    let keys = Keys::generate();
    // `{}`: memory_projection absent ⇒ false (fail closed).
    setup_tenant(&pool, tenant, &keys, serde_json::json!({})).await;

    let service = service(pool.clone());
    let consumer = consumer(pool.clone());
    let (push, event) = signed_created_push(&keys, "hello buzz");
    let envelope = ingest_envelope(&pool, &service, &push, &event.id.to_hex()).await;

    let outcome = consumer.process(&envelope).await;
    assert_eq!(outcome, ConsumerOutcome::Processed);
    assert_eq!(
        catalog_count(&pool, tenant).await,
        0,
        "a disabled policy must not project"
    );
    assert_eq!(
        receipt_count(&pool).await,
        1,
        "the event is durably processed (receipt kept) even when skipped"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 3. dm channel → consumed, never projected
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn dm_channel_never_projected() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    let keys = Keys::generate();
    setup_tenant(
        &pool,
        tenant,
        &keys,
        serde_json::json!({ "memory_projection": true, "content_indexing": false }),
    )
    .await;

    let service = service(pool.clone());
    let consumer = consumer(pool.clone());
    let (push, event) = signed_created_push_with_kind(&keys, "hello buzz", ChatChannelKind::Dm);
    let buzz_event_id = event.id.to_hex();
    let envelope = ingest_envelope(&pool, &service, &push, &buzz_event_id).await;

    let outcome = consumer.process(&envelope).await;
    assert_eq!(outcome, ConsumerOutcome::Processed);
    assert_eq!(
        catalog_count(&pool, tenant).await,
        0,
        "a dm-channel event must never be projected"
    );
    assert_eq!(
        receipt_count(&pool).await,
        1,
        "the event is durably consumed (receipt kept) with no effect"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 4. Edited event → same record updated, provenance appended
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn edit_event_updates_same_record_and_appends_provenance() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    let keys = Keys::generate();
    setup_tenant(
        &pool,
        tenant,
        &keys,
        serde_json::json!({ "memory_projection": true, "content_indexing": false }),
    )
    .await;

    let service = service(pool.clone());
    let consumer = consumer(pool.clone());

    // Created event: message M IS the created event id.
    let (created_push, created_event) = signed_created_push(&keys, "v1");
    let message_id = created_event.id.to_hex();
    let created_envelope = ingest_envelope(&pool, &service, &created_push, &message_id).await;
    assert_eq!(
        consumer.process(&created_envelope).await,
        ConsumerOutcome::Processed
    );

    // Edited event for the same message M (new signed event, later created_at).
    let (edit_push, edit_event) = signed_push(
        &keys,
        "v2",
        &message_id,
        ObservedEventType::Edited,
        ChatChannelKind::Workspace,
    );
    let edit_id = edit_event.id.to_hex();
    let edit_envelope = ingest_envelope(&pool, &service, &edit_push, &edit_id).await;
    assert_eq!(
        consumer.process(&edit_envelope).await,
        ConsumerOutcome::Processed
    );

    let t2 = edit_envelope.data["buzz"]["created_at"]
        .as_str()
        .expect("edit created_at");
    assert_eq!(
        catalog_count(&pool, tenant).await,
        1,
        "one record per message, updated not duplicated"
    );
    let row = catalog_row(&pool, tenant).await.expect("record exists");
    assert_eq!(row.get::<String, _>("message_id"), message_id);
    assert_eq!(row.get::<String, _>("latest_event_id"), edit_id);
    assert_eq!(row.get::<String, _>("event_type"), "edited");
    let provenance: serde_json::Value = row.get("provenance");
    assert_eq!(
        provenance.as_array().expect("provenance is an array").len(),
        2,
        "created + edited ⇒ two provenance entries"
    );
    let occurred_at: chrono::DateTime<Utc> = row.get("occurred_at");
    assert_eq!(
        occurred_at.to_rfc3339(),
        chrono::DateTime::parse_from_rfc3339(t2)
            .expect("edit created_at parses")
            .to_rfc3339(),
        "occurred_at is the edit event's created_at"
    );
    assert_eq!(receipt_count(&pool).await, 2, "one receipt per event");

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 5. Deleted event → record tombstoned
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn tombstone_marks_record_deleted() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    let keys = Keys::generate();
    setup_tenant(
        &pool,
        tenant,
        &keys,
        serde_json::json!({ "memory_projection": true, "content_indexing": false }),
    )
    .await;

    let service = service(pool.clone());
    let consumer = consumer(pool.clone());

    let (created_push, created_event) = signed_created_push(&keys, "v1");
    let message_id = created_event.id.to_hex();
    let created_envelope = ingest_envelope(&pool, &service, &created_push, &message_id).await;
    assert_eq!(
        consumer.process(&created_envelope).await,
        ConsumerOutcome::Processed
    );

    let (deleted_push, deleted_event) = signed_push(
        &keys,
        "deleted",
        &message_id,
        ObservedEventType::Deleted,
        ChatChannelKind::Workspace,
    );
    let deleted_id = deleted_event.id.to_hex();
    let deleted_envelope = ingest_envelope(&pool, &service, &deleted_push, &deleted_id).await;
    assert_eq!(
        consumer.process(&deleted_envelope).await,
        ConsumerOutcome::Processed
    );

    assert_eq!(
        catalog_count(&pool, tenant).await,
        1,
        "the tombstone updates the existing record, never a second row"
    );
    let row = catalog_row(&pool, tenant).await.expect("record exists");
    assert_eq!(row.get::<String, _>("event_type"), "deleted");
    assert_eq!(row.get::<String, _>("latest_event_id"), deleted_id);
    assert_eq!(row.get::<String, _>("indexing_status"), "tombstoned");
    assert!(
        row.get::<Option<chrono::DateTime<Utc>>, _>("tombstoned_at")
            .is_some(),
        "tombstoned_at must be set"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 6. content_indexing → body copied from the observation index, never in the
//    durable envelope
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn content_indexing_stores_body_copy_in_record() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    let keys = Keys::generate();
    setup_tenant(
        &pool,
        tenant,
        &keys,
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;

    let service = service(pool.clone());
    let consumer = consumer(pool.clone());
    let (push, event) = signed_created_push(&keys, "hello buzz");
    let envelope = ingest_envelope(&pool, &service, &push, &event.id.to_hex()).await;

    // Reference-first durable event: the body lives in the observation index,
    // never in the outbox envelope.
    let serialized_data = serde_json::to_string(&envelope.data).unwrap();
    assert!(
        !serialized_data.contains("hello buzz"),
        "the durable event must not carry the message body"
    );

    assert_eq!(
        consumer.process(&envelope).await,
        ConsumerOutcome::Processed
    );
    let row = catalog_row(&pool, tenant).await.expect("record exists");
    assert_eq!(
        row.get::<Option<String>, _>("content").as_deref(),
        Some("hello buzz"),
        "the consumer copies the body from the observation index"
    );
    assert_eq!(row.get::<String, _>("indexing_status"), "content_stored");

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 7. Tombstone-before-create delivery: the delete is consumed first (no-op),
//    then the create retry arrives → never a live record
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn tombstone_observed_before_create_produces_no_record() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    let keys = Keys::generate();
    setup_tenant(
        &pool,
        tenant,
        &keys,
        serde_json::json!({ "memory_projection": true, "content_indexing": false }),
    )
    .await;

    let service = service(pool.clone());
    let consumer = consumer(pool.clone());

    // Created at t0, deleted at t1 > t0. Explicit `created_at` makes the
    // ordering deterministic regardless of clock resolution (Nostr timestamps
    // are second-resolution; the guard compares against the event time).
    let t0 = Timestamp::from_secs(1_752_000_000);
    let t1 = Timestamp::from_secs(1_752_000_010);

    let created_event = EventBuilder::text_note("hello")
        .custom_created_at(t0)
        .sign_with_keys(&keys)
        .expect("sign created event");
    let created_id = created_event.id.to_hex();
    let created_push = BuzzEventPush {
        event: serde_json::to_value(&created_event).unwrap(),
        context: BuzzPushContext {
            community_id: COMMUNITY_ID.to_string(),
            channel_id: CHANNEL_ID.to_string(),
            channel_kind: ChatChannelKind::Workspace,
            thread_root_id: None,
            message_id: created_id.clone(),
            event_type: ObservedEventType::Created,
            supersedes_event_id: None,
        },
    };
    let created_envelope = ingest_envelope(&pool, &service, &created_push, &created_id).await;

    let deleted_event = EventBuilder::text_note("deleted")
        .custom_created_at(t1)
        .sign_with_keys(&keys)
        .expect("sign deleted event");
    let deleted_push = BuzzEventPush {
        event: serde_json::to_value(&deleted_event).unwrap(),
        context: BuzzPushContext {
            community_id: COMMUNITY_ID.to_string(),
            channel_id: CHANNEL_ID.to_string(),
            channel_kind: ChatChannelKind::Workspace,
            thread_root_id: None,
            message_id: created_id.clone(),
            event_type: ObservedEventType::Deleted,
            supersedes_event_id: Some(created_id.clone()),
        },
    };
    let deleted_id = deleted_event.id.to_hex();
    let deleted_envelope = ingest_envelope(&pool, &service, &deleted_push, &deleted_id).await;

    // Delivery-order inversion: the delete is consumed FIRST (a no-op — the
    // message was never projected, so the tombstone leaves no record), then
    // the create retry arrives. The tombstone-before-create guard consults
    // the observation index and must refuse to build a live record for a
    // message with a Deleted observation at-or-after this event.
    assert_eq!(
        consumer.process(&deleted_envelope).await,
        ConsumerOutcome::Processed,
        "a tombstone with no prior record is consumed with no effect"
    );
    assert_eq!(
        consumer.process(&created_envelope).await,
        ConsumerOutcome::Processed,
        "the create retry is consumed but must not project"
    );
    assert_eq!(
        catalog_count(&pool, tenant).await,
        0,
        "a deleted message must never be projected, regardless of delivery order"
    );
    assert_eq!(
        receipt_count(&pool).await,
        2,
        "both events are durably processed (receipts present)"
    );

    cleanup(&pool, tenant).await;
}
