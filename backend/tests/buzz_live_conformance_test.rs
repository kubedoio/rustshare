//! LIVE conformance suite: Elembra against a REAL Buzz relay (v1alpha1
//! upstream authorization contract, `docs/specs/buzz-upstream-authorization-v1alpha1.md`).
//!
//! Unlike the fake-relay suites, this suite runs against the real relay built
//! from the `feat/relay-authorization-v1alpha1` worktree (see
//! `scripts/run-buzz-conformance.sh`): real NIP-98-authenticated access
//! checks, real kind-19030 signed responses, real channel registry and real
//! stream-message state.
//!
//! Architecture: in-process Elembra (AppState built exactly like the
//! fake-relay suite's buzz-mode wiring: `ChatResourceOwner::with_authority`
//! with `buzz_gateway: Some`), the gateway pointed at the REAL relay, and
//! the dev Elembra DB as the store (fresh tenant per test).
//!
//! The suite SEEDS the relay itself over its public HTTP surface
//! (`POST /events`, dev-mode X-Pubkey auth — the operator's tooling path),
//! and seeds Elembra by ingesting the SAME signed events through the real
//! in-process observation bridge. The gateway's NIP-98 path and the relay's
//! pinned trusted-service gate are what the proofs exercise.
//!
//! Env-gated (`#[ignore = "requires live Buzz relay"]`):
//!
//!   RUSTSHARE_BUZZ_LIVE_RELAY_URL     ws relay URL (default ws://127.0.0.1:7447)
//!   RUSTSHARE_BUZZ_LIVE_SERVICE_SK    Elembra service/owner secret key (64 hex)
//!   RUSTSHARE_BUZZ_LIVE_RELAY_PUBKEY  the relay's identity pubkey (64 hex pin)
//!   RUSTSHARE_BUZZ_LIVE_METRICS_URL   relay metrics endpoint (default http://127.0.0.1:9102)
//!
//! Run (script): `scripts/run-buzz-conformance.sh` — builds the relay image,
//! brings up the stack with `RELAY_TRUSTED_SERVICE_PUBKEYS=<service pk>` and
//! `RELAY_URL=ws://127.0.0.1:7447`, then:
//!
//!   set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true \
//!     cargo test -p rustshare-server --test buzz_live_conformance_test -- \
//!       --ignored --test-threads=1

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use axum::extract::State;
use chrono::Utc;
use nostr::{Event as NostrEvent, EventBuilder, Keys, Kind, Tag};
use reqwest::Client;
use rustshare_core::domain::{
    ActionCapability, ApplicationId, ApplicationRegistry, PrincipalId, TenantId, WorkspaceId,
};
use rustshare_core::events::EventBroadcaster;
use rustshare_core::services::{
    ChatIntegrationService, FileService, FolderService, HttpWebhookDispatcher, NotificationService,
    PermissionResolver, ShareService, ThumbnailService, VaultSyncService,
};
use rustshare_crypto::{SecretEncryptionKey, WebhookSigner};
use rustshare_infrastructure::repositories::{
    FileRepository, FolderRepository, NotificationRepository, PermissionResolverRepository,
    ShareRepository, UserRepository,
};
use rustshare_integration_events::event_types::CHAT_BUZZ_EVENT_OBSERVED_V1;
use rustshare_integration_events::OutboxConsumer;
use rustshare_memory::event::{ChatChannelKind, ObservedEventType};
use rustshare_resource_auth::{
    Decision, PrincipalContext, Representation, ResourceOwner, ResourceOwnerRegistry, ResourceRef,
    SourceAuthorizer, SourceError, CHAT_READ,
};
use rustshare_server::authz::ChatResourceOwner;
use rustshare_server::buzz_gateway::{BuzzGatewayAuthority, BuzzGatewayClient};
use rustshare_server::buzz_observation::{
    BuzzEventPush, BuzzObservationService, BuzzPushContext, IngestOutcome,
};
use rustshare_server::config::OutboxWorkerConfig;
use rustshare_server::handlers::chat_app::{list_channels, ChannelInfo};
use rustshare_server::handlers::extractors::AuthenticatedUser;
use rustshare_server::memory_projection::MemoryChatProjectionConsumer;
use rustshare_server::middleware::RateLimitConfig;
use rustshare_server::oidc_runtime::OidcRuntimeCache;
use rustshare_server::outbox_dispatcher::{OutboxDispatcher, OutboxStatus};
use rustshare_server::services::ask_workspace::AskWorkspaceService;
use rustshare_server::services::note_service::NoteService;
use rustshare_server::services::unified_search::{SearchSource, UnifiedSearchService};
use rustshare_server::AppState;
use rustshare_storage::repos::ShareNotificationRepoImpl;
use rustshare_storage::{
    ChatIdentityStore, ChatObservationStore, EventStore, MemoryCatalogStore, MetadataStore,
    ObjectStore, ObjectStoreOptions, OutboxStore,
};
use sqlx::PgPool;
use uuid::Uuid;

/// Serializes the tests within this binary (same convention as the other
/// DB-backed suites).
static SERIAL: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

const TEST_CONSUMER_ID: &str = "io.elembra.memory.chat-projection.buzz-conformance.v1";

/// Shared pool over `DATABASE_URL` with the same fallback the storage-layer
/// tests use.
async fn pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    PgPool::connect(&database_url)
        .await
        .expect("failed to connect to the dev database")
}

/// Remove every row the tests create for `tenant_id`.
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
    for table in ["integration_deliveries", "integration_outbox"] {
        sqlx::query(&format!("DELETE FROM {table} WHERE tenant_id = $1"))
            .bind(tenant_id.0)
            .execute(pool)
            .await
            .unwrap();
    }
}

// ---------------------------------------------------------------------------
// Live relay env + seeding over the relay's public HTTP surface
// ---------------------------------------------------------------------------

/// Live-relay configuration from the environment.
#[derive(Debug, Clone)]
struct LiveEnv {
    /// WebSocket relay URL, also the mapping's `relay_url` value.
    relay_ws: String,
    /// HTTP base derived from the ws URL (the gateway's target).
    relay_http: String,
    /// The relay's identity pubkey (mapping pin).
    relay_pubkey: String,
    /// The Elembra service / relay-owner key (NIP-98 signer + admin ops).
    service_keys: Keys,
    /// Prometheus metrics endpoint of the relay.
    metrics_url: String,
}

