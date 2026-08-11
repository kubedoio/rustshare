//! End-to-end integration suite proving the full Buzz → Elembra Memory
//! projection pipeline against the REAL stack: real signed Nostr kind-1
//! events, the real
//! [`BuzzObservationService`](rustshare_server::buzz_observation::BuzzObservationService)
//! HMAC bridge, the durable `io.elembra.chat.buzz.event.observed.v1` outbox
//! event, the real
//! [`OutboxDispatcher`](rustshare_server::outbox_dispatcher::OutboxDispatcher)
//! (claim → process → ack cycle), the real
//! [`MemoryChatProjectionConsumer`](rustshare_server::memory_projection::MemoryChatProjectionConsumer),
//! the real [`ChatResourceOwner`](rustshare_server::authz::ChatResourceOwner)
//! source-authorizer surface, and the real admin reconciliation path
//! ([`reconcile_chat_memory_for_tenant`](rustshare_server::handlers::memory_reconcile::reconcile_chat_memory_for_tenant)).
//!
//! The suite proves the 14 acceptance requirements of the projection:
//!
//! 1. exactly one `memory_catalog` record per message (idempotent);
//! 2. duplicate observations and at-least-once redeliveries never duplicate;
//! 3. event id / signature / checksum / provenance are preserved from the
//!    signed event;
//! 4. the author's live binding principal is recorded;
//! 5. workspace == tenant and community mapping are recorded;
//! 6. cross-tenant pushes fail closed (no observation, no outbox row);
//! 7. an offline memory worker recovers without event loss;
//! 8. a memory outage does not affect Chat ingestion;
//! 9. `dm` / `private` / `excluded` channels are never projected;
//! 10. revoking a member's admission blocks future exposure immediately;
//! 11. stale (or restored) Memory state can never override Buzz
//!     authorization;
//! 12. tombstone behavior follows Buzz semantics (deleted ⇒ tombstoned ⇒
//!     not-found, irreversibly);
//! 13. reconciliation repairs a missing projection idempotently;
//! 14. only signed events are ingested (unsigned/tampered ⇒ rejected with
//!     nothing written) and the pipeline is reference-first (no body in the
//!     durable envelope or the record unless the tenant opted in).
//!
//! Requirement #14's "no private Buzz database dependency" is structural: it
//! is asserted by the adversarial architecture review (grep for forbidden
//! table names), not by runtime tests — the observation index
//! (`chat_observed_events`) is the bridge's verified state and the only
//! Chat-side source the projection reads.
//!
//! DB-backed and `#[ignore]`d; run against the dev database (migrations
//! applied, including `20260810000005`) with `--test-threads=1`:
//!
//!   set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true \
//!     cargo test --test buzz_memory_projection_e2e_test -- --ignored --test-threads=1
//!
//! The chat tables are tenant-scoped and the outbox/delivery/receipt/consumer
//! tables are process-global, so every test takes a shared `SERIAL` guard and
//! cleans up exactly the rows it created (same convention as the outbox and
//! memory-projection suites). Every test uses a fresh tenant and community id
//! (the active-community mapping index is global).

use chrono::Utc;
use nostr::{EventBuilder, Keys, Timestamp};
use rustshare_core::domain::{
    ActionCapability, ApplicationId, ApplicationRegistry, PrincipalId, TenantId, WorkspaceId,
};
use rustshare_crypto::WebhookSigner;
use rustshare_integration_events::event_types::CHAT_BUZZ_EVENT_OBSERVED_V1;
use rustshare_integration_events::{IntegrationEvent, OutboxConsumer};
use rustshare_memory::event::{ChatChannelKind, ObservedEventType};
use rustshare_resource_auth::{
    Decision, PrincipalContext, Purpose, Representation, ResourceOwnerRegistry, ResourceRef,
    SourceAuthorizer, SourceError, CHAT_READ,
};
use rustshare_server::authz::ChatResourceOwner;
use rustshare_server::buzz_observation::{
    BuzzEventPush, BuzzObservationService, BuzzPushContext, BuzzPushError, IngestOutcome,
};
use rustshare_server::config::OutboxWorkerConfig;
use rustshare_server::handlers::memory_reconcile::reconcile_chat_memory_for_tenant;
use rustshare_server::memory_projection::MemoryChatProjectionConsumer;
use rustshare_server::outbox_dispatcher::OutboxDispatcher;
use rustshare_storage::{
    ChatIdentityStore, ChatObservationStore, MemoryCatalogStore, OutboxStore, ReconcileCounts,
};
use sqlx::{PgPool, Row};
use std::sync::{Arc, LazyLock};
use uuid::Uuid;

/// Serializes the tests within this binary (same convention as the outbox,
/// buzz-push and memory-projection suites).
static SERIAL: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

const WEBHOOK_SECRET: &str = "test-buzz-webhook-secret";
const CHANNEL_ID: &str = "channel-1";
const TEST_CONSUMER_ID: &str = "io.elembra.memory.chat-projection.buzz-e2e-test.v1";

/// The ids a tenant setup creates, for the assertions.
struct TenantSetup {
    /// The principal the author's pubkey is bound to.
    principal: PrincipalId,
    community_id: String,
}

/// Shared pool over `DATABASE_URL` with the same fallback the storage-layer
/// tests use.
async fn pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    PgPool::connect(&database_url)
        .await
        .expect("failed to connect to the dev database")
}

/// An `OutboxStore` over the harness pool with the canonical first-party
/// Application registry (the Chat manifest owns
/// `io.elembra.chat.buzz.event.observed.v1`).
fn outbox_store(pool: PgPool) -> Arc<OutboxStore> {
    let registry = Arc::new(ApplicationRegistry::first_party().unwrap());
    Arc::new(OutboxStore::new(pool, registry))
}

/// The shared chat stores over `pool`. The catalog is wired with the
/// observation index so the consumer's tombstone-before-create delivery guard
/// is active.
fn stores(pool: PgPool) -> (ChatIdentityStore, ChatObservationStore, MemoryCatalogStore) {
    let chat_identity = ChatIdentityStore::new(pool.clone());
    let observations = ChatObservationStore::new(pool.clone());
    let catalog = MemoryCatalogStore::with_observation_store(pool, observations.clone());
    (chat_identity, observations, catalog)
}

/// The bridge service under test, backed by the canonical first-party
/// Application registry.
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

