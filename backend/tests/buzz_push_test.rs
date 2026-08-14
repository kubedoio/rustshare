//! Integration tests for the Buzz observation bridge
//! ([`rustshare_server::buzz_observation::BuzzObservationService`]) end to
//! end against the dev database: signed Nostr event in, observation row +
//! durable outbox event out, all fail-closed rejections.
//!
//! DB-backed and `#[ignore]`d; run against the dev database (migrations
//! applied, including `20260810000005`) with `--test-threads=1`:
//!
//!   set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true \
//!     cargo test --test buzz_push_test -- --ignored --test-threads=1
//!
//! The chat tables are tenant-scoped and the outbox/delivery/receipt tables
//! are process-global, so every test takes a shared `SERIAL` guard and cleans
//! up exactly the rows it created (same convention as the outbox suite).

use chrono::Utc;
use nostr::{EventBuilder, Keys};
use rustshare_core::domain::{ApplicationRegistry, PrincipalId, TenantId};
use rustshare_crypto::WebhookSigner;
use rustshare_integration_events::event_types::CHAT_BUZZ_EVENT_OBSERVED_V1;
use rustshare_memory::event::{ChatChannelKind, ObservedEventType};
use rustshare_server::buzz_observation::{
    BuzzEventPush, BuzzObservationService, BuzzPushContext, BuzzPushError, IngestOutcome,
};
use rustshare_storage::{ChatIdentityStore, ChatObservationStore, OutboxStore};
use sqlx::{PgPool, Row};
use std::sync::{Arc, LazyLock};
use uuid::Uuid;

/// Serializes the tests within this binary (same convention as the outbox and
/// chat-observation suites).
static SERIAL: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

const WEBHOOK_SECRET: &str = "test-buzz-webhook-secret";
const COMMUNITY_ID: &str = "community-1";

/// Shared pool over `DATABASE_URL` with the same fallback the storage-layer
/// tests use.
async fn pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    PgPool::connect(&database_url)
        .await
        .expect("failed to connect to the dev database")
}

/// The service under test, backed by the canonical first-party Application
/// registry (the Chat manifest owns `io.elembra.chat.buzz.event.observed.v1`).
fn service(pool: PgPool) -> BuzzObservationService {
    let registry = Arc::new(ApplicationRegistry::first_party().unwrap());
    BuzzObservationService::new(
        pool.clone(),
        ChatIdentityStore::new(pool.clone()),
        ChatObservationStore::new(pool.clone()),
        Arc::new(OutboxStore::new(pool.clone(), registry)),
        WebhookSigner::new(WEBHOOK_SECRET),
        300,
        Arc::new(rustshare_core::events::EventBroadcaster::new(64)),
    )
}

/// Remove every row the tests create for `tenant_id`: the chat tables, the
/// chat Application enablement, and the outbox-side tables. The deliveries
/// FK-cascade from the outbox; receipts are not FK'd, so both are deleted
/// explicitly.
async fn cleanup(pool: &PgPool, tenant_id: TenantId) {
    for table in [
        "chat_observed_events",
        "memory_catalog",
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
    for table in [
        "integration_consumer_receipts",
        "integration_deliveries",
        "integration_outbox",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE tenant_id = $1"))
            .bind(tenant_id.0)
            .execute(pool)
            .await
            .unwrap();
    }
}

async fn insert_mapping(pool: &PgPool, tenant_id: TenantId, community_id: &str) -> Uuid {
    let mapping_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_workspace_communities
            (mapping_id, tenant_id, workspace_id, community_id, relay_url, active)
         VALUES ($1, $2, $3, $4, $5, true)",
    )
    .bind(mapping_id)
    .bind(tenant_id.0)
    .bind(tenant_id.0)
    .bind(community_id)
    .bind("wss://relay.example.test")
    .execute(pool)
    .await
    .unwrap();
    mapping_id
}

async fn insert_binding(pool: &PgPool, tenant_id: TenantId, pubkey: &str, status: &str) -> Uuid {
    let binding_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_identity_bindings
            (binding_id, tenant_id, principal_id, buzz_pubkey, status, verified_at, audit_metadata)
         VALUES ($1, $2, $3, $4, $5, now(), '{}'::jsonb)",
    )
    .bind(binding_id)
    .bind(tenant_id.0)
    .bind(PrincipalId::from(Uuid::new_v4()).0)
    .bind(pubkey)
    .bind(status)
    .execute(pool)
    .await
    .unwrap();
    binding_id
}

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