impl LiveEnv {
    fn load() -> Option<Self> {
        let relay_ws = std::env::var("RUSTSHARE_BUZZ_LIVE_RELAY_URL")
            .or_else(|_| std::env::var("RUSTSHARE_BUZZ_LIVE_RELAY_WS"))
            .unwrap_or_else(|_| "ws://127.0.0.1:7447".to_string());
        let service_sk = std::env::var("RUSTSHARE_BUZZ_LIVE_SERVICE_SK").ok()?;
        let relay_pubkey = std::env::var("RUSTSHARE_BUZZ_LIVE_RELAY_PUBKEY").ok()?;
        let metrics_url = std::env::var("RUSTSHARE_BUZZ_LIVE_METRICS_URL").unwrap_or_else(|_| {
            format!(
                "http://{}/metrics",
                relay_ws
                    .strip_prefix("ws://")
                    .or_else(|| relay_ws.strip_prefix("wss://"))
                    .unwrap_or("127.0.0.1:7447")
                    .split(':')
                    .next()
                    .map(|host| format!("{host}:9102"))
                    .unwrap_or_else(|| "127.0.0.1:9102".to_string())
            )
        });
        let service_keys = Keys::parse(&service_sk).ok()?;
        let relay_http = relay_ws
            .strip_prefix("wss://")
            .map(|rest| format!("https://{rest}"))
            .or_else(|| {
                relay_ws
                    .strip_prefix("ws://")
                    .map(|rest| format!("http://{rest}"))
            })?;
        Some(Self {
            relay_http,
            relay_ws,
            relay_pubkey,
            service_keys,
            metrics_url,
        })
    }
}

/// Submit a signed event to the relay's public HTTP surface (`POST /events`,
/// dev-mode X-Pubkey auth — the operator's tooling path; the Elembra gateway
/// itself only ever uses NIP-98). Returns the relay-accepted event id.
async fn relay_submit(
    env: &LiveEnv,
    keys: &Keys,
    kind: u16,
    tags: Vec<Tag>,
    content: &str,
) -> String {
    let event = EventBuilder::new(Kind::from(kind), content)
        .tags(tags)
        .sign_with_keys(keys)
        .expect("sign the relay event");
    let response = Client::new()
        .post(format!("{}/events", env.relay_http))
        .header("X-Pubkey", keys.public_key().to_hex())
        .json(&serde_json::to_value(&event).expect("serialize the relay event"))
        .send()
        .await
        .unwrap_or_else(|e| panic!("relay must be reachable at {}: {e}", env.relay_http));
    assert_eq!(
        response.status(),
        reqwest::StatusCode::OK,
        "POST /events must succeed"
    );
    let body: serde_json::Value = response.json().await.expect("relay answers JSON");
    // Pin the acceptance field: a silently-unlanded seed must fail loudly
    // here instead of surfacing as a confusing downstream assertion.
    let accepted = body
        .get("accepted")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or_else(|| {
            panic!("relay /events response must carry the `accepted` field: {body}")
        });
    assert!(accepted, "relay must accept the seed event: {body}");
    event.id.to_hex()
}

/// Create a channel on the relay (kind-9007, NIP-29 create-group), signed by
/// the owner. `visibility` is `open|private`, `channel_type` `stream|forum|dm|workflow`.
async fn relay_create_channel(
    env: &LiveEnv,
    channel_uuid: &str,
    name: &str,
    visibility: &str,
    channel_type: &str,
) {
    relay_submit(
        env,
        &env.service_keys,
        9007,
        vec![
            Tag::parse(["h", channel_uuid]).expect("h tag"),
            Tag::parse(["name", name]).expect("name tag"),
            Tag::parse(["visibility", visibility]).expect("visibility tag"),
            Tag::parse(["channel_type", channel_type]).expect("channel_type tag"),
        ],
        "conformance channel",
    )
    .await;
}

/// Admit `pubkey` to the community (kind-9030, owner authority).
async fn relay_admit_member(env: &LiveEnv, pubkey: &str) {
    relay_submit(
        env,
        &env.service_keys,
        9030,
        vec![Tag::parse(["p", pubkey]).expect("p tag")],
        "conformance admit",
    )
    .await;
}

/// Add `pubkey` to a channel's membership (kind-9000 put-user, owner
/// authority) — the channel-level membership the access checks read.
async fn relay_put_user(env: &LiveEnv, channel_uuid: &str, pubkey: &str) {
    relay_submit(
        env,
        &env.service_keys,
        9000,
        vec![
            Tag::parse(["h", channel_uuid]).expect("h tag"),
            Tag::parse(["p", pubkey]).expect("p tag"),
        ],
        "conformance put-user",
    )
    .await;
}

/// Remove `pubkey` from a channel's membership (kind-9001 remove-user).
async fn relay_remove_user(env: &LiveEnv, channel_uuid: &str, pubkey: &str) {
    relay_submit(
        env,
        &env.service_keys,
        9001,
        vec![
            Tag::parse(["h", channel_uuid]).expect("h tag"),
            Tag::parse(["p", pubkey]).expect("p tag"),
        ],
        "conformance remove-user",
    )
    .await;
}

/// Remove `pubkey` from the community (kind-9031).
async fn relay_revoke_member(env: &LiveEnv, pubkey: &str) {
    relay_submit(
        env,
        &env.service_keys,
        9031,
        vec![Tag::parse(["p", pubkey]).expect("p tag")],
        "conformance revoke",
    )
    .await;
}

/// Publish a kind-9 stream message to the relay (canonical chat wire format:
/// `["h", <channel-uuid>]`), signed by `keys`. Returns the signed event.
async fn relay_publish_message(
    env: &LiveEnv,
    keys: &Keys,
    channel_uuid: &str,
    content: &str,
) -> NostrEvent {
    let event = EventBuilder::new(Kind::Custom(9), content)
        .tags(vec![Tag::parse(["h", channel_uuid]).expect("h tag")])
        .sign_with_keys(keys)
        .expect("sign the stream message");
    let response = Client::new()
        .post(format!("{}/events", env.relay_http))
        .header("X-Pubkey", keys.public_key().to_hex())
        .json(&serde_json::to_value(&event).expect("serialize the stream message"))
        .send()
        .await
        .expect("relay must be reachable");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("relay answers JSON");
    // Pin the acceptance field: a silently-unlanded seed must fail loudly
    // here instead of surfacing as a confusing downstream assertion.
    let accepted = body
        .get("accepted")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or_else(|| {
            panic!("relay /events response must carry the `accepted` field: {body}")
        });
    assert!(
        accepted,
        "relay must accept the stream message (is the publisher a channel member?): {body}"
    );
    event
}

/// Publish a message into a channel the READER may not publish to (e.g. a
/// private channel the reader cannot access): signed by the OWNER, who has
/// admin scope on every channel. The access checks evaluate the READER's
/// membership, never the author's, so the denial is the reader's.
async fn relay_publish_message_as_owner(
    env: &LiveEnv,
    channel_uuid: &str,
    content: &str,
) -> NostrEvent {
    relay_publish_message(env, &env.service_keys, channel_uuid, content).await
}