/// The memory projection consumer under test over the same pool.
fn consumer(pool: PgPool) -> MemoryChatProjectionConsumer {
    let (chat_identity, observations, catalog) = stores(pool.clone());
    MemoryChatProjectionConsumer::new_for_test(
        pool,
        chat_identity,
        observations,
        catalog,
        TEST_CONSUMER_ID,
    )
}

/// Register the memory consumer durably BEFORE pushing, so publish-time eager
/// fan-out creates its pending delivery obligations (same convention as
/// `outbox_integration_test.rs::register_ref_consumer`).
async fn register_consumer(store: &OutboxStore) {
    store
        .register_consumer(TEST_CONSUMER_ID, &[CHAT_BUZZ_EVENT_OBSERVED_V1.to_string()])
        .await
        .unwrap();
}

/// Run one full dispatcher pass: maintenance, registration sync, then one
/// claim → process → ack cycle per consumer — exactly how
/// `outbox_integration_test.rs` drives the REAL dispatcher
/// (`dispatcher.tick()`).
async fn dispatch_once(pool: &PgPool, store: Arc<OutboxStore>) {
    let consumer = Arc::new(consumer(pool.clone())) as Arc<dyn OutboxConsumer>;
    let dispatcher = Arc::new(OutboxDispatcher::new(
        store,
        vec![consumer],
        OutboxWorkerConfig::default(),
        "e2e-test-worker".to_string(),
    ));
    dispatcher.tick().await;
}

/// Insert the active mapping for `community_id` under `tenant` (workspace ==
/// tenant, per the platform invariant). Returns the mapping id.
async fn insert_mapping(pool: &PgPool, tenant: TenantId, community_id: &str) -> Uuid {
    let mapping_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_workspace_communities
            (mapping_id, tenant_id, workspace_id, community_id, relay_url, active)
         VALUES ($1, $2, $3, $4, $5, true)",
    )
    .bind(mapping_id)
    .bind(tenant.0)
    .bind(tenant.0)
    .bind(community_id)
    .bind("wss://relay.example.test")
    .execute(pool)
    .await
    .unwrap();
    mapping_id
}

/// Insert an active binding for `pubkey` and return its principal id and
/// binding id (the projected record must carry the principal as
/// `author_principal_id`, and the admission FK references the binding id).
async fn insert_binding(pool: &PgPool, tenant: TenantId, pubkey: &str) -> (PrincipalId, Uuid) {
    let binding_id = Uuid::new_v4();
    let principal_id = PrincipalId::from(Uuid::new_v4());
    sqlx::query(
        "INSERT INTO chat_identity_bindings
            (binding_id, tenant_id, principal_id, buzz_pubkey, status, verified_at, audit_metadata)
         VALUES ($1, $2, $3, $4, 'active', now(), '{}'::jsonb)",
    )
    .bind(binding_id)
    .bind(tenant.0)
    .bind(principal_id.0)
    .bind(pubkey)
    .execute(pool)
    .await
    .unwrap();
    (principal_id, binding_id)
}

/// Insert an active admission for `pubkey` in the `mapping_id` community.
async fn insert_admission(
    pool: &PgPool,
    tenant: TenantId,
    mapping_id: Uuid,
    binding_id: Uuid,
    pubkey: &str,
) {
    sqlx::query(
        "INSERT INTO chat_buzz_admissions
            (admission_id, tenant_id, mapping_id, binding_id, buzz_pubkey, active)
         VALUES ($1, $2, $3, $4, $5, true)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant.0)
    .bind(mapping_id)
    .bind(binding_id)
    .bind(pubkey)
    .execute(pool)
    .await
    .unwrap();
}

/// Enable the chat Application with the given projection configuration JSONB.
async fn enable_chat(pool: &PgPool, tenant: TenantId, configuration: serde_json::Value) {
    sqlx::query(
        "INSERT INTO application_enablements
            (tenant_id, workspace_id, application_id, enabled, configuration)
         VALUES ($1, $2, 'io.elembra.chat', true, $3)",
    )
    .bind(tenant.0)
    .bind(tenant.0)
    .bind(configuration)
    .execute(pool)
    .await
    .unwrap();
}

/// Full happy-path tenant setup: active mapping, active binding for `keys`,
/// active admission for the bound pubkey, and the chat Application enabled
/// with `configuration`.
async fn setup_tenant(
    pool: &PgPool,
    tenant: TenantId,
    keys: &Keys,
    community_id: &str,
    configuration: serde_json::Value,
) -> TenantSetup {
    let mapping_id = insert_mapping(pool, tenant, community_id).await;
    let pubkey = keys.public_key().to_hex();
    let (principal, binding_id) = insert_binding(pool, tenant, &pubkey).await;
    insert_admission(pool, tenant, mapping_id, binding_id, &pubkey).await;
    enable_chat(pool, tenant, configuration).await;
    TenantSetup {
        principal,
        community_id: community_id.to_string(),
    }
}

/// Revoke every active admission for the tenant (as `revoke_principal` would
/// for a binding; `active = false` is sufficient to prove immediate
/// revocation semantics).
async fn revoke_admissions(pool: &PgPool, tenant: TenantId) {
    sqlx::query("UPDATE chat_buzz_admissions SET active = false WHERE tenant_id = $1 AND active")
        .bind(tenant.0)
        .execute(pool)
        .await
        .unwrap();
}

/// A real signed kind-1 (text note) event with an explicit `created_at`.
fn signed_note(keys: &Keys, content: &str, created_at: Timestamp) -> nostr::Event {
    EventBuilder::text_note(content)
        .custom_created_at(created_at)
        .sign_with_keys(keys)
        .expect("sign text note")
}

/// A created-event push: message id == event id (the first event of a message
/// IS the message id), `created_at = now`.
fn created_push(
    keys: &Keys,
    content: &str,
    community_id: &str,
    channel_kind: ChatChannelKind,
) -> (BuzzEventPush, nostr::Event) {
    created_push_at(
        keys,
        content,
        community_id,
        channel_kind,
        Timestamp::from_secs(Utc::now().timestamp() as u64),
    )
}