/// Standard happy-path tenant setup: active mapping, active binding for
/// `keys`, chat Application enabled with the given projection configuration.
async fn setup_tenant(
    pool: &PgPool,
    tenant_id: TenantId,
    keys: &Keys,
    configuration: serde_json::Value,
) {
    insert_mapping(pool, tenant_id, COMMUNITY_ID).await;
    insert_binding(pool, tenant_id, &keys.public_key().to_hex(), "active").await;
    enable_chat_application(pool, tenant_id, configuration).await;
}

/// Build a valid signed push of `kind` for `keys` (message_id == event id,
/// since a created event IS the message root).
fn signed_push_with_kind(
    keys: &Keys,
    content: &str,
    kind: nostr::Kind,
) -> (BuzzEventPush, nostr::Event) {
    let event = EventBuilder::new(kind, content)
        .sign_with_keys(keys)
        .expect("sign event");
    let push = BuzzEventPush {
        event: serde_json::to_value(&event).unwrap(),
        context: BuzzPushContext {
            community_id: COMMUNITY_ID.to_string(),
            channel_id: "channel-1".to_string(),
            channel_kind: ChatChannelKind::Workspace,
            thread_root_id: None,
            message_id: event.id.to_hex(),
            event_type: ObservedEventType::Created,
            supersedes_event_id: None,
        },
    };
    (push, event)
}

/// Build a valid signed text-note push (kind 1, legacy) for `keys`.
fn signed_push(keys: &Keys, content: &str) -> (BuzzEventPush, nostr::Event) {
    signed_push_with_kind(keys, content, nostr::Kind::TextNote)
}

fn sign_payload(signer: &WebhookSigner, payload: &[u8], timestamp: i64) -> String {
    signer
        .sign_with_timestamp(timestamp, payload)
        .expect("sign payload")
}

async fn observation_row(
    pool: &PgPool,
    tenant_id: TenantId,
    event_id: &str,
) -> sqlx::postgres::PgRow {
    sqlx::query(
        "SELECT event_id, message_id, event_type, supersedes_event_id, checksum, signature,
                signature_verified, body, author_pubkey, author_principal_id, event_created_at,
                active
         FROM chat_observed_events WHERE tenant_id = $1 AND event_id = $2",
    )
    .bind(tenant_id.0)
    .bind(event_id)
    .fetch_one(pool)
    .await
    .expect("observation row must exist")
}