/// Count relay HTTP requests whose route pattern contains `needle` (the
/// metrics endpoint exports `http_requests_total{...,action="<route>"}`).
async fn relay_route_count(env: &LiveEnv, needle: &str) -> u64 {
    let body = Client::new()
        .get(&env.metrics_url)
        .send()
        .await
        .expect("relay metrics must be reachable")
        .text()
        .await
        .expect("metrics body");
    body.lines()
        .filter(|line| line.contains("http_requests_total") && line.contains(needle))
        .map(|line| {
            line.rsplit(' ')
                .next()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0)
        })
        .sum()
}

// ---------------------------------------------------------------------------
// Elembra-side fixture (fresh tenant, in-process stores + gateway)
// ---------------------------------------------------------------------------

struct Fixture {
    tenant: TenantId,
    principal: PrincipalId,
    community_id: String,
    user_keys: Keys,
}

/// Fresh tenant wired to the live relay: active mapping (pinned to the live
/// relay's pubkey), active binding + admission for `user_keys`, Chat enabled.
async fn setup_tenant(
    pool: &PgPool,
    env: &LiveEnv,
    user_keys: &Keys,
    configuration: serde_json::Value,
) -> Fixture {
    let tenant = TenantId::from(Uuid::new_v4());
    cleanup(pool, tenant).await;
    let community_id = format!("community-{}", Uuid::new_v4());
    let mapping_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_workspace_communities
            (mapping_id, tenant_id, workspace_id, community_id, relay_url, relay_pubkey, active)
         VALUES ($1, $2, $3, $4, $5, $6, true)",
    )
    .bind(mapping_id)
    .bind(tenant.0)
    .bind(tenant.0)
    .bind(&community_id)
    .bind(&env.relay_ws)
    .bind(&env.relay_pubkey)
    .execute(pool)
    .await
    .unwrap();
    let principal = PrincipalId::from(Uuid::new_v4());
    let binding_id = Uuid::new_v4();
    let pubkey = user_keys.public_key().to_hex();
    sqlx::query(
        "INSERT INTO chat_identity_bindings
            (binding_id, tenant_id, principal_id, buzz_pubkey, status, verified_at, audit_metadata)
         VALUES ($1, $2, $3, $4, 'active', now(), '{}'::jsonb)",
    )
    .bind(binding_id)
    .bind(tenant.0)
    .bind(principal.0)
    .bind(&pubkey)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO chat_buzz_admissions
            (admission_id, tenant_id, mapping_id, binding_id, buzz_pubkey, active)
         VALUES ($1, $2, $3, $4, $5, true)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant.0)
    .bind(mapping_id)
    .bind(binding_id)
    .bind(&pubkey)
    .execute(pool)
    .await
    .unwrap();
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
    Fixture {
        tenant,
        principal,
        community_id,
        user_keys: user_keys.clone(),
    }
}

/// The gateway client for the live relay, built from the service key.
fn live_gateway(env: &LiveEnv) -> Arc<BuzzGatewayClient> {
    Arc::new(
        BuzzGatewayClient::new_for_test(env.service_keys.clone(), Client::builder())
            .expect("build the live gateway"),
    )
}

/// In-process observation bridge (real HMAC + signature verification).
fn observation_service(pool: PgPool) -> BuzzObservationService {
    let registry = Arc::new(ApplicationRegistry::first_party().unwrap());
    BuzzObservationService::new(
        pool.clone(),
        ChatIdentityStore::new(pool.clone()),
        ChatObservationStore::new(pool.clone()),
        Arc::new(OutboxStore::new(pool.clone(), registry)),
        WebhookSigner::new("conformance-webhook-secret"),
        300,
        Arc::new(EventBroadcaster::new(64)),
    )
}

/// Ingest one signed relay event into Elembra through the real webhook path
/// (the observation row the gate's pre-filter needs), into the tenant that
/// owns `community_id`.
async fn ingest_event_into(
    service: &BuzzObservationService,
    community_id: &str,
    event: &NostrEvent,
) {
    let push = BuzzEventPush {
        event: serde_json::to_value(event).unwrap(),
        context: BuzzPushContext {
            community_id: community_id.to_string(),
            channel_id: channel_of(event).to_string(),
            channel_kind: ChatChannelKind::Workspace,
            thread_root_id: None,
            message_id: event.id.to_hex(),
            event_type: ObservedEventType::Created,
            supersedes_event_id: None,
        },
    };
    let payload = serde_json::to_vec(&push).unwrap();
    let signature = WebhookSigner::new("conformance-webhook-secret")
        .sign_with_timestamp(Utc::now().timestamp(), &payload)
        .expect("sign the webhook payload");
    assert_eq!(
        service
            .verify_and_ingest(&payload, &signature)
            .await
            .expect("webhook ingestion must succeed"),
        IngestOutcome::FirstObservation
    );
}

/// The channel uuid of a signed stream message (`["h", <uuid>]`).
fn channel_of(event: &NostrEvent) -> &str {
    event
        .tags
        .iter()
        .find(|tag| tag.kind().to_string() == "h")
        .and_then(|tag| tag.content())
        .expect("stream message carries an h tag")
}

/// Bind an additional pubkey into the tenant (binding + admission) so the
/// observation bridge accepts messages authored by it (the bridge requires a
/// live binding for the EVENT author). Used for owner-authored fixtures.
async fn bind_pubkey(pool: &PgPool, tenant: TenantId, community_id: &str, pubkey: &str) {
    let principal = PrincipalId::from(Uuid::new_v4());
    let binding_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_identity_bindings
            (binding_id, tenant_id, principal_id, buzz_pubkey, status, verified_at, audit_metadata)
         VALUES ($1, $2, $3, $4, 'active', now(), '{}'::jsonb)",
    )
    .bind(binding_id)
    .bind(tenant.0)
    .bind(principal.0)
    .bind(pubkey)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO chat_buzz_admissions
            (admission_id, tenant_id, mapping_id, binding_id, buzz_pubkey, active)
         SELECT $1, $2, mapping_id, $3, $4, true
         FROM chat_workspace_communities
         WHERE tenant_id = $2 AND community_id = $5",
    )
    .bind(Uuid::new_v4())
    .bind(tenant.0)
    .bind(binding_id)
    .bind(pubkey)
    .bind(community_id)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed one message end-to-end: publish to the live relay (kind 9, h tag),
/// ingest the SAME signed event into Elembra. Returns the ref.
async fn seed_message(
    env: &LiveEnv,
    obs_service: &BuzzObservationService,
    fixture: &Fixture,
    channel_uuid: &str,
    content: &str,
) -> ResourceRef {
    let event = relay_publish_message(env, &fixture.user_keys, channel_uuid, content).await;
    ingest_event_into(obs_service, &fixture.community_id, &event).await;
    chat_ref(&event.id.to_hex())
}

/// Seed a private channel (created by the owner) with `user_keys` admitted
/// as a member; returns the channel uuid.
async fn seed_private_channel(env: &LiveEnv, fixture: &Fixture) -> String {
    let channel_uuid = Uuid::new_v4().to_string();
    relay_create_channel(
        env,
        &channel_uuid,
        "conformance-private",
        "private",
        "stream",
    )
    .await;
    relay_admit_member(env, &fixture.user_keys.public_key().to_hex()).await;
    relay_put_user(env, &channel_uuid, &fixture.user_keys.public_key().to_hex()).await;
    channel_uuid
}