/// A created-event push with an explicit event `created_at` (for the
/// ordering-sensitive tests).
fn created_push_at(
    keys: &Keys,
    content: &str,
    community_id: &str,
    channel_kind: ChatChannelKind,
    created_at: Timestamp,
) -> (BuzzEventPush, nostr::Event) {
    let event = signed_note(keys, content, created_at);
    let event_id = event.id.to_hex();
    let push = BuzzEventPush {
        event: serde_json::to_value(&event).unwrap(),
        context: BuzzPushContext {
            community_id: community_id.to_string(),
            channel_id: CHANNEL_ID.to_string(),
            channel_kind,
            thread_root_id: None,
            message_id: event_id,
            event_type: ObservedEventType::Created,
            supersedes_event_id: None,
        },
    };
    (push, event)
}

/// A deleted-event push for the message whose root event is `message_id`: the
/// delete is a different signed event superseding the message root (the real
/// contract the bridge validates).
fn deleted_push_at(
    keys: &Keys,
    content: &str,
    message_id: &str,
    community_id: &str,
    created_at: Timestamp,
) -> (BuzzEventPush, nostr::Event) {
    let event = signed_note(keys, content, created_at);
    let push = BuzzEventPush {
        event: serde_json::to_value(&event).unwrap(),
        context: BuzzPushContext {
            community_id: community_id.to_string(),
            channel_id: CHANNEL_ID.to_string(),
            channel_kind: ChatChannelKind::Workspace,
            thread_root_id: None,
            message_id: message_id.to_string(),
            event_type: ObservedEventType::Deleted,
            supersedes_event_id: Some(message_id.to_string()),
        },
    };
    (push, event)
}

/// HMAC-sign (fresh timestamp, real `WebhookSigner`) and ingest one serialized
/// push payload through the REAL bridge service.
async fn push(
    service: &BuzzObservationService,
    payload: &[u8],
) -> Result<IngestOutcome, BuzzPushError> {
    let signature = WebhookSigner::new(WEBHOOK_SECRET)
        .sign_with_timestamp(Utc::now().timestamp(), payload)
        .expect("sign payload");
    service.verify_and_ingest(payload, &signature).await
}

/// Serialize a `BuzzEventPush` and ingest it (see [`push`]).
async fn ingest_push(
    service: &BuzzObservationService,
    buzz_push: &BuzzEventPush,
) -> Result<IngestOutcome, BuzzPushError> {
    push(service, &serde_json::to_vec(buzz_push).unwrap()).await
}