// ---------------------------------------------------------------------------
// 1. First push: observation row + durable outbox event; duplicate: no-op
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn push_creates_observation_and_durable_event() {
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
    let signer = WebhookSigner::new(WEBHOOK_SECRET);
    let (push, event) = signed_push(&keys, "hello buzz");
    let payload = serde_json::to_vec(&push).unwrap();
    let signature = sign_payload(&signer, &payload, Utc::now().timestamp());
    let event_id = event.id.to_hex();

    let outcome = service
        .verify_and_ingest(&payload, &signature)
        .await
        .expect("first push must succeed");
    assert_eq!(outcome, IngestOutcome::FirstObservation);

    // Observation row: checksum/signature/signature_verified, no body.
    let row = observation_row(&pool, tenant, &event_id).await;
    let checksum: String = row.get("checksum");
    assert_eq!(checksum, format!("sha256:{event_id}"));
    let stored_signature: String = row.get("signature");
    assert_eq!(stored_signature, event.sig.to_string());
    assert!(row.get::<bool, _>("signature_verified"));
    assert!(row.get::<Option<String>, _>("body").is_none());
    assert_eq!(row.get::<String, _>("author_pubkey"), event.pubkey.to_hex());
    assert!(row.get::<Option<Uuid>, _>("author_principal_id").is_some());
    let created_at_ts: i64 = row
        .get::<chrono::DateTime<Utc>, _>("event_created_at")
        .timestamp();
    assert_eq!(created_at_ts, event.created_at.as_secs() as i64);
    assert!(row.get::<bool, _>("active"));

    // Durable outbox event: deterministic id, Buzz creation time, checksum.
    let expected_id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("elembra://io.elembra.chat/event/{event_id}").as_bytes(),
    );
    let outbox = sqlx::query(
        "SELECT event_id, event_json FROM integration_outbox
         WHERE tenant_id = $1 AND event_type = $2",
    )
    .bind(tenant.0)
    .bind(CHAT_BUZZ_EVENT_OBSERVED_V1)
    .fetch_one(&pool)
    .await
    .expect("outbox row must exist");
    let outbox_id: Uuid = outbox.get("event_id");
    assert_eq!(
        outbox_id, expected_id,
        "deterministic event id keyed on Buzz event id"
    );
    let event_json: serde_json::Value = outbox.get("event_json");
    assert_eq!(event_json["source"], "elembra://io.elembra.chat");
    assert_eq!(event_json["type"], CHAT_BUZZ_EVENT_OBSERVED_V1);
    assert_eq!(event_json["elembraTenant"], tenant.0.to_string());
    assert_eq!(event_json["elembraWorkspace"], tenant.0.to_string());
    assert_eq!(
        event_json["subject"],
        format!("elembra://io.elembra.chat/message/{event_id}")
    );
    let envelope_time =
        chrono::DateTime::parse_from_rfc3339(event_json["time"].as_str().expect("envelope time"))
            .unwrap();
    assert_eq!(
        envelope_time.timestamp(),
        event.created_at.as_secs() as i64,
        "envelope time is the Buzz event's creation time, not now"
    );
    assert_eq!(event_json["data"]["buzz"]["event_id"], event_id);
    assert_eq!(event_json["data"]["buzz"]["message_id"], event_id);
    assert_eq!(
        event_json["data"]["buzz"]["checksum"],
        format!("sha256:{event_id}")
    );
    assert_eq!(event_json["data"]["buzz"]["signature_verified"], true);
    assert_eq!(
        event_json["data"]["buzz"]["signature"],
        event.sig.to_string()
    );
    assert_eq!(event_json["data"]["buzz"]["pubkey"], event.pubkey.to_hex());
    assert_eq!(event_json["data"]["context"]["community_id"], COMMUNITY_ID);
    assert_eq!(event_json["data"]["context"]["channel_kind"], "workspace");
    assert!(event_json["data"].get("body").is_none());
    assert!(event_json["data"]
        .as_object()
        .unwrap()
        .keys()
        .all(|key| key != "content"));

    // Re-push of the identical payload: duplicate, still one row each.
    let outcome = service
        .verify_and_ingest(&payload, &signature)
        .await
        .expect("duplicate push must not error");
    assert_eq!(outcome, IngestOutcome::DuplicateObservation);

    let observation_count: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM chat_observed_events WHERE tenant_id = $1",
    )
    .bind(tenant.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        observation_count, 1,
        "duplicate must not create a second row"
    );
    let outbox_count: i64 =
        sqlx::query_scalar("SELECT count(*)::bigint FROM integration_outbox WHERE tenant_id = $1")
            .bind(tenant.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        outbox_count, 1,
        "durable event must be published exactly once"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 1a. Chat-message kinds: stream kind 9 is accepted; every other kind fails
//     closed at the kind gate (whitelist: TextNote legacy, 9, 40002)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn push_accepts_stream_message_kind_9() {
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
    let signer = WebhookSigner::new(WEBHOOK_SECRET);
    // Kind 9 (KIND_STREAM_MESSAGE) is Buzz's channel-scoped chat kind; the
    // push context carries the channel identity, so the existing context
    // shape is unchanged.
    let (push, event) = signed_push_with_kind(&keys, "stream message", nostr::Kind::Custom(9));
    let payload = serde_json::to_vec(&push).unwrap();
    let signature = sign_payload(&signer, &payload, Utc::now().timestamp());
    let event_id = event.id.to_hex();

    assert_eq!(
        service
            .verify_and_ingest(&payload, &signature)
            .await
            .expect("kind-9 push must succeed"),
        IngestOutcome::FirstObservation
    );

    // The observation row is written like any chat message.
    let row = observation_row(&pool, tenant, &event_id).await;
    assert_eq!(
        row.get::<String, _>("checksum"),
        format!("sha256:{event_id}")
    );
    assert!(row.get::<bool, _>("signature_verified"));
    assert_eq!(row.get::<String, _>("author_pubkey"), event.pubkey.to_hex());
    let outbox_count: i64 =
        sqlx::query_scalar("SELECT count(*)::bigint FROM integration_outbox WHERE tenant_id = $1")
            .bind(tenant.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(outbox_count, 1, "the durable event must be published");

    cleanup(&pool, tenant).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn push_rejects_non_chat_message_kind() {
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
    let signer = WebhookSigner::new(WEBHOOK_SECRET);
    // A validly signed event of an arbitrary kind (1000): id and signature
    // are fine, but the kind is not in the chat-message whitelist
    // (1, 9, 40002), so it must fail closed at the kind gate.
    let (push, _) = signed_push_with_kind(&keys, "hello buzz", nostr::Kind::Custom(1000));
    let payload = serde_json::to_vec(&push).unwrap();
    let signature = sign_payload(&signer, &payload, Utc::now().timestamp());

    let err = service
        .verify_and_ingest(&payload, &signature)
        .await
        .unwrap_err();
    assert!(
        matches!(err, BuzzPushError::VerificationFailed),
        "expected VerificationFailed, got {err:?}"
    );

    // Fail closed: nothing was written.
    let observation_count: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM chat_observed_events WHERE tenant_id = $1",
    )
    .bind(tenant.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(observation_count, 0, "no observation row may be written");
    let outbox_count: i64 =
        sqlx::query_scalar("SELECT count(*)::bigint FROM integration_outbox WHERE tenant_id = $1")
            .bind(tenant.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(outbox_count, 0, "no outbox row may be written");

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 1b. Edited event superseding the message root (`supersedes == message_id`)
//     is accepted: the root event's id IS the message id, so a first edit
//     legitimately supersedes it. Regression test for the push-context
//     identity rules.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn push_accepts_edited_event_superseding_message_root() {
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
    let signer = WebhookSigner::new(WEBHOOK_SECRET);

    // First push: the created event IS the message root.
    let (created_push, created_event) = signed_push(&keys, "v1");
    let message_id = created_event.id.to_hex();
    let payload = serde_json::to_vec(&created_push).unwrap();
    let signature = sign_payload(&signer, &payload, Utc::now().timestamp());
    assert_eq!(
        service
            .verify_and_ingest(&payload, &signature)
            .await
            .expect("created push must succeed"),
        IngestOutcome::FirstObservation
    );

    // Second push: an edited event superseding the message root. The created
    // event's id IS the message id, so `supersedes_event_id == message_id` is
    // the correct first-edit contract and must be accepted.
    let edit_event = EventBuilder::text_note("v2")
        .sign_with_keys(&keys)
        .expect("sign edit event");
    let edit_push = BuzzEventPush {
        event: serde_json::to_value(&edit_event).unwrap(),
        context: BuzzPushContext {
            community_id: COMMUNITY_ID.to_string(),
            channel_id: "channel-1".to_string(),
            channel_kind: ChatChannelKind::Workspace,
            thread_root_id: None,
            message_id: message_id.clone(),
            event_type: ObservedEventType::Edited,
            supersedes_event_id: Some(message_id.clone()),
        },
    };
    let payload = serde_json::to_vec(&edit_push).unwrap();
    let signature = sign_payload(&signer, &payload, Utc::now().timestamp());
    let edit_id = edit_event.id.to_hex();
    assert_eq!(
        service
            .verify_and_ingest(&payload, &signature)
            .await
            .expect("an edit superseding the message root must be accepted"),
        IngestOutcome::FirstObservation
    );

    // The edited observation row: stable message_id, its own event id, and the
    // superseded root recorded.
    let row = observation_row(&pool, tenant, &edit_id).await;
    assert_eq!(row.get::<String, _>("message_id"), message_id);
    assert_eq!(row.get::<String, _>("event_type"), "edited");
    let supersedes: Option<String> = row.get("supersedes_event_id");
    assert_eq!(
        supersedes.as_deref(),
        Some(message_id.as_str()),
        "the edit must record the superseded message root"
    );

    // Its durable outbox event: one per observation, deterministically keyed.
    let expected_id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("elembra://io.elembra.chat/event/{edit_id}").as_bytes(),
    );
    let outbox_rows = sqlx::query(
        "SELECT event_id FROM integration_outbox
         WHERE tenant_id = $1 AND event_type = $2",
    )
    .bind(tenant.0)
    .bind(CHAT_BUZZ_EVENT_OBSERVED_V1)
    .fetch_all(&pool)
    .await
    .expect("outbox rows must exist");
    assert_eq!(outbox_rows.len(), 2, "one durable event per observation");
    let ids: Vec<Uuid> = outbox_rows.iter().map(|r| r.get("event_id")).collect();
    assert!(
        ids.contains(&expected_id),
        "the edit envelope must be published"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 2. content_indexing=true: body stored on the observation row, never in the
//    durable envelope
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn push_with_content_indexing_stores_body() {
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
    let signer = WebhookSigner::new(WEBHOOK_SECRET);
    let (push, event) = signed_push(&keys, "message body v1");
    let payload = serde_json::to_vec(&push).unwrap();
    let signature = sign_payload(&signer, &payload, Utc::now().timestamp());

    assert_eq!(
        service
            .verify_and_ingest(&payload, &signature)
            .await
            .unwrap(),
        IngestOutcome::FirstObservation
    );

    let row = observation_row(&pool, tenant, &event.id.to_hex()).await;
    assert_eq!(
        row.get::<Option<String>, _>("body").as_deref(),
        Some("message body v1")
    );

    // The durable envelope carries reference metadata only — never the body.
    let event_json: serde_json::Value = sqlx::query_scalar(
        "SELECT event_json FROM integration_outbox WHERE tenant_id = $1 AND event_type = $2",
    )
    .bind(tenant.0)
    .bind(CHAT_BUZZ_EVENT_OBSERVED_V1)
    .fetch_one(&pool)
    .await
    .unwrap();
    let serialized_data = serde_json::to_string(&event_json["data"]).unwrap();
    assert!(
        !serialized_data.contains("message body v1"),
        "the durable event must not carry the message body"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 2b. Bodies are captured only for workspace channels: even with
//     content_indexing on, a dm-channel event must not store a body
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn dm_channel_body_not_stored_even_with_content_indexing() {
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
    let signer = WebhookSigner::new(WEBHOOK_SECRET);
    let event = EventBuilder::text_note("dm channel secret")
        .sign_with_keys(&keys)
        .expect("sign text note");
    let event_id = event.id.to_hex();
    let push = BuzzEventPush {
        event: serde_json::to_value(&event).unwrap(),
        context: BuzzPushContext {
            community_id: COMMUNITY_ID.to_string(),
            channel_id: "channel-1".to_string(),
            channel_kind: ChatChannelKind::Dm,
            thread_root_id: None,
            message_id: event_id.clone(),
            event_type: ObservedEventType::Created,
            supersedes_event_id: None,
        },
    };
    let payload = serde_json::to_vec(&push).unwrap();
    let signature = sign_payload(&signer, &payload, Utc::now().timestamp());

    assert_eq!(
        service
            .verify_and_ingest(&payload, &signature)
            .await
            .unwrap(),
        IngestOutcome::FirstObservation
    );

    // The observation row exists (dm events are still observed) but its body
    // is never captured: a never-eligible channel cannot leak a body into the
    // indexing copy under the tenant's opt-in.
    let row = observation_row(&pool, tenant, &event_id).await;
    assert!(
        row.get::<Option<String>, _>("body").is_none(),
        "a dm-channel body must never be stored, even with content_indexing on"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 3. Tampered Nostr signature ⇒ VerificationFailed
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn push_rejects_tampered_signature() {
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
    let signer = WebhookSigner::new(WEBHOOK_SECRET);
    let (push, _) = signed_push(&keys, "hello buzz");

    // Tamper the Schnorr signature in the payload, then sign the tampered
    // payload (the HMAC must remain valid so the failure is the Nostr check).
    let mut value = serde_json::to_value(&push).unwrap();
    let mut sig = value["event"]["sig"]
        .as_str()
        .unwrap()
        .to_string()
        .into_bytes();
    sig[0] = if sig[0] == b'0' { b'1' } else { b'0' };
    value["event"]["sig"] = serde_json::json!(String::from_utf8(sig).unwrap());
    let payload = serde_json::to_vec(&value).unwrap();
    let signature = sign_payload(&signer, &payload, Utc::now().timestamp());

    let err = service
        .verify_and_ingest(&payload, &signature)
        .await
        .unwrap_err();
    assert!(
        matches!(err, BuzzPushError::VerificationFailed),
        "expected VerificationFailed, got {err:?}"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 4. Unknown community ⇒ UnknownCommunity
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn push_rejects_unknown_community() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    let keys = Keys::generate();
    // No community mapping row at all.
    insert_binding(&pool, tenant, &keys.public_key().to_hex(), "active").await;
    enable_chat_application(
        &pool,
        tenant,
        serde_json::json!({ "memory_projection": true, "content_indexing": false }),
    )
    .await;

    let service = service(pool.clone());
    let signer = WebhookSigner::new(WEBHOOK_SECRET);
    let (push, _) = signed_push(&keys, "hello buzz");
    let payload = serde_json::to_vec(&push).unwrap();
    let signature = sign_payload(&signer, &payload, Utc::now().timestamp());

    let err = service
        .verify_and_ingest(&payload, &signature)
        .await
        .unwrap_err();
    assert!(
        matches!(err, BuzzPushError::UnknownCommunity),
        "expected UnknownCommunity, got {err:?}"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 5. Unbound author (no binding, or non-active binding) ⇒ UnboundAuthor
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn push_rejects_unbound_author() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    let keys = Keys::generate();
    insert_mapping(&pool, tenant, COMMUNITY_ID).await;
    enable_chat_application(
        &pool,
        tenant,
        serde_json::json!({ "memory_projection": true, "content_indexing": false }),
    )
    .await;

    let service = service(pool.clone());
    let signer = WebhookSigner::new(WEBHOOK_SECRET);
    let (push, _) = signed_push(&keys, "hello buzz");
    let payload = serde_json::to_vec(&push).unwrap();
    let signature = sign_payload(&signer, &payload, Utc::now().timestamp());

    // No binding row at all.
    let err = service
        .verify_and_ingest(&payload, &signature)
        .await
        .unwrap_err();
    assert!(
        matches!(err, BuzzPushError::UnboundAuthor),
        "expected UnboundAuthor without a binding, got {err:?}"
    );

    // A `pending` binding is not a live author either.
    insert_binding(&pool, tenant, &keys.public_key().to_hex(), "pending").await;
    let err = service
        .verify_and_ingest(&payload, &signature)
        .await
        .unwrap_err();
    assert!(
        matches!(err, BuzzPushError::UnboundAuthor),
        "a pending binding must not authorize an author, got {err:?}"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 6. Bad HMAC / replay window ⇒ Unauthorized
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn push_rejects_bad_hmac() {
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
    let signer = WebhookSigner::new(WEBHOOK_SECRET);
    let (push, _) = signed_push(&keys, "hello buzz");
    let payload = serde_json::to_vec(&push).unwrap();
    let now = Utc::now().timestamp();

    // Missing header value.
    let err = service.verify_and_ingest(&payload, "").await.unwrap_err();
    assert!(matches!(err, BuzzPushError::Unauthorized));

    // Signed with the wrong secret.
    let wrong_signer = WebhookSigner::new("some-other-secret");
    let wrong = sign_payload(&wrong_signer, &payload, now);
    let err = service
        .verify_and_ingest(&payload, &wrong)
        .await
        .unwrap_err();
    assert!(matches!(err, BuzzPushError::Unauthorized));

    // Valid HMAC but outside the replay window (signed in the past).
    let expired = sign_payload(&signer, &payload, now - 3600);
    let err = service
        .verify_and_ingest(&payload, &expired)
        .await
        .unwrap_err();
    assert!(matches!(err, BuzzPushError::Unauthorized));

    // Non-timestamped `v1=` signature: the replay window is unenforceable,
    // so it fails closed.
    let plain = signer.sign(&payload).unwrap();
    let err = service
        .verify_and_ingest(&payload, &plain)
        .await
        .unwrap_err();
    assert!(matches!(err, BuzzPushError::Unauthorized));

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 7. One ACTIVE mapping per community_id globally (migration 20260810000005):
//    the partial unique index `chat_workspace_communities_active_community` is
//    the PRIMARY defense against the cross-tenant ambiguity that
//    `mapping_by_community` would otherwise ingest into an arbitrary tenant.
//    `mapping_by_community`'s multi-row check (which returns a distinct
//    `CommunityMappingError::Ambiguous`) is defense-in-depth: it is
//    unreachable through this schema because the index refuses the second
//    active mapping, but it is retained in case the index is ever dropped or
//    rows are imported from outside the schema.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn second_active_mapping_for_community_is_refused_and_no_cross_tenant_write() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant_a = TenantId::from(Uuid::new_v4());
    let tenant_b = TenantId::from(Uuid::new_v4());
    let keys = Keys::generate();
    setup_tenant(
        &pool,
        tenant_a,
        &keys,
        serde_json::json!({ "memory_projection": true, "content_indexing": false }),
    )
    .await;

    // (a) The unique active-community index blocks a second ACTIVE mapping
    // for the same community_id, so the ambiguous state is unrepresentable.
    let err = sqlx::query(
        "INSERT INTO chat_workspace_communities
            (mapping_id, tenant_id, workspace_id, community_id, relay_url, active)
         VALUES ($1, $2, $3, $4, $5, true)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_b.0)
    .bind(tenant_b.0)
    .bind(COMMUNITY_ID)
    .bind("wss://relay.example.test")
    .execute(&pool)
    .await
    .expect_err("a second active mapping must violate the unique index");
    assert_eq!(
        err.as_database_error()
            .expect("unique violation is a database error")
            .constraint(),
        Some("chat_workspace_communities_active_community"),
        "unexpected error: {err}"
    );

    // A deactivated mapping frees the community for another tenant, so tenant B
    // may hold the same community_id while INACTIVE.
    sqlx::query(
        "INSERT INTO chat_workspace_communities
            (mapping_id, tenant_id, workspace_id, community_id, relay_url, active)
         VALUES ($1, $2, $3, $4, $5, false)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_b.0)
    .bind(tenant_b.0)
    .bind(COMMUNITY_ID)
    .bind("wss://relay.example.test")
    .execute(&pool)
    .await
    .expect("an inactive mapping for the same community_id must be allowed");

    // Re-activating tenant B's mapping would recreate the ambiguity, so the
    // index must refuse the flip too.
    let err = sqlx::query(
        "UPDATE chat_workspace_communities SET active = true
         WHERE tenant_id = $1 AND community_id = $2",
    )
    .bind(tenant_b.0)
    .bind(COMMUNITY_ID)
    .execute(&pool)
    .await
    .expect_err("re-activating the second mapping must violate the unique index");
    assert_eq!(
        err.as_database_error()
            .expect("unique violation is a database error")
            .constraint(),
        Some("chat_workspace_communities_active_community"),
        "unexpected error: {err}"
    );

    // (b) The store lookup still resolves the unambiguous single-row case and
    // never surfaces the inactive tenant-B mapping.
    let store = ChatIdentityStore::new(pool.clone());
    let found = store
        .mapping_by_community(COMMUNITY_ID)
        .await
        .expect("single-row lookup must succeed")
        .expect("tenant A's active mapping must be found");
    assert_eq!(found.tenant_id, tenant_a);
    assert!(found.active);

    // Push under tenant A succeeds and lands ONLY in tenant A: no observation
    // row and no outbox row ever exist for tenant B.
    let service = service(pool.clone());
    let signer = WebhookSigner::new(WEBHOOK_SECRET);
    let (push, event) = signed_push(&keys, "hello buzz");
    let payload = serde_json::to_vec(&push).unwrap();
    let signature = sign_payload(&signer, &payload, Utc::now().timestamp());
    assert_eq!(
        service
            .verify_and_ingest(&payload, &signature)
            .await
            .expect("push under tenant A must succeed"),
        IngestOutcome::FirstObservation
    );
    assert!(
        observation_row(&pool, tenant_a, &event.id.to_hex())
            .await
            .get::<Option<Uuid>, _>("author_principal_id")
            .is_some(),
        "observation row written under tenant A"
    );
    let tenant_b_observations: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM chat_observed_events WHERE tenant_id = $1",
    )
    .bind(tenant_b.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        tenant_b_observations, 0,
        "no observation row may be written for tenant B"
    );
    let tenant_b_outbox: i64 =
        sqlx::query_scalar("SELECT count(*)::bigint FROM integration_outbox WHERE tenant_id = $1")
            .bind(tenant_b.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        tenant_b_outbox, 0,
        "no outbox row may be written for tenant B"
    );

    cleanup(&pool, tenant_a).await;
    cleanup(&pool, tenant_b).await;
}

// ---------------------------------------------------------------------------
// 8. Mapping violating the workspace == tenant invariant ⇒ Persistence
//    (server-side integrity failure, fail closed)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn push_rejects_mapping_violating_workspace_tenant_invariant() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    let keys = Keys::generate();

    // Active mapping whose workspace_id differs from tenant_id: a server-side
    // integrity violation, not a malformed client payload.
    sqlx::query(
        "INSERT INTO chat_workspace_communities
            (mapping_id, tenant_id, workspace_id, community_id, relay_url, active)
         VALUES ($1, $2, $3, $4, $5, true)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant.0)
    .bind(Uuid::new_v4())
    .bind(COMMUNITY_ID)
    .bind("wss://relay.example.test")
    .execute(&pool)
    .await
    .unwrap();
    insert_binding(&pool, tenant, &keys.public_key().to_hex(), "active").await;
    enable_chat_application(
        &pool,
        tenant,
        serde_json::json!({ "memory_projection": true, "content_indexing": false }),
    )
    .await;

    let service = service(pool.clone());
    let signer = WebhookSigner::new(WEBHOOK_SECRET);
    let (push, _) = signed_push(&keys, "hello buzz");
    let payload = serde_json::to_vec(&push).unwrap();
    let signature = sign_payload(&signer, &payload, Utc::now().timestamp());

    let err = service
        .verify_and_ingest(&payload, &signature)
        .await
        .unwrap_err();
    assert!(
        matches!(err, BuzzPushError::Persistence(_)),
        "a mapping violating the workspace == tenant invariant is a server-side failure, got {err:?}"
    );
    let details = err.to_string();
    assert!(
        details.contains("workspace == tenant"),
        "error should name the invariant, got: {details}"
    );

    // Fail closed: nothing was written.
    let observation_count: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM chat_observed_events WHERE tenant_id = $1",
    )
    .bind(tenant.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(observation_count, 0, "no observation row may be written");
    let outbox_count: i64 =
        sqlx::query_scalar("SELECT count(*)::bigint FROM integration_outbox WHERE tenant_id = $1")
            .bind(tenant.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(outbox_count, 0, "no outbox row may be written");

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 1d. Author-controlled `created_at` is bounded: a future-dated event would
//     pin itself above any later delete/edit (the fold orders by
//     `event_created_at DESC`; the tombstone window is `>= since`), so pushes
//     beyond the clock-skew allowance are rejected as malformed while
//     within-skew events are accepted
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn future_dated_event_is_rejected_within_skew_accepted() {
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
    let signer = WebhookSigner::new(WEBHOOK_SECRET);

    let signed_with_created_at = |content: &str, created_at: nostr::Timestamp| {
        let event = EventBuilder::text_note(content)
            .custom_created_at(created_at)
            .sign_with_keys(&keys)
            .expect("sign text note");
        BuzzEventPush {
            event: serde_json::to_value(&event).unwrap(),
            context: BuzzPushContext {
                community_id: COMMUNITY_ID.to_string(),
                channel_id: "channel-1".to_string(),
                channel_kind: ChatChannelKind::Workspace,
                thread_root_id: None,
                message_id: event.id.to_hex(),
                event_type: ObservedEventType::Created,
                supersedes_event_id: None,
            },
        }
    };

    // +1 hour: far beyond the 15-minute skew — permanent rejection (400-class
    // malformed), and nothing is persisted.
    let future = nostr::Timestamp::from_secs((Utc::now().timestamp() + 3600) as u64);
    let push = signed_with_created_at("future pin", future);
    let payload = serde_json::to_vec(&push).unwrap();
    let signature = sign_payload(&signer, &payload, Utc::now().timestamp());
    let err = service
        .verify_and_ingest(&payload, &signature)
        .await
        .expect_err("a future-dated event must be rejected");
    assert!(
        matches!(err, BuzzPushError::Malformed(_)),
        "rejection must be permanent (malformed), got {err:?}"
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM chat_observed_events WHERE tenant_id = $1",
    )
    .bind(tenant.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        count, 0,
        "no observation row may be written for a future event"
    );

    // +5 minutes: inside the skew allowance (clock drift) — accepted.
    let skew = nostr::Timestamp::from_secs((Utc::now().timestamp() + 300) as u64);
    let push = signed_with_created_at("within skew", skew);
    let payload = serde_json::to_vec(&push).unwrap();
    let signature = sign_payload(&signer, &payload, Utc::now().timestamp());
    let outcome = service
        .verify_and_ingest(&payload, &signature)
        .await
        .expect("an event within the skew allowance must be accepted");
    assert_eq!(outcome, IngestOutcome::FirstObservation);

    cleanup(&pool, tenant).await;
}