/// Seed an open channel (created by the owner); no membership needed to read.
async fn seed_open_channel(env: &LiveEnv, fixture: &Fixture) -> String {
    let channel_uuid = Uuid::new_v4().to_string();
    relay_create_channel(env, &channel_uuid, "conformance-open", "open", "stream").await;
    relay_admit_member(env, &fixture.user_keys.public_key().to_hex()).await;
    channel_uuid
}

fn chat_ref(message_id: &str) -> ResourceRef {
    ResourceRef::new(ApplicationId::new("io.elembra.chat"), "message", message_id)
}

fn user_ctx(principal: PrincipalId, tenant: TenantId) -> PrincipalContext {
    PrincipalContext::user(principal, tenant, WorkspaceId(tenant.0))
}

fn chat_read_action() -> ActionCapability {
    ActionCapability::new(CHAT_READ)
}

fn auth(principal: PrincipalId, tenant: TenantId) -> AuthenticatedUser {
    AuthenticatedUser {
        user_id: principal.0,
        tenant_id: tenant.0,
    }
}

// ---------------------------------------------------------------------------
// AppState (buzz mode, gateway → live relay) — same graph the fake-relay
// suite constructs.
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_lines)]
async fn setup_app_state(
    pool: PgPool,
    gateway: Arc<BuzzGatewayClient>,
) -> (AppState, ChatIdentityStore, Arc<ChatObservationStore>) {
    let metadata_store = Arc::new(MetadataStore::new(pool.clone()));
    let event_store = Arc::new(EventStore::new(pool.clone()));
    let broadcaster = Arc::new(EventBroadcaster::new(100));

    let s3_endpoint = std::env::var("S3_ENDPOINT")
        .or_else(|_| std::env::var("RUSTFS_ENDPOINT"))
        .unwrap_or_else(|_| "http://localhost:9000".to_string());
    let s3_region = std::env::var("S3_REGION")
        .or_else(|_| std::env::var("RUSTFS_REGION"))
        .unwrap_or_else(|_| "us-east-1".to_string());
    let s3_bucket = std::env::var("S3_BUCKET")
        .or_else(|_| std::env::var("RUSTFS_BUCKET"))
        .unwrap_or_else(|_| "rustshare".to_string());

    let object_store = Arc::new(
        ObjectStore::new_with_options(
            s3_endpoint,
            s3_region,
            s3_bucket,
            ObjectStoreOptions {
                auto_create_bucket: true,
            },
        )
        .await
        .expect("Failed to create object store")
        .with_blob_lock_pool(pool.clone()),
    );

    let jwt_manager = Arc::new(rustshare_auth::JwtManager::new(
        "test_secret_key_at_least_32_chars_long_for_security".to_string(),
        "rustshare",
        "rustshare-api",
        24,
    ));

    let permission_resolver = Arc::new(PermissionResolver::new(Arc::new(
        PermissionResolverRepository::new(pool.clone()),
    )));

    let file_service = Arc::new(FileService::new(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        broadcaster.clone(),
        permission_resolver.clone(),
    ));

    let folder_service = Arc::new(FolderService::new(
        event_store.clone(),
        metadata_store.clone(),
        broadcaster.clone(),
        permission_resolver.clone(),
    ));

    let share_notification_repo = Arc::new(ShareNotificationRepoImpl::new(pool.clone()));

    let share_service = Arc::new(ShareService::new(
        event_store.clone(),
        metadata_store.clone(),
        broadcaster.clone(),
        jwt_manager.clone(),
        share_notification_repo.clone(),
    ));

    let thumbnail_service = Arc::new(ThumbnailService::new(pool.clone(), object_store.clone()));
    let notification_service = Arc::new(NotificationService::new(NotificationRepository::new(
        pool.clone(),
    )));

    let user_repository = Arc::new(UserRepository::new(pool.clone()));
    let file_repository = Arc::new(FileRepository::new(pool.clone()));
    let folder_repository = Arc::new(FolderRepository::new(pool.clone()));
    let share_repository = Arc::new(ShareRepository::new(pool.clone()));

    #[allow(deprecated)]
    let user_share_service = Arc::new(rustshare_core::services::UserShareService::new(
        rustshare_core::services::UserShareServiceDeps {
            share_repo: share_repository.clone(),
            user_repo: user_repository.clone(),
            file_repo: file_repository.clone(),
            folder_repo: folder_repository.clone(),
            permission_resolver: permission_resolver.clone(),
            notification_service: notification_service.clone(),
            event_store: event_store.clone(),
            broadcaster: broadcaster.clone(),
        },
    ));

    let note_service = Arc::new(NoteService::new(
        file_service.clone(),
        folder_service.clone(),
        metadata_store.clone(),
        object_store.clone(),
        permission_resolver.clone(),
        pool.clone(),
    ));

    let decision_service = Arc::new(
        rustshare_server::services::decision_service::DecisionService::new(
            file_service.clone(),
            folder_service.clone(),
            metadata_store.clone(),
            object_store.clone(),
        ),
    );

    let meeting_service = Arc::new(
        rustshare_server::services::meeting_service::MeetingService::new(
            file_service.clone(),
            folder_service.clone(),
            metadata_store.clone(),
            object_store.clone(),
        ),
    );

    let standup_service = Arc::new(
        rustshare_server::services::standup_service::StandupService::new(
            file_service.clone(),
            folder_service.clone(),
            metadata_store.clone(),
            object_store.clone(),
        ),
    );

    let application_service = Arc::new(
        rustshare_server::services::application_service::ApplicationService::new(
            folder_service.clone(),
            metadata_store.clone(),
        ),
    );

    let template_service = Arc::new(
        rustshare_server::services::template_service::TemplateService::new(
            file_service.clone(),
            folder_service.clone(),
            metadata_store.clone(),
        ),
    );

    let kanban_service = Arc::new(
        rustshare_server::services::kanban_service::KanbanService::new(
            file_service.clone(),
            folder_service.clone(),
            metadata_store.clone(),
            object_store.clone(),
            user_repository.clone(),
        ),
    );

    let brainstorming_service = Arc::new(
        rustshare_server::services::brainstorming_service::BrainstormingService::new(
            file_service.clone(),
            folder_service.clone(),
            metadata_store.clone(),
            object_store.clone(),
        ),
    );

    let vault_sync_service = Arc::new(VaultSyncService::new(
        metadata_store.clone(),
        object_store.clone(),
    ));

    let chat_integration_service = Arc::new(ChatIntegrationService::new(
        metadata_store.clone(),
        event_store.clone(),
        broadcaster.clone(),
        "test-secret",
        Arc::new(HttpWebhookDispatcher::new()),
    ));

    let secret_key = SecretEncryptionKey::from_bytes([0u8; 32]);

    let mail_service = Arc::new(rustshare_server::services::mail_service::MailService::new(
        metadata_store.clone(),
        object_store.clone(),
        file_service.clone(),
        folder_service.clone(),
        permission_resolver.clone(),
        event_store.clone(),
        broadcaster.clone(),
        Arc::new(secret_key.clone()),
    ));

    let outbox_store = Arc::new(OutboxStore::new(
        pool.clone(),
        Arc::new(ApplicationRegistry::first_party().unwrap()),
    ));
    let chat_identity_store = ChatIdentityStore::new(pool.clone());
    let chat_observation_store = Arc::new(ChatObservationStore::new(pool.clone()));
    let memory_catalog_store = Arc::new(MemoryCatalogStore::new(pool.clone()));
    let buzz_observation_service = Arc::new(BuzzObservationService::new(
        pool.clone(),
        chat_identity_store.clone(),
        (*chat_observation_store).clone(),
        outbox_store.clone(),
        WebhookSigner::new("test-secret"),
        300,
        Arc::new(EventBroadcaster::new(64)),
    ));

    // ONE Chat owner instance, registered in the source authorizer AND exposed
    // on AppState — the channel gate and the per-message gate must agree.
    let chat_owner = Arc::new(ChatResourceOwner::with_authority(
        chat_identity_store.clone(),
        (*chat_observation_store).clone(),
        Box::new(BuzzGatewayAuthority(gateway.clone())),
    ));
    let mut owners = ResourceOwnerRegistry::new();
    let chat_owner_registered: Arc<dyn ResourceOwner> = chat_owner.clone();
    owners
        .register(
            chat_owner_registered,
            &ApplicationRegistry::first_party().unwrap(),
        )
        .expect("the io.elembra.chat owner registers against the canonical registry");
    let source_authorizer = Arc::new(SourceAuthorizer::new(owners));

    let unified_search_service = Arc::new(UnifiedSearchService::new(
        source_authorizer.clone(),
        metadata_store.clone(),
        None,
        memory_catalog_store.clone(),
    ));

    let ask_workspace_service = Arc::new(AskWorkspaceService::new(
        unified_search_service.clone(),
        None,
    ));

    let state = AppState {
        db_pool: pool,
        metadata_store,
        event_store,
        object_store,
        jwt_manager,
        broadcaster,
        file_service,
        folder_service,
        share_service,
        thumbnail_service,
        permission_resolver,
        source_authorizer,
        notification_service,
        user_share_service,
        ai_service: None,
        upload_service: None,
        rate_limit_config: Arc::new(RateLimitConfig::new()),
        secret_key,
        oidc_runtime_cache: OidcRuntimeCache::new(),
        poll_rate_limiter: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        default_tenant_id: Uuid::nil(),
        note_service,
        decision_service,
        meeting_service,
        standup_service,
        application_service,
        template_service,
        kanban_service,
        brainstorming_service,
        user_repository,
        public_base_url: "http://localhost:8080".to_string(),
        collab_rooms: Arc::new(rustshare_server::handlers::collab::CollabRooms::new()),
        vault_sync_service,
        chat_integration_service,
        mail_service,
        outbox_store,
        chat_observation_store: chat_observation_store.clone(),
        memory_catalog_store,
        unified_search_service,
        ask_workspace_service,
        buzz_observation_service,
        chat_owner,
        buzz_gateway: Some(gateway),
        outbox_status: Arc::new(OutboxStatus::default()),
        outbox_worker_enabled: false,
        outbox_readiness_staleness_secs: 60,
        shutdown_tx: tokio::sync::broadcast::channel(1).0,
        prometheus_handle: rustshare_server::metrics::init_metrics(),
    };

    (state, chat_identity_store, chat_observation_store)
}