/// Reload the durable envelope for a Buzz event id from the outbox by its
/// deterministic event id (UUIDv5), exactly as the dispatcher would hand it to
/// the consumer.
async fn envelope_from_outbox(pool: &PgPool, buzz_event_id: &str) -> IntegrationEvent {
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

/// A fresh `SourceAuthorizer` seeded with the Chat owner adapter over `pool`.
async fn authorizer(pool: PgPool) -> SourceAuthorizer {
    let registry = ApplicationRegistry::first_party().expect("first-party manifests are valid");
    let mut owners = ResourceOwnerRegistry::new();
    owners
        .register(
            Arc::new(ChatResourceOwner::new(
                ChatIdentityStore::new(pool.clone()),
                ChatObservationStore::new(pool.clone()),
            )),
            &registry,
        )
        .expect("the io.elembra.chat owner registers against the canonical registry");
    SourceAuthorizer::new(owners)
}

/// A plain human-user principal context (workspace == tenant per the platform
/// invariant).
fn user_ctx(principal: PrincipalId, tenant: TenantId) -> PrincipalContext {
    PrincipalContext::user(principal, tenant, WorkspaceId(tenant.0))
}

/// A canonical ref for a Chat message.
fn chat_ref(message_id: &str) -> ResourceRef {
    ResourceRef::new(ApplicationId::new("io.elembra.chat"), "message", message_id)
}

fn chat_read_action() -> ActionCapability {
    ActionCapability::new(CHAT_READ)
}

/// Run the reconcile orchestration over the shared stores (the admin repair
/// path, no outbox replay, no receipts).
async fn reconcile(pool: &PgPool, tenant_id: TenantId) -> ReconcileCounts {
    let (chat_identity, observations, catalog) = stores(pool.clone());
    reconcile_chat_memory_for_tenant(&chat_identity, &observations, &catalog, tenant_id, None)
        .await
        .expect("reconcile must succeed")
}

/// The single catalog row for `tenant_id`, if any.
async fn catalog_row(pool: &PgPool, tenant_id: TenantId) -> Option<sqlx::postgres::PgRow> {
    sqlx::query(
        "SELECT record_id, tenant_id, workspace_id, message_id, latest_event_id, event_type,
                community_id, channel_id, channel_kind, author_pubkey, author_principal_id,
                occurred_at, observed_at, checksum, signature, signature_verified, provenance,
                content, indexing_status, tombstoned_at
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

async fn observation_count(pool: &PgPool, tenant_id: TenantId) -> i64 {
    sqlx::query_scalar("SELECT count(*)::bigint FROM chat_observed_events WHERE tenant_id = $1")
        .bind(tenant_id.0)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn outbox_count(pool: &PgPool, tenant_id: TenantId) -> i64 {
    sqlx::query_scalar("SELECT count(*)::bigint FROM integration_outbox WHERE tenant_id = $1")
        .bind(tenant_id.0)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn receipt_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*)::bigint FROM integration_consumer_receipts WHERE consumer_id = $1",
    )
    .bind(TEST_CONSUMER_ID)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn delivery_count(pool: &PgPool, state: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*)::bigint FROM integration_deliveries
         WHERE consumer_id = $1 AND state = $2",
    )
    .bind(TEST_CONSUMER_ID)
    .bind(state)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Remove every row the tests create for `tenant_id` — the chat tables, the
/// chat Application enablement, the receipts for the consumer, the
/// outbox-side rows (deliveries FK-cascade from the outbox), and the durable
/// consumer registration (subscriptions cascade).
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
        .bind(TEST_CONSUMER_ID)
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
    sqlx::query("DELETE FROM integration_consumers WHERE consumer_id = $1")
        .bind(TEST_CONSUMER_ID)
        .execute(pool)
        .await
        .unwrap();
}

// ---------------------------------------------------------------------------
// 1. One created event → exactly one record (reference-first)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn e2e_one_event_creates_exactly_one_record() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    // Defensive pre-cleanup: every test must be deterministic even if a
    // previous (possibly interrupted) run left rows for this tenant or for
    // the shared consumer id behind.
    cleanup(&pool, tenant).await;
    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    setup_tenant(
        &pool,
        tenant,
        &keys,
        &community,
        serde_json::json!({ "memory_projection": true, "content_indexing": false }),
    )
    .await;
    let store = outbox_store(pool.clone());
    register_consumer(&store).await;

    let service = service(pool.clone());
    let (buzz_push, event) =
        created_push(&keys, "hello buzz", &community, ChatChannelKind::Workspace);
    assert_eq!(
        ingest_push(&service, &buzz_push).await.unwrap(),
        IngestOutcome::FirstObservation
    );
    let buzz_event_id = event.id.to_hex();

    dispatch_once(&pool, store).await;

    assert_eq!(catalog_count(&pool, tenant).await, 1, "exactly one record");
    let row = catalog_row(&pool, tenant).await.expect("record exists");
    assert_eq!(row.get::<String, _>("message_id"), buzz_event_id);
    assert_eq!(row.get::<String, _>("latest_event_id"), buzz_event_id);
    assert_eq!(row.get::<String, _>("event_type"), "created");
    assert_eq!(row.get::<String, _>("indexing_status"), "reference_only");
    assert_eq!(
        row.get::<Option<String>, _>("content"),
        None,
        "content_indexing off ⇒ no body copy"
    );
    assert_eq!(receipt_count(&pool).await, 1, "one durable receipt");
    assert_eq!(
        delivery_count(&pool, "processed").await,
        1,
        "the delivery ledger shows the event processed"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 2. Duplicate observation ⇒ no duplicate; redelivery ⇒ still no duplicate
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn e2e_duplicate_observation_creates_no_duplicate() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    // Defensive pre-cleanup: every test must be deterministic even if a
    // previous (possibly interrupted) run left rows for this tenant or for
    // the shared consumer id behind.
    cleanup(&pool, tenant).await;
    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    setup_tenant(
        &pool,
        tenant,
        &keys,
        &community,
        serde_json::json!({ "memory_projection": true, "content_indexing": false }),
    )
    .await;
    let store = outbox_store(pool.clone());
    register_consumer(&store).await;

    let service = service(pool.clone());
    let (buzz_push, event) =
        created_push(&keys, "hello buzz", &community, ChatChannelKind::Workspace);
    let payload = serde_json::to_vec(&buzz_push).unwrap();
    let buzz_event_id = event.id.to_hex();

    assert_eq!(
        push(&service, &payload).await.unwrap(),
        IngestOutcome::FirstObservation
    );
    assert_eq!(
        push(&service, &payload).await.unwrap(),
        IngestOutcome::DuplicateObservation,
        "the identical observation is a no-op"
    );
    assert_eq!(
        outbox_count(&pool, tenant).await,
        1,
        "exactly one durable outbox event"
    );

    dispatch_once(&pool, store.clone()).await;
    assert_eq!(catalog_count(&pool, tenant).await, 1);
    assert_eq!(receipt_count(&pool).await, 1);

    // Redelivery scenario: an operator (or a crashed worker's lease cycle)
    // resets the processed delivery to pending, then the dispatcher runs
    // again — at-least-once. The consumer's durable receipt must collapse it:
    // still exactly one record and one receipt.
    sqlx::query(
        "UPDATE integration_deliveries SET state = 'pending', available_at = now(), claim_token = NULL, claim_expires_at = NULL WHERE consumer_id = $1 AND event_id = $2",
    )
    .bind(TEST_CONSUMER_ID)
    .bind(Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("elembra://io.elembra.chat/event/{buzz_event_id}").as_bytes(),
    ))
    .execute(&pool)
    .await
    .unwrap();
    dispatch_once(&pool, store.clone()).await;

    assert_eq!(
        catalog_count(&pool, tenant).await,
        1,
        "redelivery must not duplicate the record"
    );
    assert_eq!(receipt_count(&pool).await, 1, "exactly one durable receipt");
    assert_eq!(
        delivery_count(&pool, "processed").await,
        1,
        "the redelivered event is processed again"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 3. Event id / signature / checksum / provenance preserved from the event
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn e2e_event_id_signature_checksum_provenance_preserved() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    // Defensive pre-cleanup: every test must be deterministic even if a
    // previous (possibly interrupted) run left rows for this tenant or for
    // the shared consumer id behind.
    cleanup(&pool, tenant).await;
    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    let setup = setup_tenant(
        &pool,
        tenant,
        &keys,
        &community,
        serde_json::json!({ "memory_projection": true, "content_indexing": false }),
    )
    .await;
    let store = outbox_store(pool.clone());
    register_consumer(&store).await;

    let created_at = Timestamp::from_secs(1_752_000_000);
    let service = service(pool.clone());
    let (buzz_push, event) = created_push_at(
        &keys,
        "provenance preserved",
        &community,
        ChatChannelKind::Workspace,
        created_at,
    );
    assert_eq!(
        ingest_push(&service, &buzz_push).await.unwrap(),
        IngestOutcome::FirstObservation
    );

    dispatch_once(&pool, store).await;

    let buzz_event_id = event.id.to_hex();
    let row = catalog_row(&pool, tenant).await.expect("record exists");
    assert_eq!(row.get::<String, _>("latest_event_id"), buzz_event_id);
    assert_eq!(row.get::<String, _>("signature"), event.sig.to_string());
    assert_eq!(
        row.get::<String, _>("checksum"),
        format!("sha256:{buzz_event_id}")
    );
    assert!(row.get::<bool, _>("signature_verified"));
    assert_eq!(
        row.get::<chrono::DateTime<Utc>, _>("occurred_at"),
        chrono::DateTime::<Utc>::from_timestamp(created_at.as_secs() as i64, 0).unwrap(),
        "occurred_at is the signed event's created_at"
    );
    let provenance: serde_json::Value = row.get("provenance");
    assert_eq!(
        provenance.as_array().expect("provenance is an array").len(),
        1
    );
    assert_eq!(provenance[0]["event_id"], serde_json::json!(buzz_event_id));
    assert_eq!(
        row.get::<Option<Uuid>, _>("author_principal_id"),
        Some(setup.principal.0)
    );
    assert_eq!(row.get::<String, _>("community_id"), setup.community_id);
    assert_eq!(row.get::<String, _>("channel_id"), CHANNEL_ID);
    assert_eq!(row.get::<String, _>("channel_kind"), "workspace");

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 4. Author principal mapping: the live binding's principal is recorded
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn e2e_principal_mapping_correct() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    // Defensive pre-cleanup: every test must be deterministic even if a
    // previous (possibly interrupted) run left rows for this tenant or for
    // the shared consumer id behind.
    cleanup(&pool, tenant).await;
    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    let setup = setup_tenant(
        &pool,
        tenant,
        &keys,
        &community,
        serde_json::json!({ "memory_projection": true, "content_indexing": false }),
    )
    .await;
    let store = outbox_store(pool.clone());
    register_consumer(&store).await;

    let service = service(pool.clone());
    let (buzz_push, _event) =
        created_push(&keys, "hello buzz", &community, ChatChannelKind::Workspace);
    assert_eq!(
        ingest_push(&service, &buzz_push).await.unwrap(),
        IngestOutcome::FirstObservation
    );

    dispatch_once(&pool, store).await;

    let row = catalog_row(&pool, tenant).await.expect("record exists");
    assert_eq!(
        row.get::<Option<Uuid>, _>("author_principal_id"),
        Some(setup.principal.0),
        "the record's author_principal_id is the principal the author's pubkey is bound to"
    );
    assert_eq!(
        row.get::<String, _>("author_pubkey"),
        keys.public_key().to_hex()
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 5. Workspace == tenant and community mapping recorded
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn e2e_workspace_community_mapping_correct() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    // Defensive pre-cleanup: every test must be deterministic even if a
    // previous (possibly interrupted) run left rows for this tenant or for
    // the shared consumer id behind.
    cleanup(&pool, tenant).await;
    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    setup_tenant(
        &pool,
        tenant,
        &keys,
        &community,
        serde_json::json!({ "memory_projection": true, "content_indexing": false }),
    )
    .await;
    let store = outbox_store(pool.clone());
    register_consumer(&store).await;

    let service = service(pool.clone());
    let (buzz_push, _event) =
        created_push(&keys, "hello buzz", &community, ChatChannelKind::Workspace);
    assert_eq!(
        ingest_push(&service, &buzz_push).await.unwrap(),
        IngestOutcome::FirstObservation
    );

    dispatch_once(&pool, store).await;

    let row = catalog_row(&pool, tenant).await.expect("record exists");
    assert_eq!(row.get::<Uuid, _>("tenant_id"), tenant.0);
    assert_eq!(
        row.get::<Uuid, _>("workspace_id"),
        tenant.0,
        "workspace == tenant per the platform invariant"
    );
    assert_eq!(row.get::<String, _>("community_id"), community);

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 6. Cross-tenant fail-closed: no observation, no outbox, no record
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn e2e_cross_tenant_fails_closed() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant_a = TenantId::from(Uuid::new_v4());
    let tenant_b = TenantId::from(Uuid::new_v4());
    // Defensive pre-cleanup for both tenants (see the other tests' comment).
    cleanup(&pool, tenant_a).await;
    cleanup(&pool, tenant_b).await;
    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    let configuration = serde_json::json!({ "memory_projection": true, "content_indexing": false });

    // The pubkey is bound in tenant A only; the community maps (active) to
    // tenant B only.
    insert_binding(&pool, tenant_a, &keys.public_key().to_hex()).await;
    insert_mapping(&pool, tenant_b, &community).await;
    enable_chat(&pool, tenant_b, configuration).await;

    let service = service(pool.clone());
    let (buzz_push, _event) = created_push(
        &keys,
        "cross-tenant",
        &community,
        ChatChannelKind::Workspace,
    );
    let err = ingest_push(&service, &buzz_push).await.unwrap_err();
    assert!(
        matches!(&err, BuzzPushError::UnboundAuthor),
        "an author bound only in another tenant is unbound here, got {err:?}"
    );

    // Fail closed: nothing was written for either tenant.
    assert_eq!(observation_count(&pool, tenant_a).await, 0);
    assert_eq!(observation_count(&pool, tenant_b).await, 0);
    assert_eq!(outbox_count(&pool, tenant_a).await, 0);
    assert_eq!(outbox_count(&pool, tenant_b).await, 0);
    assert_eq!(catalog_count(&pool, tenant_a).await, 0);
    assert_eq!(catalog_count(&pool, tenant_b).await, 0);

    // Second variant: the same pubkey bound in BOTH tenants, pushed to B's
    // community → succeeds and writes ONLY tenant-B rows (no leakage into A).
    insert_binding(&pool, tenant_b, &keys.public_key().to_hex()).await;
    let (push2, _event2) = created_push(
        &keys,
        "cross-tenant allowed",
        &community,
        ChatChannelKind::Workspace,
    );
    assert_eq!(
        ingest_push(&service, &push2).await.unwrap(),
        IngestOutcome::FirstObservation
    );
    assert_eq!(observation_count(&pool, tenant_b).await, 1);
    assert_eq!(outbox_count(&pool, tenant_b).await, 1);
    assert_eq!(
        observation_count(&pool, tenant_a).await,
        0,
        "no cross-tenant leakage into tenant A"
    );
    assert_eq!(outbox_count(&pool, tenant_a).await, 0);

    cleanup(&pool, tenant_a).await;
    cleanup(&pool, tenant_b).await;
}

// ---------------------------------------------------------------------------
// 7. Offline memory worker: durable events accumulate, then recover
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn e2e_offline_memory_worker_recovers_without_event_loss() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    // Defensive pre-cleanup: every test must be deterministic even if a
    // previous (possibly interrupted) run left rows for this tenant or for
    // the shared consumer id behind.
    cleanup(&pool, tenant).await;
    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    setup_tenant(
        &pool,
        tenant,
        &keys,
        &community,
        serde_json::json!({ "memory_projection": true, "content_indexing": false }),
    )
    .await;
    let store = outbox_store(pool.clone());
    register_consumer(&store).await;
    store
        .set_consumer_enabled(TEST_CONSUMER_ID, false)
        .await
        .unwrap();

    let service = service(pool.clone());
    for content in ["message one", "message two"] {
        let (buzz_push, _event) =
            created_push(&keys, content, &community, ChatChannelKind::Workspace);
        assert_eq!(
            ingest_push(&service, &buzz_push).await.unwrap(),
            IngestOutcome::FirstObservation
        );
    }

    // Durable while the worker is offline: both outbox rows exist and both
    // deliveries are pending (obligations are created regardless of `enabled`).
    assert_eq!(outbox_count(&pool, tenant).await, 2);
    assert_eq!(delivery_count(&pool, "pending").await, 2);
    assert_eq!(delivery_count(&pool, "processed").await, 0);
    assert_eq!(catalog_count(&pool, tenant).await, 0);

    // A dispatch pass while the worker is disabled must NOT claim the pending
    // deliveries: the real dispatcher runs with `enabled = false` and leaves
    // both events queued (nothing processed, nothing projected).
    dispatch_once(&pool, store.clone()).await;
    assert_eq!(
        delivery_count(&pool, "pending").await,
        2,
        "disabled consumer must not claim deliveries"
    );
    assert_eq!(delivery_count(&pool, "processed").await, 0);
    assert_eq!(catalog_count(&pool, tenant).await, 0);

    // Re-enable the worker and run the dispatch pass: both events process,
    // one record per message, one receipt per event.
    store
        .set_consumer_enabled(TEST_CONSUMER_ID, true)
        .await
        .unwrap();
    dispatch_once(&pool, store).await;

    assert_eq!(delivery_count(&pool, "processed").await, 2);
    assert_eq!(delivery_count(&pool, "pending").await, 0);
    assert_eq!(
        catalog_count(&pool, tenant).await,
        2,
        "one record per message"
    );
    assert_eq!(receipt_count(&pool).await, 2, "one receipt per event");

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 8. Memory outage does not affect Chat ingestion
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn e2e_memory_outage_does_not_affect_chat() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    // Defensive pre-cleanup: every test must be deterministic even if a
    // previous (possibly interrupted) run left rows for this tenant or for
    // the shared consumer id behind.
    cleanup(&pool, tenant).await;
    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    setup_tenant(
        &pool,
        tenant,
        &keys,
        &community,
        serde_json::json!({ "memory_projection": true, "content_indexing": false }),
    )
    .await;

    // Hold an uncommitted `DROP TABLE memory_catalog` on one connection: the
    // table is gone (to this test's view) for the duration of the outage, and
    // an ACCESS EXCLUSIVE lock is held. The Chat ingest path must never touch
    // `memory_catalog`, so the push from the pool's other connections
    // succeeds and commits while the DROP is still open. This is the
    // strongest runtime proof that the observation/outbox write path has no
    // dependency on Memory-owned state.
    //
    // (If a future change made the ingest path touch `memory_catalog`, this
    // push would block on the DROP lock; the timeout below converts that
    // regression into a clean test failure instead of an infinite hang.)
    let mut drop_tx = pool.begin().await.unwrap();
    sqlx::query("DROP TABLE memory_catalog")
        .execute(&mut *drop_tx)
        .await
        .unwrap();

    let service = service(pool.clone());
    let (buzz_push, _event) =
        created_push(&keys, "hello buzz", &community, ChatChannelKind::Workspace);
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        ingest_push(&service, &buzz_push),
    )
    .await
    .expect("ingest must not block on the memory_catalog outage")
    .unwrap();
    assert_eq!(
        outcome,
        IngestOutcome::FirstObservation,
        "ingest succeeds during a memory outage"
    );
    assert_eq!(observation_count(&pool, tenant).await, 1);
    assert_eq!(outbox_count(&pool, tenant).await, 1);

    // End the outage: the DROP rolls back and the table is back.
    drop_tx.rollback().await.unwrap();
    let _back: i64 = sqlx::query_scalar("SELECT count(*)::bigint FROM memory_catalog")
        .fetch_one(&pool)
        .await
        .expect("memory_catalog exists again after the outage");
    assert_eq!(observation_count(&pool, tenant).await, 1);
    assert_eq!(outbox_count(&pool, tenant).await, 1);

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 9. dm / private / excluded channels are consumed but never projected
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn e2e_excluded_and_dm_channels_not_projected() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    // Defensive pre-cleanup: every test must be deterministic even if a
    // previous (possibly interrupted) run left rows for this tenant or for
    // the shared consumer id behind.
    cleanup(&pool, tenant).await;
    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    setup_tenant(
        &pool,
        tenant,
        &keys,
        &community,
        serde_json::json!({ "memory_projection": true, "content_indexing": false }),
    )
    .await;
    let store = outbox_store(pool.clone());
    register_consumer(&store).await;

    let service = service(pool.clone());
    for (kind, content) in [
        (ChatChannelKind::Dm, "dm secret"),
        (ChatChannelKind::Private, "private secret"),
        (ChatChannelKind::Excluded, "excluded secret"),
    ] {
        let (buzz_push, _event) = created_push(&keys, content, &community, kind);
        assert_eq!(
            ingest_push(&service, &buzz_push).await.unwrap(),
            IngestOutcome::FirstObservation,
            "kind={kind:?}"
        );
    }

    dispatch_once(&pool, store).await;

    assert_eq!(
        catalog_count(&pool, tenant).await,
        0,
        "never-eligible channels must never be projected"
    );
    assert_eq!(receipt_count(&pool).await, 3, "one receipt per event");
    assert_eq!(
        delivery_count(&pool, "processed").await,
        3,
        "all three events are durably consumed"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 10. Revoked membership blocks future exposure immediately
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn e2e_revoked_membership_blocks_future_exposure() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    // Defensive pre-cleanup: every test must be deterministic even if a
    // previous (possibly interrupted) run left rows for this tenant or for
    // the shared consumer id behind.
    cleanup(&pool, tenant).await;
    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    let setup = setup_tenant(
        &pool,
        tenant,
        &keys,
        &community,
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;
    let store = outbox_store(pool.clone());
    register_consumer(&store).await;

    let service = service(pool.clone());
    let body = "body copy exists";
    let (buzz_push, event) = created_push(&keys, body, &community, ChatChannelKind::Workspace);
    assert_eq!(
        ingest_push(&service, &buzz_push).await.unwrap(),
        IngestOutcome::FirstObservation
    );
    dispatch_once(&pool, store).await;
    let row = catalog_row(&pool, tenant).await.expect("record exists");
    assert_eq!(row.get::<String, _>("indexing_status"), "content_stored");
    assert_eq!(
        row.get::<Option<String>, _>("content").as_deref(),
        Some(body)
    );

    let authorizer = authorizer(pool.clone()).await;
    let ctx = user_ctx(setup.principal, tenant);
    let reference = chat_ref(&event.id.to_hex());

    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &reference)
            .await,
        Decision::Allow,
        "a bound, admitted member may read a workspace message"
    );

    revoke_admissions(&pool, tenant).await;

    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &reference)
            .await,
        Decision::Deny,
        "a revoked admission must deny immediately, without any Memory state change"
    );
    assert!(
        matches!(
            authorizer
                .resolve(&ctx, &reference, Purpose::RagContext)
                .await,
            Err(SourceError::NotFound)
        ),
        "resolve must fail with the existence-hiding variant"
    );
    assert!(
        matches!(
            authorizer
                .fetch(&ctx, &reference, Representation::Raw)
                .await,
            Err(SourceError::NotFound)
        ),
        "fetch must fail with the existence-hiding variant"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 11. Stale Memory can never override Buzz authorization
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn e2e_stale_memory_cannot_override_buzz_authorization() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    // Defensive pre-cleanup: every test must be deterministic even if a
    // previous (possibly interrupted) run left rows for this tenant or for
    // the shared consumer id behind.
    cleanup(&pool, tenant).await;
    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    let setup = setup_tenant(
        &pool,
        tenant,
        &keys,
        &community,
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;
    let store = outbox_store(pool.clone());
    register_consumer(&store).await;

    let service = service(pool.clone());
    let body = "stale content must never leak";
    let (buzz_push, event) = created_push(&keys, body, &community, ChatChannelKind::Workspace);
    assert_eq!(
        ingest_push(&service, &buzz_push).await.unwrap(),
        IngestOutcome::FirstObservation
    );
    dispatch_once(&pool, store).await;
    let message_id = event.id.to_hex();
    let row = catalog_row(&pool, tenant).await.expect("record exists");
    assert_eq!(row.get::<String, _>("indexing_status"), "content_stored");
    assert_eq!(
        row.get::<Option<String>, _>("content").as_deref(),
        Some(body)
    );

    let authorizer = authorizer(pool.clone()).await;
    let ctx = user_ctx(setup.principal, tenant);
    let reference = chat_ref(&message_id);

    // Baseline: while the member is admitted, current Buzz state grants.
    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &reference)
            .await,
        Decision::Allow
    );

    // Memory is gone (corruption / manual deletion): the catalog row —
    // including the stored body — disappears.
    sqlx::query("DELETE FROM memory_catalog WHERE tenant_id = $1")
        .bind(tenant.0)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(catalog_count(&pool, tenant).await, 0);

    // The member's admission is revoked — the ONLY state that can flip the
    // outcome (authorize never reads the catalog). The denial is identical
    // with and without the catalog row: authz never depends on Memory state.
    revoke_admissions(&pool, tenant).await;
    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &reference)
            .await,
        Decision::Deny,
        "still denied with the catalog row deleted (authz never depends on the catalog)"
    );
    assert!(
        matches!(
            authorizer
                .fetch(&ctx, &reference, Representation::Raw)
                .await,
            Err(SourceError::NotFound)
        ),
        "fetch must not resurrect content from the deleted catalog"
    );

    // Reconciliation restores the record — WITH its stored content (the
    // observation index still holds the body copy).
    let counts = reconcile(&pool, tenant).await;
    assert_eq!(counts.created, 1, "the record is rebuilt");
    assert_eq!(catalog_count(&pool, tenant).await, 1);
    let restored = catalog_row(&pool, tenant).await.expect("record restored");
    assert_eq!(
        restored.get::<String, _>("indexing_status"),
        "content_stored"
    );
    assert_eq!(
        restored.get::<Option<String>, _>("content").as_deref(),
        Some(body)
    );

    // The restored record + content must NOT grant anything: the member is
    // still revoked, so authorize denies and fetch yields no content.
    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &reference)
            .await,
        Decision::Deny,
        "a present catalog record (with content) must never override a revoked admission"
    );
    assert!(
        matches!(
            authorizer
                .fetch(&ctx, &reference, Representation::Raw)
                .await,
            Err(SourceError::NotFound)
        ),
        "fetch must fail closed even though the record (with content) exists"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 12. Tombstone behavior follows Buzz semantics
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn e2e_tombstone_behavior_follows_buzz_semantics() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    // Defensive pre-cleanup: every test must be deterministic even if a
    // previous (possibly interrupted) run left rows for this tenant or for
    // the shared consumer id behind.
    cleanup(&pool, tenant).await;
    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    let setup = setup_tenant(
        &pool,
        tenant,
        &keys,
        &community,
        serde_json::json!({ "memory_projection": true, "content_indexing": false }),
    )
    .await;
    let store = outbox_store(pool.clone());
    register_consumer(&store).await;

    // Created at t0, deleted at t0+5s: the deleted observation row is the
    // latest for the message (authz lookup) and the reconcile fold is
    // created-then-deleted.
    let t0 = Timestamp::from_secs(1_752_000_000);
    let service = service(pool.clone());
    let (created, created_event) =
        created_push_at(&keys, "v1", &community, ChatChannelKind::Workspace, t0);
    let message_id = created_event.id.to_hex();
    assert_eq!(
        ingest_push(&service, &created).await.unwrap(),
        IngestOutcome::FirstObservation
    );
    let (deleted, deleted_event) = deleted_push_at(
        &keys,
        "deleted",
        &message_id,
        &community,
        Timestamp::from_secs(1_752_000_005),
    );
    assert_eq!(
        ingest_push(&service, &deleted).await.unwrap(),
        IngestOutcome::FirstObservation
    );

    dispatch_once(&pool, store).await;

    assert_eq!(catalog_count(&pool, tenant).await, 1);
    let row = catalog_row(&pool, tenant).await.expect("record exists");
    assert_eq!(row.get::<String, _>("message_id"), message_id);
    assert_eq!(
        row.get::<String, _>("latest_event_id"),
        deleted_event.id.to_hex()
    );
    assert_eq!(row.get::<String, _>("event_type"), "deleted");
    assert_eq!(row.get::<String, _>("indexing_status"), "tombstoned");
    assert!(
        row.get::<Option<chrono::DateTime<Utc>>, _>("tombstoned_at")
            .is_some(),
        "tombstoned_at must be set"
    );
    let provenance: serde_json::Value = row.get("provenance");
    assert_eq!(
        provenance.as_array().expect("provenance is an array").len(),
        2,
        "created + deleted ⇒ two provenance entries"
    );

    // A tombstoned message is not exposable: existence-hiding NotFound.
    let authorizer = authorizer(pool.clone()).await;
    let ctx = user_ctx(setup.principal, tenant);
    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &chat_ref(&message_id))
            .await,
        Decision::NotFound,
        "a deleted message must look absent"
    );

    // Reconciliation re-run must not resurrect the tombstone.
    let counts = reconcile(&pool, tenant).await;
    assert_eq!(counts.processed, 2);
    assert_eq!(catalog_count(&pool, tenant).await, 1);
    let after = catalog_row(&pool, tenant)
        .await
        .expect("record still exists");
    assert_eq!(after.get::<String, _>("event_type"), "deleted");
    assert_eq!(after.get::<String, _>("indexing_status"), "tombstoned");

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 13. Reconciliation repairs a missing projection idempotently
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn e2e_reconciliation_repairs_missing_projection() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    // Defensive pre-cleanup: every test must be deterministic even if a
    // previous (possibly interrupted) run left rows for this tenant or for
    // the shared consumer id behind.
    cleanup(&pool, tenant).await;
    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    setup_tenant(
        &pool,
        tenant,
        &keys,
        &community,
        serde_json::json!({ "memory_projection": true, "content_indexing": false }),
    )
    .await;
    let store = outbox_store(pool.clone());
    register_consumer(&store).await;

    let service = service(pool.clone());
    let (buzz_push, _event) =
        created_push(&keys, "hello buzz", &community, ChatChannelKind::Workspace);
    assert_eq!(
        ingest_push(&service, &buzz_push).await.unwrap(),
        IngestOutcome::FirstObservation
    );
    dispatch_once(&pool, store).await;
    assert_eq!(catalog_count(&pool, tenant).await, 1);

    // Corrupt the projection: drop the record.
    sqlx::query("DELETE FROM memory_catalog WHERE tenant_id = $1")
        .bind(tenant.0)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(catalog_count(&pool, tenant).await, 0);

    let counts = reconcile(&pool, tenant).await;
    assert_eq!(counts.processed, 1);
    assert_eq!(counts.created, 1, "the record is rebuilt exactly once");
    assert_eq!(catalog_count(&pool, tenant).await, 1);

    // Idempotent: a second run creates nothing and duplicates nothing.
    let counts = reconcile(&pool, tenant).await;
    assert_eq!(counts.created, 0, "re-running reconcile must not re-create");
    assert_eq!(
        catalog_count(&pool, tenant).await,
        1,
        "no duplicate records"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 14. Only signed events are ingested; reference-first by default
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn e2e_ingestion_requires_signed_events_only() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    // Defensive pre-cleanup: every test must be deterministic even if a
    // previous (possibly interrupted) run left rows for this tenant or for
    // the shared consumer id behind.
    cleanup(&pool, tenant).await;
    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    setup_tenant(
        &pool,
        tenant,
        &keys,
        &community,
        serde_json::json!({ "memory_projection": true, "content_indexing": false }),
    )
    .await;

    let service = service(pool.clone());
    let (buzz_push, _event) =
        created_push(&keys, "secret body", &community, ChatChannelKind::Workspace);

    // (a) Tampered Schnorr signature: the HMAC stays valid (it covers the
    // tampered payload), so the rejection is the Nostr verification.
    let mut value = serde_json::to_value(&buzz_push).unwrap();
    let mut sig = value["event"]["sig"]
        .as_str()
        .unwrap()
        .to_string()
        .into_bytes();
    sig[0] = if sig[0] == b'0' { b'1' } else { b'0' };
    value["event"]["sig"] = serde_json::json!(String::from_utf8(sig).unwrap());
    let err = push(&service, &serde_json::to_vec(&value).unwrap())
        .await
        .unwrap_err();
    assert!(
        matches!(&err, BuzzPushError::VerificationFailed),
        "a tampered signature must be rejected, got {err:?}"
    );

    // (b) Tampered event id: keep the raw `id` and the created-context
    // `message_id` consistent (the bridge's context check compares them), so
    // the rejection is the cryptographic one — `verify()` recomputes the
    // canonical id from the serialized event and the tampered id cannot match.
    let mut value = serde_json::to_value(&buzz_push).unwrap();
    let mut id = value["event"]["id"]
        .as_str()
        .unwrap()
        .to_string()
        .into_bytes();
    id[0] = if id[0] == b'0' { b'1' } else { b'0' };
    let tampered_id = String::from_utf8(id).unwrap();
    value["event"]["id"] = serde_json::json!(tampered_id.clone());
    value["context"]["message_id"] = serde_json::json!(tampered_id);
    let err = push(&service, &serde_json::to_vec(&value).unwrap())
        .await
        .unwrap_err();
    assert!(
        matches!(&err, BuzzPushError::VerificationFailed),
        "a tampered event id must be rejected, got {err:?}"
    );

    // Fail closed: NOTHING was written — no observation, no outbox, no catalog.
    assert_eq!(observation_count(&pool, tenant).await, 0);
    assert_eq!(outbox_count(&pool, tenant).await, 0);
    assert_eq!(catalog_count(&pool, tenant).await, 0);

    // (c) A valid push is ingested reference-first: the durable envelope
    // never carries the body, and the projected record has no content when
    // content_indexing is off.
    let store = outbox_store(pool.clone());
    register_consumer(&store).await;
    let (valid_push, valid_event) =
        created_push(&keys, "secret body", &community, ChatChannelKind::Workspace);
    assert_eq!(
        ingest_push(&service, &valid_push).await.unwrap(),
        IngestOutcome::FirstObservation
    );
    let envelope = envelope_from_outbox(&pool, &valid_event.id.to_hex()).await;
    assert!(envelope.data.get("body").is_none());
    assert!(envelope
        .data
        .as_object()
        .unwrap()
        .keys()
        .all(|key| key != "content"));
    let serialized_data = serde_json::to_string(&envelope.data).unwrap();
    assert!(
        !serialized_data.contains("secret body"),
        "the durable event must not carry the message body"
    );

    dispatch_once(&pool, store).await;
    let row = catalog_row(&pool, tenant).await.expect("record exists");
    assert_eq!(row.get::<String, _>("indexing_status"), "reference_only");
    assert_eq!(
        row.get::<Option<String>, _>("content"),
        None,
        "reference-first: no body copy without content_indexing"
    );

    cleanup(&pool, tenant).await;
}