// ---------------------------------------------------------------------------
// Proofs
// ---------------------------------------------------------------------------

/// P1. An allowed channel read succeeds: a member with an available message
/// is authorized and the message bytes are fetchable.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires live Buzz relay (scripts/run-buzz-conformance.sh)"]
async fn live_p1_allowed_channel_read_succeeds() {
    let _guard = SERIAL.lock().await;
    let Some(env) = LiveEnv::load() else {
        eprintln!("SKIP live_p1: RUSTSHARE_BUZZ_LIVE_SERVICE_SK / RELAY_PUBKEY not set");
        return;
    };
    let pool = pool().await;
    let fixture = setup_tenant(
        &pool,
        &env,
        &Keys::generate(),
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;
    let channel = seed_private_channel(&env, &fixture).await;
    let obs = observation_service(pool.clone());
    let reference = seed_message(&env, &obs, &fixture, &channel, "live p1 body").await;

    let (state, _, _) = setup_app_state(pool.clone(), live_gateway(&env)).await;
    let ctx = user_ctx(fixture.principal, fixture.tenant);
    assert_eq!(
        state
            .source_authorizer
            .authorize(&ctx, &chat_read_action(), &reference)
            .await,
        Decision::Allow,
        "a live-relay channel member must be allowed"
    );
    let fetched = state
        .source_authorizer
        .fetch(&ctx, &reference, Representation::Raw)
        .await
        .expect("the member fetches the message");
    assert_eq!(
        fetched.data.as_ref(),
        "live p1 body".as_bytes(),
        "fetch returns the relay message bytes"
    );
    cleanup(&pool, fixture.tenant).await;
}

/// P2. A denied (private, non-member) channel read fails closed, and the
/// handler-level surface is existence-hiding.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires live Buzz relay (scripts/run-buzz-conformance.sh)"]
async fn live_p2_denied_private_channel_fails() {
    let _guard = SERIAL.lock().await;
    let Some(env) = LiveEnv::load() else {
        eprintln!("SKIP live_p2: RUSTSHARE_BUZZ_LIVE_SERVICE_SK / RELAY_PUBKEY not set");
        return;
    };
    let pool = pool().await;
    let user_keys = Keys::generate();
    let fixture = setup_tenant(
        &pool,
        &env,
        &user_keys,
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;
    // A private channel the user is NOT a member of (owner-created, no
    // put-user for the user).
    let channel_uuid = Uuid::new_v4().to_string();
    relay_create_channel(
        &env,
        &channel_uuid,
        "conformance-private-2",
        "private",
        "stream",
    )
    .await;
    relay_admit_member(&env, &user_keys.public_key().to_hex()).await;
    // The message exists at the relay AND in Elembra's observation index —
    // only the relay's membership decision denies it.
    // The message is authored by the OWNER (the reader has no publish scope
    // in a channel they cannot access); access checks evaluate the READER.
    // The observation bridge requires a live binding for the EVENT author,
    // so the owner pubkey is bound too.
    bind_pubkey(
        &pool,
        fixture.tenant,
        &fixture.community_id,
        &env.service_keys.public_key().to_hex(),
    )
    .await;
    let obs = observation_service(pool.clone());
    let event = relay_publish_message_as_owner(&env, &channel_uuid, "live p2 body").await;
    ingest_event_into(&obs, &fixture.community_id, &event).await;
    let reference = chat_ref(&event.id.to_hex());

    let (state, _, _) = setup_app_state(pool.clone(), live_gateway(&env)).await;
    let ctx = user_ctx(fixture.principal, fixture.tenant);
    assert_eq!(
        state
            .source_authorizer
            .authorize(&ctx, &chat_read_action(), &reference)
            .await,
        Decision::Deny,
        "a non-member of a private channel must be denied by the live relay"
    );
    assert!(
        matches!(
            state
                .source_authorizer
                .fetch(&ctx, &reference, Representation::Raw)
                .await,
            Err(SourceError::NotFound)
        ),
        "the fetch surface is existence-hiding"
    );
    cleanup(&pool, fixture.tenant).await;
}

/// P3. Cross-community access fails: a mapping pointing at the same relay
/// under a DIFFERENT host binds no community (host-derived isolation) and
/// fails closed.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires live Buzz relay (scripts/run-buzz-conformance.sh)"]
async fn live_p3_cross_community_access_fails() {
    let _guard = SERIAL.lock().await;
    let Some(env) = LiveEnv::load() else {
        eprintln!("SKIP live_p3: RUSTSHARE_BUZZ_LIVE_SERVICE_SK / RELAY_PUBKEY not set");
        return;
    };
    let pool = pool().await;
    let user_keys = Keys::generate();
    let fixture = setup_tenant(
        &pool,
        &env,
        &user_keys,
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;
    let channel = seed_private_channel(&env, &fixture).await;

    // Second tenant whose mapping points at the SAME relay under a different
    // host string ("localhost" vs "127.0.0.1"): the relay binds communities
    // from the request Host, so this host is unmapped and every access check
    // fails closed (the relay answers 404 for an unbound host).
    let foreign_tenant = TenantId::from(Uuid::new_v4());
    cleanup(&pool, foreign_tenant).await;
    let foreign_community = format!("community-{}", Uuid::new_v4());
    let foreign_ws = env.relay_ws.replace("127.0.0.1", "localhost");
    assert_ne!(foreign_ws, env.relay_ws, "the foreign host must differ");
    let foreign_principal = PrincipalId::from(Uuid::new_v4());
    let binding_id = Uuid::new_v4();
    let foreign_mapping_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_workspace_communities
            (mapping_id, tenant_id, workspace_id, community_id, relay_url, relay_pubkey, active)
         VALUES ($1, $2, $3, $4, $5, $6, true)",
    )
    .bind(foreign_mapping_id)
    .bind(foreign_tenant.0)
    .bind(foreign_tenant.0)
    .bind(&foreign_community)
    .bind(&foreign_ws)
    .bind(&env.relay_pubkey)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO chat_identity_bindings
            (binding_id, tenant_id, principal_id, buzz_pubkey, status, verified_at, audit_metadata)
         VALUES ($1, $2, $3, $4, 'active', now(), '{}'::jsonb)",
    )
    .bind(binding_id)
    .bind(foreign_tenant.0)
    .bind(foreign_principal.0)
    .bind(user_keys.public_key().to_hex())
    .execute(&pool)
    .await
    .unwrap();
    // The foreign tenant passes ALL Elembra pre-filters (binding, admission,
    // enablement) so the denial is genuinely the relay's host-derived
    // community isolation, not a local pre-filter.
    sqlx::query(
        "INSERT INTO chat_buzz_admissions
            (admission_id, tenant_id, mapping_id, binding_id, buzz_pubkey, active)
         VALUES ($1, $2, $3, $4, $5, true)",
    )
    .bind(Uuid::new_v4())
    .bind(foreign_tenant.0)
    .bind(foreign_mapping_id)
    .bind(binding_id)
    .bind(user_keys.public_key().to_hex())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO application_enablements
            (tenant_id, workspace_id, application_id, enabled, configuration)
         VALUES ($1, $2, 'io.elembra.chat', true, $3)",
    )
    .bind(foreign_tenant.0)
    .bind(foreign_tenant.0)
    .bind(serde_json::json!({ "memory_projection": true, "content_indexing": true }))
    .execute(&pool)
    .await
    .unwrap();

    // The foreign tenant needs its own observation row (the observation
    // index is tenant-scoped, and a Buzz event id can be observed in exactly
    // one community): publish a DISTINCT message and ingest it under the
    // foreign community only, so every Elembra pre-filter passes and the
    // denial is genuinely the relay's host-derived community isolation.
    let obs = observation_service(pool.clone());
    let reference = seed_message(&env, &obs, &fixture, &channel, "live p3 body").await;
    let foreign_event =
        relay_publish_message(&env, &fixture.user_keys, &channel, "live p3 foreign body").await;
    ingest_event_into(&obs, &foreign_community, &foreign_event).await;
    let foreign_reference = chat_ref(&foreign_event.id.to_hex());

    let (state, _, _) = setup_app_state(pool.clone(), live_gateway(&env)).await;
    let foreign_ctx = user_ctx(foreign_principal, foreign_tenant);
    assert_eq!(
        state
            .source_authorizer
            .authorize(&foreign_ctx, &chat_read_action(), &foreign_reference)
            .await,
        Decision::Deny,
        "a cross-community mapping must fail closed at the live relay"
    );
    // The primary tenant still works (host isolation does not break it).
    let ctx = user_ctx(fixture.principal, fixture.tenant);
    assert_eq!(
        state
            .source_authorizer
            .authorize(&ctx, &chat_read_action(), &reference)
            .await,
        Decision::Allow
    );
    cleanup(&pool, fixture.tenant).await;
    cleanup(&pool, foreign_tenant).await;
}

/// P4. A revoked user is denied immediately: relay-side channel-membership
/// removal flips the very next authorize — no caching.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires live Buzz relay (scripts/run-buzz-conformance.sh)"]
async fn live_p4_revoked_user_denied_immediately() {
    let _guard = SERIAL.lock().await;
    let Some(env) = LiveEnv::load() else {
        eprintln!("SKIP live_p4: RUSTSHARE_BUZZ_LIVE_SERVICE_SK / RELAY_PUBKEY not set");
        return;
    };
    let pool = pool().await;
    let fixture = setup_tenant(
        &pool,
        &env,
        &Keys::generate(),
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;
    let channel = seed_private_channel(&env, &fixture).await;
    let obs = observation_service(pool.clone());
    let reference = seed_message(&env, &obs, &fixture, &channel, "live p4 body").await;

    let (state, _, _) = setup_app_state(pool.clone(), live_gateway(&env)).await;
    let ctx = user_ctx(fixture.principal, fixture.tenant);
    assert_eq!(
        state
            .source_authorizer
            .authorize(&ctx, &chat_read_action(), &reference)
            .await,
        Decision::Allow
    );

    // Relay-side revocation: remove the channel membership (kind-9001) and
    // the community membership (kind-9031).
    let pubkey = fixture.user_keys.public_key().to_hex();
    relay_remove_user(&env, &channel, &pubkey).await;
    relay_revoke_member(&env, &pubkey).await;

    assert_eq!(
        state
            .source_authorizer
            .authorize(&ctx, &chat_read_action(), &reference)
            .await,
        Decision::Deny,
        "the very next authorize after relay revocation must deny — no caching"
    );
    cleanup(&pool, fixture.tenant).await;
}

/// P5. A relay that is unreachable fails closed: authorize returns Deny, not
/// an error and never Allow.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires live Buzz relay (scripts/run-buzz-conformance.sh)"]
async fn live_p5_relay_unavailable_fails_closed() {
    let _guard = SERIAL.lock().await;
    let Some(env) = LiveEnv::load() else {
        eprintln!("SKIP live_p5: RUSTSHARE_BUZZ_LIVE_SERVICE_SK / RELAY_PUBKEY not set");
        return;
    };
    let pool = pool().await;
    let fixture = setup_tenant(
        &pool,
        &env,
        &Keys::generate(),
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;
    let channel = seed_private_channel(&env, &fixture).await;
    let obs = observation_service(pool.clone());
    let reference = seed_message(&env, &obs, &fixture, &channel, "live p5 body").await;

    // A gateway pointed at a dead port on the same host.
    let dead_gateway = Arc::new(
        BuzzGatewayClient::new_for_test(env.service_keys.clone(), Client::builder())
            .expect("build the dead gateway"),
    );
    let (state, _, _) = setup_app_state(pool.clone(), dead_gateway).await;
    // Re-point the mapping at the dead relay.
    sqlx::query("UPDATE chat_workspace_communities SET relay_url = $1 WHERE tenant_id = $2")
        .bind(format!("ws://127.0.0.1:{}", dead_port().await))
        .bind(fixture.tenant.0)
        .execute(&pool)
        .await
        .unwrap();

    let ctx = user_ctx(fixture.principal, fixture.tenant);
    assert_eq!(
        state
            .source_authorizer
            .authorize(&ctx, &chat_read_action(), &reference)
            .await,
        Decision::Deny,
        "an unreachable relay must fail closed to Deny"
    );
    cleanup(&pool, fixture.tenant).await;
}

/// Find a local port nothing listens on.
async fn dead_port() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

/// P6. Batch decisions equal single decisions for the same message set
/// (mixed allow/deny/not_found against the live relay).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires live Buzz relay (scripts/run-buzz-conformance.sh)"]
async fn live_p6_batch_decisions_equal_single_decisions() {
    let _guard = SERIAL.lock().await;
    let Some(env) = LiveEnv::load() else {
        eprintln!("SKIP live_p6: RUSTSHARE_BUZZ_LIVE_SERVICE_SK / RELAY_PUBKEY not set");
        return;
    };
    let pool = pool().await;
    let user_keys = Keys::generate();
    let fixture = setup_tenant(
        &pool,
        &env,
        &user_keys,
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;
    let allowed = seed_private_channel(&env, &fixture).await;
    // A private channel the user is NOT a member of (denied).
    let denied_channel = Uuid::new_v4().to_string();
    relay_create_channel(
        &env,
        &denied_channel,
        "conformance-denied",
        "private",
        "stream",
    )
    .await;
    let obs = observation_service(pool.clone());
    let mut refs = Vec::new();
    for content in ["p6 allow 1", "p6 allow 2"] {
        refs.push(seed_message(&env, &obs, &fixture, &allowed, content).await);
    }
    // The reader cannot publish into the denied channel; the OWNER can (and
    // the owner pubkey is bound so the bridge accepts the author).
    bind_pubkey(
        &pool,
        fixture.tenant,
        &fixture.community_id,
        &env.service_keys.public_key().to_hex(),
    )
    .await;
    for content in ["p6 deny 1", "p6 deny 2"] {
        let event = relay_publish_message_as_owner(&env, &denied_channel, content).await;
        ingest_event_into(&obs, &fixture.community_id, &event).await;
        refs.push(chat_ref(&event.id.to_hex()));
    }
    // A message that exists nowhere (unknown at the relay).
    let unknown_ref = chat_ref(&"a".repeat(64));
    // The unknown ref is filtered by Elembra's pre-filter (no observation
    // row) → NotFound without any relay call; the batch/single parity still
    // holds for the four observed refs.
    let observed_refs: Vec<ResourceRef> = refs.clone();

    let (state, _, _) = setup_app_state(pool.clone(), live_gateway(&env)).await;
    let ctx = user_ctx(fixture.principal, fixture.tenant);
    let batch = state
        .source_authorizer
        .authorize_batch(&ctx, &chat_read_action(), &observed_refs)
        .await
        .expect("batch authorization succeeds");
    assert_eq!(batch.len(), 4);
    for (i, decision) in batch.iter().enumerate() {
        let single = state
            .source_authorizer
            .authorize(&ctx, &chat_read_action(), &observed_refs[i])
            .await;
        assert_eq!(decision.decision, single, "batch/single parity for ref {i}");
    }
    assert!(batch[0].decision.is_allow() && batch[1].decision.is_allow());
    assert_eq!(batch[2].decision, Decision::Deny);
    assert_eq!(batch[3].decision, Decision::Deny);
    // The unknown ref matches too.
    assert_eq!(
        state
            .source_authorizer
            .authorize(&ctx, &chat_read_action(), &unknown_ref)
            .await,
        Decision::NotFound
    );
    cleanup(&pool, fixture.tenant).await;
}

/// P7. Channel listing is authoritative: the registry (not the observation
/// index) drives discovery, and relay revocation is reflected on the next
/// call.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires live Buzz relay (scripts/run-buzz-conformance.sh)"]
async fn live_p7_channel_listing_is_authoritative() {
    let _guard = SERIAL.lock().await;
    let Some(env) = LiveEnv::load() else {
        eprintln!("SKIP live_p7: RUSTSHARE_BUZZ_LIVE_SERVICE_SK / RELAY_PUBKEY not set");
        return;
    };
    let pool = pool().await;
    let fixture = setup_tenant(
        &pool,
        &env,
        &Keys::generate(),
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;
    // A private member channel with ZERO observed events: listed purely from
    // the registry.
    let member_channel = seed_private_channel(&env, &fixture).await;
    // An open channel: listed for the member without membership.
    let open_channel = seed_open_channel(&env, &fixture).await;
    // A private non-member channel: never listed.
    let hidden_channel = Uuid::new_v4().to_string();
    relay_create_channel(
        &env,
        &hidden_channel,
        "conformance-hidden",
        "private",
        "stream",
    )
    .await;

    let (state, _, _) = setup_app_state(pool.clone(), live_gateway(&env)).await;
    let response = list_channels(
        State(state.clone()),
        auth(fixture.principal, fixture.tenant),
    )
    .await
    .expect("the list succeeds");
    let channels: HashMap<String, ChannelInfo> = response
        .0
        .into_iter()
        .map(|channel| (channel.channel_id.clone(), channel))
        .collect();
    assert!(
        channels.contains_key(&member_channel),
        "a registry-visible channel with zero observations is listed"
    );
    assert!(
        channels.contains_key(&open_channel),
        "an open channel is listed"
    );
    assert!(
        !channels.contains_key(&hidden_channel),
        "a private non-member channel is never listed"
    );

    // Relay-side revocation is reflected on the very next call.
    let pubkey = fixture.user_keys.public_key().to_hex();
    relay_remove_user(&env, &member_channel, &pubkey).await;
    let response = list_channels(
        State(state.clone()),
        auth(fixture.principal, fixture.tenant),
    )
    .await
    .expect("the re-list succeeds");
    let channels: Vec<String> = response.0.into_iter().map(|c| c.channel_id).collect();
    assert!(
        !channels.contains(&member_channel),
        "a relay-revoked channel disappears on the next list call"
    );
    cleanup(&pool, fixture.tenant).await;
}

/// P9. Memory/Search/Ask cannot bypass Buzz: the message is indexed and
/// searchable, but after relay-side revocation the RAG materialization (the
/// Ask retrieval path, which reauthorizes through the live relay) returns
/// nothing for it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires live Buzz relay (scripts/run-buzz-conformance.sh)"]
async fn live_p9_memory_search_ask_cannot_bypass_buzz() {
    let _guard = SERIAL.lock().await;
    let Some(env) = LiveEnv::load() else {
        eprintln!("SKIP live_p9: RUSTSHARE_BUZZ_LIVE_SERVICE_SK / RELAY_PUBKEY not set");
        return;
    };
    let pool = pool().await;
    let fixture = setup_tenant(
        &pool,
        &env,
        &Keys::generate(),
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;
    let channel = seed_private_channel(&env, &fixture).await;
    let obs = observation_service(pool.clone());
    let reference = seed_message(
        &env,
        &obs,
        &fixture,
        &channel,
        "conformance searchable payload",
    )
    .await;
    let message_id = reference.resource_id.clone();

    // Fold the observation into the Memory catalog through the REAL pipeline
    // (outbox → dispatcher → projection consumer).
    let outbox = Arc::new(OutboxStore::new(
        pool.clone(),
        Arc::new(ApplicationRegistry::first_party().unwrap()),
    ));
    outbox
        .register_consumer(TEST_CONSUMER_ID, &[CHAT_BUZZ_EVENT_OBSERVED_V1.to_string()])
        .await
        .unwrap();
    let consumer = Arc::new(MemoryChatProjectionConsumer::new_for_test(
        pool.clone(),
        ChatIdentityStore::new(pool.clone()),
        ChatObservationStore::new(pool.clone()),
        MemoryCatalogStore::with_observation_store(
            pool.clone(),
            ChatObservationStore::new(pool.clone()),
        ),
        TEST_CONSUMER_ID,
    )) as Arc<dyn OutboxConsumer>;
    let dispatcher = Arc::new(OutboxDispatcher::new(
        outbox,
        vec![consumer],
        OutboxWorkerConfig::default(),
        "buzz-conformance-worker".to_string(),
    ));
    dispatcher.tick().await;

    let (state, _, _) = setup_app_state(pool.clone(), live_gateway(&env)).await;
    let ctx = user_ctx(fixture.principal, fixture.tenant);
    let search = state
        .unified_search_service
        .search(&ctx, "conformance searchable", &[SearchSource::Chat], 10)
        .await
        .expect("search succeeds while the user is a member");
    let message_uri = chat_ref(&message_id).to_uri();
    assert!(
        search
            .results
            .iter()
            .any(|result| result.resource_ref == message_uri),
        "the message is indexed and searchable"
    );
    let rag = state
        .unified_search_service
        .materialize_for_rag(&ctx, &search.results, 10, 100_000, 100_000)
        .await;
    assert!(
        rag.iter()
            .any(|source| source.resource.resource_id == message_id),
        "the message materializes for RAG while the user is a member"
    );

    // Revoke at the relay: the index still holds the record, but the
    // reauthorization through the live relay must exclude it.
    let pubkey = fixture.user_keys.public_key().to_hex();
    relay_remove_user(&env, &channel, &pubkey).await;
    relay_revoke_member(&env, &pubkey).await;
    let rag = state
        .unified_search_service
        .materialize_for_rag(&ctx, &search.results, 10, 100_000, 100_000)
        .await;
    assert!(
        !rag.iter()
            .any(|source| source.resource.resource_id == message_id),
        "Ask/RAG materialization must not bypass the live relay revocation"
    );
    cleanup(&pool, fixture.tenant).await;
}

/// P10 (correctness half; the latency budget is Task E7's): a 64-message
/// page authorizes in exactly ONE relay batch round-trip, counted via the
/// relay's own metrics endpoint.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires live Buzz relay (scripts/run-buzz-conformance.sh)"]
async fn live_p10_64_message_page_is_one_batch_round_trip() {
    let _guard = SERIAL.lock().await;
    let Some(env) = LiveEnv::load() else {
        eprintln!("SKIP live_p10: RUSTSHARE_BUZZ_LIVE_SERVICE_SK / RELAY_PUBKEY not set");
        return;
    };
    let pool = pool().await;
    let fixture = setup_tenant(
        &pool,
        &env,
        &Keys::generate(),
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;
    let channel = seed_private_channel(&env, &fixture).await;
    let obs = observation_service(pool.clone());
    let mut refs = Vec::with_capacity(64);
    for i in 0..64 {
        refs.push(
            seed_message(
                &env,
                &obs,
                &fixture,
                &channel,
                &format!("live p10 message {i}"),
            )
            .await,
        );
    }

    let before = relay_route_count(&env, "check-batch").await;
    let (state, _, _) = setup_app_state(pool.clone(), live_gateway(&env)).await;
    let ctx = user_ctx(fixture.principal, fixture.tenant);
    let decisions = state
        .source_authorizer
        .authorize_batch(&ctx, &chat_read_action(), &refs)
        .await
        .expect("a 64-ref batch must be authorized");
    assert_eq!(decisions.len(), 64);
    assert!(
        decisions.iter().all(|d| d.decision.is_allow()),
        "every member message on the page is allowed"
    );
    let after = relay_route_count(&env, "check-batch").await;
    assert_eq!(
        after - before,
        1,
        "a 64-message page must cost exactly ONE relay batch round-trip"
    );
    cleanup(&pool, fixture.tenant).await;
}
