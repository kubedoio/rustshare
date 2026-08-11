//! DB-backed integration suite proving the Buzz source-authorization &
//! reconciliation gateway end-to-end against an in-test FAKE Buzz relay.
//!
//! The fake relay ([`start_fake_buzz`]) is a real HTTP server
//! (`axum::serve` on `127.0.0.1:0`) with **no database** — it implements the
//! v1alpha1 upstream contract
//! (`docs/specs/buzz-upstream-authorization-v1alpha1.md`): NIP-98 service
//! authentication, `POST /api/v1/relay/access/check` and
//! `GET /api/v1/relay/state/events`, and kind-19030 responses signed by the
//! relay key (echo + freshness enforced by the real client under test). Every
//! production request path runs over this public HTTP contract: the real
//! [`BuzzGatewayClient`], the real
//! [`ChatResourceOwner`](rustshare_server::authz::ChatResourceOwner) gate, and
//! the real
//! [`reconcile_chat_memory_from_buzz_for_tenant`](rustshare_server::handlers::memory_reconcile::reconcile_chat_memory_from_buzz_for_tenant)
//! repair path.
//!
//! The suite proves the 14 requirements plus 4 negative cases:
//!
//! 1. a channel member can materialize a workspace message (body bytes);
//! 2. a non-member cannot materialize (existence-hiding NotFound);
//! 3. a non-member of a private channel is denied;
//! 4. membership removal at the relay takes effect immediately;
//! 5. a DM participant is allowed (metadata) with no body; an outsider is
//!    denied;
//! 6. a local community admission can never bypass the relay's channel
//!    decision;
//! 7. stale Memory/observation state can never grant access;
//! 8. cross-tenant refs fail closed, and a wrong `relay_pubkey` pin fails
//!    closed (InvalidResponse → Deny);
//! 9. an unreachable relay fails closed;
//! 10. only the service key is ever used (no human user key server-side);
//! 11. reconcile-from-Buzz repairs a missing Memory projection without
//!     touching the outbox;
//! 12. reconcile-from-Buzz repairs a missing observation index from the
//!     relay's state over HTTP;
//! 13. reconcile-from-Buzz is idempotent;
//! 14. the repair flows over the public HTTP contract (no relay DB exists);
//!     N1. deleted messages are not_found (tombstone semantics end-to-end);
//!     N2. binding rotation asks the relay for the NEW pubkey;
//!     N3. unknown channels are not_found;
//!     N4. the tenant-scope guard skips foreign-community entries during a
//!     shared-relay reconcile.
//!
//! DB-backed and `#[ignore]`d; run against the dev database (migrations
//! applied, including `20260810000006`) with `--test-threads=1`:
//!
//!   set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true \
//!     cargo test -p rustshare-server --test buzz_authority_gateway_test -- \
//!       --ignored --test-threads=1
//!
//! The chat tables are tenant-scoped and the outbox/delivery/receipt/consumer
//! tables are process-global, so every test takes a shared `SERIAL` guard and
//! cleans up exactly the rows it created (same convention as the chat-owner,
//! buzz-memory-projection and memory-reconcile suites). Every test uses a
//! fresh tenant and community id (the active-community mapping index is
//! global).

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, LazyLock, Mutex};

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode, Uri};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::{DateTime, Utc};
use nostr::nips::nip98::{verify_auth_header, HttpMethod};
use nostr::{Event as NostrEvent, EventBuilder, JsonUtil, Keys, Kind, Tag, Timestamp};
use reqwest::Client;
use rustshare_core::domain::{
    ActionCapability, ApplicationId, ApplicationRegistry, PrincipalId, TenantId, WorkspaceId,
};
use rustshare_crypto::WebhookSigner;
use rustshare_integration_events::event_types::CHAT_BUZZ_EVENT_OBSERVED_V1;
use rustshare_integration_events::OutboxConsumer;
use rustshare_memory::event::{ChatChannelKind, ObservedEventType};
use rustshare_resource_auth::{
    BuzzChannelKind, BuzzReadDecision, Candidate, Decision, PrincipalContext, Purpose,
    Representation, ResourceOwnerRegistry, ResourceRef, SourceAuthorizer, SourceError, CHAT_READ,
};
use rustshare_server::authz::ChatResourceOwner;
use rustshare_server::buzz_gateway::{
    BuzzAccessCheckRequest, BuzzGatewayAuthority, BuzzGatewayClient, BuzzStateContext,
    BuzzStateEntry,
};
use rustshare_server::buzz_observation::{
    BuzzEventPush, BuzzObservationService, BuzzPushContext, BuzzPushError, IngestOutcome,
};
use rustshare_server::config::OutboxWorkerConfig;
use rustshare_server::handlers::memory_reconcile::reconcile_chat_memory_from_buzz_for_tenant;
use rustshare_server::memory_projection::{
    MemoryChatProjectionConsumer, MEMORY_CHAT_PROJECTION_CONSUMER_ID,
};
use rustshare_server::outbox_dispatcher::OutboxDispatcher;
use rustshare_storage::{
    ChatIdentityStore, ChatObservationStore, MemoryCatalogStore, OutboxStore, ReconcileCounts,
};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use url::Url;
use uuid::Uuid;

/// Serializes the tests within this binary (same convention as the chat-owner
/// and buzz-memory-projection suites).
static SERIAL: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

const WEBHOOK_SECRET: &str = "test-buzz-webhook-secret";
const CHANNEL_ID: &str = "channel-1";

// ---------------------------------------------------------------------------
// Fake Buzz relay (in-memory only — no database)
// ---------------------------------------------------------------------------

/// In-memory state of the fake relay. There is deliberately NO database: the
/// fake implements the v1alpha1 upstream contract against this state, and the
/// only way production code can read it is over the public HTTP endpoints.
struct FakeBuzzState {
    /// The relay's signing key; every kind-19030 response is signed with it
    /// (pinned via the mapping's `relay_pubkey`).
    relay_keys: Keys,
    /// The ONLY accepted NIP-98 signer (64-hex): the workload's service
    /// public key. Any other verified signer is rejected with 401.
    service_pubkey: String,
    /// `channel_id → member pubkeys` — the relay's current membership.
    channels: HashMap<String, HashSet<String>>,
    /// `channel_id → channel_kind` (informational; the FAKE's decision uses
    /// the member set for every kind).
    channel_kinds: HashMap<String, String>,
    /// `message_id → author pubkey` — the relay's current message index.
    messages: HashMap<String, String>,
    /// Message ids currently deleted (tombstoned) at the relay.
    deleted: HashSet<String>,
    /// The relay's signed event state, paged by `state/events`.
    events: Vec<BuzzStateEntry>,
    /// Every access-check request received, recorded for assertions:
    /// `{ "auth_pubkey": <verified NIP-98 signer>, "request": <body> }`.
    access_check_requests: Vec<serde_json::Value>,
    /// How many state-paging requests the relay served.
    state_requests: u64,
}

impl FakeBuzzState {
    /// Register a signed state entry; the message is also indexed for
    /// access-check availability (author = the event's pubkey).
    fn register_event(&mut self, entry: BuzzStateEntry) {
        if let Some(author) = entry
            .event
            .get("pubkey")
            .and_then(serde_json::Value::as_str)
        {
            self.messages
                .insert(entry.context.message_id.clone(), author.to_string());
        }
        self.events.push(entry);
    }

    fn add_member(&mut self, channel_id: &str, pubkey: &str) {
        self.channels
            .entry(channel_id.to_string())
            .or_default()
            .insert(pubkey.to_string());
    }

    fn remove_member(&mut self, channel_id: &str, pubkey: &str) {
        if let Some(members) = self.channels.get_mut(channel_id) {
            members.remove(pubkey);
        }
    }

    fn set_channel_kind(&mut self, channel_id: &str, kind: &str) {
        self.channel_kinds
            .insert(channel_id.to_string(), kind.to_string());
    }

    fn mark_deleted(&mut self, message_id: &str) {
        self.deleted.insert(message_id.to_string());
    }
}

/// Cloneable server handle for the axum handlers.
#[derive(Clone)]
struct FakeServerHandle {
    state: Arc<Mutex<FakeBuzzState>>,
    /// `host:port` of the bound listener (Host-header fallback).
    host: String,
}

/// A running fake relay: address, shared state, the serve task, and a
/// shutdown trigger (dropping the struct closes the trigger and stops the
/// server gracefully).
struct FakeBuzzServer {
    addr: SocketAddr,
    state: Arc<Mutex<FakeBuzzState>>,
    task: tokio::task::JoinHandle<()>,
    shutdown: tokio::sync::oneshot::Sender<()>,
}

impl FakeBuzzServer {
    async fn stop(self) {
        let _ = self.shutdown.send(());
        let _ = self.task.await;
    }
}

/// Bind and serve the fake relay on an ephemeral loopback port.
fn start_fake_buzz(relay_keys: Keys, service_pubkey: String) -> FakeBuzzServer {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind the fake relay");
    listener
        .set_nonblocking(true)
        .expect("nonblocking listener");
    let listener = tokio::net::TcpListener::from_std(listener).expect("tokio listener");
    let addr = listener.local_addr().expect("fake relay address");
    let state = Arc::new(Mutex::new(FakeBuzzState {
        relay_keys,
        service_pubkey,
        channels: HashMap::new(),
        channel_kinds: HashMap::new(),
        messages: HashMap::new(),
        deleted: HashSet::new(),
        events: Vec::new(),
        access_check_requests: Vec::new(),
        state_requests: 0,
    }));
    let app = Router::new()
        .route("/api/v1/relay/access/check", post(access_check))
        .route("/api/v1/relay/state/events", get(state_events))
        .with_state(FakeServerHandle {
            state: state.clone(),
            host: addr.to_string(),
        });
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let server = axum::serve(listener, app).with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
        });
        if let Err(error) = server.await {
            tracing::error!(%error, "fake buzz relay server failed");
        }
    });
    FakeBuzzServer {
        addr,
        state,
        task,
        shutdown: shutdown_tx,
    }
}

/// Rebuild the exact URL the client signed, from the Host header + the
/// endpoint path. The client signs `Url::as_str()` of the URL it built via
/// `http_base(relay_url).join(path)`; rebuilding with the same `url` crate
/// over the same host:port yields an identical serialization (NIP-98 compares
/// serialized URLs).
fn request_url(host: &str, headers: &HeaderMap, path: &str) -> Url {
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or(host);
    Url::parse(&format!("http://{host}{path}")).expect("fake relay URL parses")
}

/// Rebuild the exact state-events URL the client signed, appending the query
/// pairs in the same order the client's `query_pairs_mut` uses (since, limit,
/// cursor) so the serialization matches byte-for-byte.
fn state_url(
    host: &str,
    headers: &HeaderMap,
    since: Option<i64>,
    limit: u32,
    cursor: Option<&str>,
) -> Url {
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or(host);
    let mut url = Url::parse(&format!("http://{host}/api/v1/relay/state/events"))
        .expect("fake relay URL parses");
    {
        let mut query = url.query_pairs_mut();
        if let Some(since) = since {
            query.append_pair("since", &since.to_string());
        }
        query.append_pair("limit", &limit.to_string());
        if let Some(cursor) = cursor {
            query.append_pair("cursor", cursor);
        }
    }
    url
}

/// Sign a kind-19030 response event from the relay key.
fn relay_19030(relay_keys: &Keys, content: serde_json::Value) -> NostrEvent {
    EventBuilder::new(Kind::from(19_030), content.to_string())
        .sign_with_keys(relay_keys)
        .expect("sign the relay response")
}

/// `POST /api/v1/relay/access/check`: verify NIP-98 (service key only), then
/// answer from current state (message availability → channel visibility →
/// membership). The response is a kind-19030 event signed by the relay key
/// echoing the request verbatim, with a fresh `evaluated_at`.
async fn access_check(
    State(handle): State<FakeServerHandle>,
    headers: HeaderMap,
    body: Body,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let body_bytes = axum::body::to_bytes(body, 1024 * 1024)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("cannot read body: {e}")))?;
    let url = request_url(&handle.host, &headers, "/api/v1/relay/access/check");
    let auth = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                "missing NIP-98 Authorization header".to_string(),
            )
        })?;
    let signer = verify_auth_header(
        auth,
        &url,
        HttpMethod::POST,
        Timestamp::now(),
        Some(&body_bytes),
    )
    .map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            "invalid NIP-98 authorization".to_string(),
        )
    })?;
    let request: BuzzAccessCheckRequest = serde_json::from_slice(&body_bytes).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("invalid check request: {e}"),
        )
    })?;
    let (decision, reason, evaluated_at) = {
        let mut state = handle.state.lock().unwrap();
        if signer.to_hex() != state.service_pubkey {
            return Err((
                StatusCode::UNAUTHORIZED,
                "untrusted NIP-98 signer".to_string(),
            ));
        }
        state
            .access_check_requests
            .push(serde_json::json!({ "auth_pubkey": signer.to_hex(), "request": &request }));
        let decision = if request.message_id.as_ref().is_some_and(|message_id| {
            !state.messages.contains_key(message_id) || state.deleted.contains(message_id)
        }) {
            ("not_found", "message unavailable")
        } else if !state.channels.contains_key(&request.channel_id) {
            ("not_found", "unknown channel")
        } else if !state
            .channels
            .get(&request.channel_id)
            .is_some_and(|members| members.contains(&request.pubkey))
        {
            ("deny", "not a member")
        } else {
            ("allow", "member")
        };
        (
            decision.0.to_string(),
            decision.1.to_string(),
            Utc::now().timestamp(),
        )
    };
    let content = serde_json::json!({
        "decision": decision,
        "reason": reason,
        "evaluated_at": evaluated_at,
        "pubkey": request.pubkey,
        "channel_id": request.channel_id,
        "message_id": request.message_id,
    });
    let event = {
        let state = handle.state.lock().unwrap();
        relay_19030(&state.relay_keys, content)
    };
    let raw: serde_json::Value = serde_json::from_str(&event.as_json())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(raw))
}

/// The event's `created_at` (unix seconds) from its raw JSON.
fn event_created_at(entry: &BuzzStateEntry) -> i64 {
    entry
        .event
        .get("created_at")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0)
}

/// Parse a query string into a map (values are plain digits/cursors; no
/// percent-decoding needed for the fake's own values).
fn parse_query(query: Option<&str>) -> HashMap<String, String> {
    query
        .unwrap_or("")
        .split('&')
        .filter(|pair| !pair.is_empty())
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

/// `GET /api/v1/relay/state/events`: verify NIP-98 (GET, no payload), then
/// page the in-memory event state (`since` filter, `limit`, integer-offset
/// cursor; `complete` on the last page). The response is a kind-19030 event
/// signed by the relay key whose content is `{ entries, cursor, complete }`.
async fn state_events(
    State(handle): State<FakeServerHandle>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let query = parse_query(uri.query());
    let since: Option<i64> = query.get("since").and_then(|value| value.parse().ok());
    let limit: u32 = query
        .get("limit")
        .and_then(|value| value.parse().ok())
        .unwrap_or(200)
        .clamp(1, 500);
    let cursor: Option<String> = query.get("cursor").cloned();
    let url = state_url(&handle.host, &headers, since, limit, cursor.as_deref());
    let auth = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                "missing NIP-98 Authorization header".to_string(),
            )
        })?;
    let signer =
        verify_auth_header(auth, &url, HttpMethod::GET, Timestamp::now(), None).map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                "invalid NIP-98 authorization".to_string(),
            )
        })?;
    let (entries, next_cursor, complete) = {
        let mut state = handle.state.lock().unwrap();
        if signer.to_hex() != state.service_pubkey {
            return Err((
                StatusCode::UNAUTHORIZED,
                "untrusted NIP-98 signer".to_string(),
            ));
        }
        state.state_requests += 1;
        let filtered: Vec<BuzzStateEntry> = state
            .events
            .iter()
            .filter(|entry| since.is_none_or(|since| event_created_at(entry) >= since))
            .cloned()
            .collect();
        let offset: usize = cursor
            .as_deref()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let page: Vec<BuzzStateEntry> = filtered
            .iter()
            .skip(offset)
            .take(limit as usize)
            .cloned()
            .collect();
        let complete = offset + page.len() >= filtered.len();
        let next = if complete {
            None
        } else {
            Some((offset + page.len()).to_string())
        };
        (page, next, complete)
    };
    let content = serde_json::json!({
        "entries": entries,
        "cursor": next_cursor,
        "complete": complete,
    });
    let event = {
        let state = handle.state.lock().unwrap();
        relay_19030(&state.relay_keys, content)
    };
    let raw: serde_json::Value = serde_json::from_str(&event.as_json())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(raw))
}

// ---------------------------------------------------------------------------
// Shared harness (DB pool, stores, tenant setup, fixtures)
// ---------------------------------------------------------------------------

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

/// The shared chat stores over `pool`, with the catalog wired to the
/// observation index (consumer tombstone-before-create guard active).
fn stores(pool: PgPool) -> (ChatIdentityStore, ChatObservationStore, MemoryCatalogStore) {
    let chat_identity = ChatIdentityStore::new(pool.clone());
    let observations = ChatObservationStore::new(pool.clone());
    let catalog = MemoryCatalogStore::with_observation_store(pool, observations.clone());
    (chat_identity, observations, catalog)
}

/// The bridge service under test (HMAC signer is unused by the reconcile
/// path; `ingest_without_outbox` never touches the outbox).
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
    MemoryChatProjectionConsumer::new(pool, chat_identity, observations, catalog)
}

/// Register the memory consumer durably BEFORE pushing, so publish-time eager
/// fan-out creates its pending delivery obligations (same convention as the
/// buzz-memory-projection suite).
async fn register_consumer(store: &OutboxStore) {
    store
        .register_consumer(
            MEMORY_CHAT_PROJECTION_CONSUMER_ID,
            &[CHAT_BUZZ_EVENT_OBSERVED_V1.to_string()],
        )
        .await
        .unwrap();
}

/// Run one full dispatcher pass (claim → process → ack), exactly how the
/// buzz-memory-projection suite drives the REAL dispatcher.
async fn dispatch_once(pool: &PgPool, store: Arc<OutboxStore>) {
    let consumer = Arc::new(consumer(pool.clone())) as Arc<dyn OutboxConsumer>;
    let dispatcher = Arc::new(OutboxDispatcher::new(
        store,
        vec![consumer],
        OutboxWorkerConfig::default(),
        "gateway-e2e-test-worker".to_string(),
    ));
    dispatcher.tick().await;
}

/// The ids a tenant setup creates, for the assertions.
struct BuzzEnv {
    principal: PrincipalId,
    community_id: String,
}

/// Insert an active mapping for `community_id` under `tenant` (workspace ==
/// tenant) pointing at the fake relay, pinned to `relay_pubkey`. Returns the
/// mapping id.
async fn insert_mapping(
    pool: &PgPool,
    tenant: TenantId,
    community_id: &str,
    relay_url: &str,
    relay_pubkey: Option<&str>,
) -> Uuid {
    let mapping_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_workspace_communities
            (mapping_id, tenant_id, workspace_id, community_id, relay_url, relay_pubkey, active)
         VALUES ($1, $2, $3, $4, $5, $6, true)",
    )
    .bind(mapping_id)
    .bind(tenant.0)
    .bind(tenant.0)
    .bind(community_id)
    .bind(relay_url)
    .bind(relay_pubkey)
    .execute(pool)
    .await
    .unwrap();
    mapping_id
}

/// Insert an active binding for `pubkey` under a fresh principal and return
/// its principal id and binding id.
async fn insert_binding(pool: &PgPool, tenant: TenantId, pubkey: &str) -> (PrincipalId, Uuid) {
    let principal_id = PrincipalId::from(Uuid::new_v4());
    let binding_id = insert_binding_for_principal(pool, tenant, principal_id, pubkey).await;
    (principal_id, binding_id)
}

/// Insert an active binding for `pubkey` under an EXPLICIT principal id (the
/// rotation path binds the new key to the SAME principal).
async fn insert_binding_for_principal(
    pool: &PgPool,
    tenant: TenantId,
    principal_id: PrincipalId,
    pubkey: &str,
) -> Uuid {
    let binding_id = Uuid::new_v4();
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
    binding_id
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

/// Full happy-path tenant setup against the fake relay: active mapping (relay
/// URL + pinned `relay_pubkey`), active binding + admission for `keys`, and
/// the chat Application enabled with `configuration`.
async fn setup_tenant_with_relay(
    pool: &PgPool,
    tenant: TenantId,
    keys: &Keys,
    community_id: &str,
    relay_url: &str,
    relay_pubkey: &str,
    configuration: serde_json::Value,
) -> BuzzEnv {
    let mapping_id =
        insert_mapping(pool, tenant, community_id, relay_url, Some(relay_pubkey)).await;
    let pubkey = keys.public_key().to_hex();
    let (principal, binding_id) = insert_binding(pool, tenant, &pubkey).await;
    insert_admission(pool, tenant, mapping_id, binding_id, &pubkey).await;
    enable_chat(pool, tenant, configuration).await;
    BuzzEnv {
        principal,
        community_id: community_id.to_string(),
    }
}

/// Revoke every active binding for the tenant (as `revoke_principal` would).
async fn revoke_bindings(pool: &PgPool, tenant: TenantId) {
    sqlx::query(
        "UPDATE chat_identity_bindings
         SET status = 'revoked', revoked_at = now()
         WHERE tenant_id = $1 AND status <> 'revoked'",
    )
    .bind(tenant.0)
    .execute(pool)
    .await
    .unwrap();
}

/// A real signed kind-1 (text note) event.
fn signed_note(keys: &Keys, content: &str) -> NostrEvent {
    EventBuilder::text_note(content)
        .sign_with_keys(keys)
        .expect("sign text note")
}

/// A created-event push: message id == event id, `created_at = now`.
fn created_push(
    keys: &Keys,
    content: &str,
    community_id: &str,
    channel_kind: ChatChannelKind,
) -> (BuzzEventPush, NostrEvent) {
    let event = signed_note(keys, content);
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

/// Map the Memory crate's channel kind onto the wire-identical authority
/// enum.
fn buzz_channel_kind(kind: ChatChannelKind) -> BuzzChannelKind {
    match kind {
        ChatChannelKind::Workspace => BuzzChannelKind::Workspace,
        ChatChannelKind::Dm => BuzzChannelKind::Dm,
        ChatChannelKind::Private => BuzzChannelKind::Private,
        ChatChannelKind::Excluded => BuzzChannelKind::Excluded,
    }
}

/// Build the relay's signed state entry for a signed event (the shape of the
/// webhook `BuzzPushContext`, so reconcile reuses its validation unchanged).
fn state_entry(
    event: &NostrEvent,
    community_id: &str,
    channel_id: &str,
    channel_kind: ChatChannelKind,
    event_type: ObservedEventType,
) -> BuzzStateEntry {
    BuzzStateEntry {
        event: serde_json::to_value(event).unwrap(),
        context: BuzzStateContext {
            community_id: community_id.to_string(),
            channel_id: channel_id.to_string(),
            channel_kind: buzz_channel_kind(channel_kind),
            thread_root_id: None,
            message_id: event.id.to_hex(),
            event_type: match event_type {
                ObservedEventType::Created => "created",
                ObservedEventType::Edited => "edited",
                ObservedEventType::Deleted => "deleted",
            }
            .to_string(),
            supersedes_event_id: None,
        },
    }
}

/// A fresh `SourceAuthorizer` seeded with the Chat owner adapter whose FINAL
/// channel/message authority is the gateway client (the production wiring).
async fn authorizer_with_gateway(
    pool: PgPool,
    gateway: Arc<BuzzGatewayClient>,
) -> SourceAuthorizer {
    let registry = ApplicationRegistry::first_party().expect("first-party manifests are valid");
    let mut owners = ResourceOwnerRegistry::new();
    owners
        .register(
            Arc::new(ChatResourceOwner::with_authority(
                ChatIdentityStore::new(pool.clone()),
                ChatObservationStore::new(pool.clone()),
                Box::new(BuzzGatewayAuthority(gateway)),
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

/// Run the reconcile-from-Buzz repair over the shared stores (the admin repair
/// path: observation index repaired from the relay's signed state over HTTP,
/// then the Memory catalog folded; no outbox writes, no receipts).
async fn reconcile_from_buzz(
    pool: &PgPool,
    tenant_id: TenantId,
    gateway: &BuzzGatewayClient,
    since: Option<DateTime<Utc>>,
) -> ReconcileCounts {
    let service = service(pool.clone());
    let (chat_identity, observations, catalog) = stores(pool.clone());
    reconcile_chat_memory_from_buzz_for_tenant(
        &service,
        &chat_identity,
        &observations,
        &catalog,
        gateway,
        tenant_id,
        since,
    )
    .await
    .expect("reconcile from buzz must succeed")
}

async fn observation_count(pool: &PgPool, tenant_id: TenantId) -> i64 {
    sqlx::query_scalar("SELECT count(*)::bigint FROM chat_observed_events WHERE tenant_id = $1")
        .bind(tenant_id.0)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn catalog_count(pool: &PgPool, tenant_id: TenantId) -> i64 {
    sqlx::query_scalar("SELECT count(*)::bigint FROM memory_catalog WHERE tenant_id = $1")
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
    sqlx::query("DELETE FROM integration_consumers WHERE consumer_id = $1")
        .bind(MEMORY_CHAT_PROJECTION_CONSUMER_ID)
        .execute(pool)
        .await
        .unwrap();
}

// ---------------------------------------------------------------------------
// 1. A channel member can materialize a workspace message (body bytes)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn authorized_member_can_materialize_message() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    // Defensive pre-cleanup: every test must be deterministic even if a
    // previous (possibly interrupted) run left rows behind.
    cleanup(&pool, tenant).await;

    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    let relay_keys = Keys::generate();
    let service_keys = Keys::generate();
    let fake = start_fake_buzz(relay_keys.clone(), service_keys.public_key().to_hex());
    let relay_url = format!("ws://127.0.0.1:{}", fake.addr.port());
    let relay_pubkey = relay_keys.public_key().to_hex();
    let env = setup_tenant_with_relay(
        &pool,
        tenant,
        &keys,
        &community,
        &relay_url,
        &relay_pubkey,
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;
    fake.state
        .lock()
        .unwrap()
        .add_member(CHANNEL_ID, &keys.public_key().to_hex());

    let store = outbox_store(pool.clone());
    register_consumer(&store).await;
    let service = service(pool.clone());
    let body = "hello authorized member";
    let (buzz_push, event) = created_push(&keys, body, &community, ChatChannelKind::Workspace);
    assert_eq!(
        ingest_push(&service, &buzz_push).await.unwrap(),
        IngestOutcome::FirstObservation
    );
    let message_id = event.id.to_hex();
    fake.state.lock().unwrap().register_event(state_entry(
        &event,
        &community,
        CHANNEL_ID,
        ChatChannelKind::Workspace,
        ObservedEventType::Created,
    ));
    dispatch_once(&pool, store).await;

    let gateway =
        Arc::new(BuzzGatewayClient::new(service_keys.clone(), Client::builder()).unwrap());
    let authorizer = authorizer_with_gateway(pool.clone(), gateway).await;
    let ctx = user_ctx(env.principal, tenant);
    let reference = chat_ref(&message_id);

    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &reference)
            .await,
        Decision::Allow,
        "a channel member is allowed by the relay's current state"
    );
    let resolved = authorizer
        .resolve(&ctx, &reference, Purpose::RagContext)
        .await
        .expect("member resolves the message");
    assert!(
        resolved.available,
        "content_indexing on ⇒ a body copy exists ⇒ available"
    );
    let fetched = authorizer
        .fetch(&ctx, &reference, Representation::Raw)
        .await
        .expect("member fetches the body");
    assert_eq!(fetched.data.as_ref(), body.as_bytes());

    let materialized = authorizer
        .materialize(
            &ctx,
            &chat_read_action(),
            vec![Candidate {
                resource: reference,
                cached_text: None,
            }],
        )
        .await
        .expect("materialization succeeds");
    assert_eq!(materialized.len(), 1);
    assert_eq!(
        materialized[0].data.as_ref(),
        body.as_bytes(),
        "materialized data is the real authorized source content"
    );

    fake.stop().await;
    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 2. A non-member cannot materialize (Deny; fetch is existence-hiding)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn unauthorized_member_cannot_materialize() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    cleanup(&pool, tenant).await;

    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    let relay_keys = Keys::generate();
    let service_keys = Keys::generate();
    let fake = start_fake_buzz(relay_keys.clone(), service_keys.public_key().to_hex());
    let relay_url = format!("ws://127.0.0.1:{}", fake.addr.port());
    let env = setup_tenant_with_relay(
        &pool,
        tenant,
        &keys,
        &community,
        &relay_url,
        &relay_keys.public_key().to_hex(),
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;

    let service = service(pool.clone());
    let (buzz_push, event) = created_push(&keys, "secret", &community, ChatChannelKind::Workspace);
    assert_eq!(
        ingest_push(&service, &buzz_push).await.unwrap(),
        IngestOutcome::FirstObservation
    );
    let message_id = event.id.to_hex();
    {
        let mut state = fake.state.lock().unwrap();
        // The message exists at the relay, but the principal is NOT a channel
        // member (the channel is registered with no members).
        state.register_event(state_entry(
            &event,
            &community,
            CHANNEL_ID,
            ChatChannelKind::Workspace,
            ObservedEventType::Created,
        ));
        state.add_member(CHANNEL_ID, &hex64(0x01));
    }

    let gateway =
        Arc::new(BuzzGatewayClient::new(service_keys.clone(), Client::builder()).unwrap());
    let authorizer = authorizer_with_gateway(pool.clone(), gateway).await;
    let ctx = user_ctx(env.principal, tenant);
    let reference = chat_ref(&message_id);

    let decision = authorizer
        .authorize(&ctx, &chat_read_action(), &reference)
        .await;
    assert!(
        matches!(decision, Decision::Deny),
        "the relay's deny must surface as Deny, got {decision:?}"
    );
    let materialized = authorizer
        .materialize(
            &ctx,
            &chat_read_action(),
            vec![Candidate {
                resource: reference.clone(),
                cached_text: None,
            }],
        )
        .await
        .expect("materialization succeeds");
    assert!(
        materialized.is_empty(),
        "a denied candidate must not materialize"
    );
    assert!(
        matches!(
            authorizer
                .fetch(&ctx, &reference, Representation::Raw)
                .await,
            Err(SourceError::NotFound)
        ),
        "fetch must fail closed with the existence-hiding variant"
    );

    fake.stop().await;
    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 3. A non-member of a private channel is denied
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn private_channel_non_member_denied() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    cleanup(&pool, tenant).await;

    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    let relay_keys = Keys::generate();
    let service_keys = Keys::generate();
    let fake = start_fake_buzz(relay_keys.clone(), service_keys.public_key().to_hex());
    let relay_url = format!("ws://127.0.0.1:{}", fake.addr.port());
    let env = setup_tenant_with_relay(
        &pool,
        tenant,
        &keys,
        &community,
        &relay_url,
        &relay_keys.public_key().to_hex(),
        serde_json::json!({ "memory_projection": true }),
    )
    .await;

    let service = service(pool.clone());
    let (buzz_push, event) = created_push(
        &keys,
        "private secret",
        &community,
        ChatChannelKind::Private,
    );
    assert_eq!(
        ingest_push(&service, &buzz_push).await.unwrap(),
        IngestOutcome::FirstObservation
    );
    let message_id = event.id.to_hex();
    {
        let mut state = fake.state.lock().unwrap();
        state.register_event(state_entry(
            &event,
            &community,
            CHANNEL_ID,
            ChatChannelKind::Private,
            ObservedEventType::Created,
        ));
        state.set_channel_kind(CHANNEL_ID, "private");
        // A private channel whose member set does NOT include the principal.
        state.add_member(CHANNEL_ID, &hex64(0x02));
    }
    assert_eq!(
        fake.state
            .lock()
            .unwrap()
            .channel_kinds
            .get(CHANNEL_ID)
            .map(String::as_str),
        Some("private"),
        "the informational channel-kind registry records the kind"
    );

    let gateway =
        Arc::new(BuzzGatewayClient::new(service_keys.clone(), Client::builder()).unwrap());
    let authorizer = authorizer_with_gateway(pool.clone(), gateway).await;
    let ctx = user_ctx(env.principal, tenant);
    let reference = chat_ref(&message_id);

    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &reference)
            .await,
        Decision::Deny,
        "a non-member of a private channel is denied by the relay"
    );
    assert!(
        matches!(
            authorizer
                .fetch(&ctx, &reference, Representation::Raw)
                .await,
            Err(SourceError::NotFound)
        ),
        "fetch fails closed"
    );

    fake.stop().await;
    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 4. Membership removal at the relay takes effect immediately
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn membership_removal_immediately_denies() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    cleanup(&pool, tenant).await;

    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    let relay_keys = Keys::generate();
    let service_keys = Keys::generate();
    let fake = start_fake_buzz(relay_keys.clone(), service_keys.public_key().to_hex());
    let relay_url = format!("ws://127.0.0.1:{}", fake.addr.port());
    let env = setup_tenant_with_relay(
        &pool,
        tenant,
        &keys,
        &community,
        &relay_url,
        &relay_keys.public_key().to_hex(),
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;
    fake.state
        .lock()
        .unwrap()
        .add_member(CHANNEL_ID, &keys.public_key().to_hex());

    let service = service(pool.clone());
    let (buzz_push, event) =
        created_push(&keys, "still bound", &community, ChatChannelKind::Workspace);
    assert_eq!(
        ingest_push(&service, &buzz_push).await.unwrap(),
        IngestOutcome::FirstObservation
    );
    let message_id = event.id.to_hex();
    fake.state.lock().unwrap().register_event(state_entry(
        &event,
        &community,
        CHANNEL_ID,
        ChatChannelKind::Workspace,
        ObservedEventType::Created,
    ));

    let gateway =
        Arc::new(BuzzGatewayClient::new(service_keys.clone(), Client::builder()).unwrap());
    let authorizer = authorizer_with_gateway(pool.clone(), gateway).await;
    let ctx = user_ctx(env.principal, tenant);
    let reference = chat_ref(&message_id);

    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &reference)
            .await,
        Decision::Allow,
        "while the principal is a channel member, the relay allows"
    );

    // Membership removal at the RELAY ONLY — no Elembra-side state change.
    fake.state
        .lock()
        .unwrap()
        .remove_member(CHANNEL_ID, &keys.public_key().to_hex());

    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &reference)
            .await,
        Decision::Deny,
        "the very next decision reflects the relay's current state"
    );

    fake.stop().await;
    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 5. DM participant allowed (metadata only); outsider denied
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn dm_participant_allowed_outsider_denied() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    cleanup(&pool, tenant).await;

    let participant = Keys::generate();
    let outsider = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    let relay_keys = Keys::generate();
    let service_keys = Keys::generate();
    let fake = start_fake_buzz(relay_keys.clone(), service_keys.public_key().to_hex());
    let relay_url = format!("ws://127.0.0.1:{}", fake.addr.port());
    let relay_pubkey = relay_keys.public_key().to_hex();

    // Both principals are bound AND admitted locally (so the gate reaches the
    // relay for both); the relay's dm member set contains only the participant.
    let mapping_id =
        insert_mapping(&pool, tenant, &community, &relay_url, Some(&relay_pubkey)).await;
    let (participant_principal, participant_binding) =
        insert_binding(&pool, tenant, &participant.public_key().to_hex()).await;
    let (outsider_principal, outsider_binding) =
        insert_binding(&pool, tenant, &outsider.public_key().to_hex()).await;
    insert_admission(
        &pool,
        tenant,
        mapping_id,
        participant_binding,
        &participant.public_key().to_hex(),
    )
    .await;
    insert_admission(
        &pool,
        tenant,
        mapping_id,
        outsider_binding,
        &outsider.public_key().to_hex(),
    )
    .await;
    enable_chat(
        &pool,
        tenant,
        serde_json::json!({ "memory_projection": true }),
    )
    .await;
    fake.state
        .lock()
        .unwrap()
        .add_member(CHANNEL_ID, &participant.public_key().to_hex());

    let service = service(pool.clone());
    let (buzz_push, event) =
        created_push(&participant, "dm content", &community, ChatChannelKind::Dm);
    assert_eq!(
        ingest_push(&service, &buzz_push).await.unwrap(),
        IngestOutcome::FirstObservation,
        "dm events are observed (reference-first; never body-captured)"
    );
    let message_id = event.id.to_hex();
    fake.state.lock().unwrap().register_event(state_entry(
        &event,
        &community,
        CHANNEL_ID,
        ChatChannelKind::Dm,
        ObservedEventType::Created,
    ));

    let gateway =
        Arc::new(BuzzGatewayClient::new(service_keys.clone(), Client::builder()).unwrap());
    let authorizer = authorizer_with_gateway(pool.clone(), gateway).await;

    // Participant: the relay allows — DM bodies are never captured, so the
    // metadata resolves but the body is unavailable.
    let participant_ctx = user_ctx(participant_principal, tenant);
    let reference = chat_ref(&message_id);
    assert_eq!(
        authorizer
            .authorize(&participant_ctx, &chat_read_action(), &reference)
            .await,
        Decision::Allow,
        "the relay allows a dm participant"
    );
    let resolved = authorizer
        .resolve(&participant_ctx, &reference, Purpose::RagContext)
        .await
        .expect("participant resolves the dm metadata");
    assert!(
        !resolved.available,
        "dm bodies are never captured ⇒ reference-only metadata"
    );
    assert!(
        matches!(
            authorizer
                .fetch(&participant_ctx, &reference, Representation::Raw)
                .await,
            Err(SourceError::VersionUnavailable)
        ),
        "fetching a body that was never captured is VersionUnavailable"
    );

    // Outsider: the relay denies (not a participant); the exposure surface is
    // existence-hiding NotFound. (The typed decision is Deny — the fake's
    // prescribed semantics deny non-members; every non-Allow outcome looks
    // absent through resolve/fetch.)
    let outsider_ctx = user_ctx(outsider_principal, tenant);
    let decision = authorizer
        .authorize(&outsider_ctx, &chat_read_action(), &reference)
        .await;
    assert!(
        matches!(decision, Decision::Deny | Decision::NotFound),
        "an outsider can never read the dm, got {decision:?}"
    );
    assert!(
        matches!(
            authorizer
                .resolve(&outsider_ctx, &reference, Purpose::RagContext)
                .await,
            Err(SourceError::NotFound)
        ),
        "resolve must fail with the existence-hiding variant"
    );
    assert!(
        matches!(
            authorizer
                .fetch(&outsider_ctx, &reference, Representation::Raw)
                .await,
            Err(SourceError::NotFound)
        ),
        "fetch must fail with the existence-hiding variant"
    );

    fake.stop().await;
    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 6. A local community admission can never bypass the relay's channel decision
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn community_admission_cannot_bypass_channel_authorization() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    cleanup(&pool, tenant).await;

    let reader = Keys::generate();
    let author = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    let relay_keys = Keys::generate();
    let service_keys = Keys::generate();
    let fake = start_fake_buzz(relay_keys.clone(), service_keys.public_key().to_hex());
    let relay_url = format!("ws://127.0.0.1:{}", fake.addr.port());
    let relay_pubkey = relay_keys.public_key().to_hex();

    // The reader is bound + admitted locally (Elembra's own admission is
    // ACTIVE), but the relay's channel member set does not include the reader.
    let mapping_id =
        insert_mapping(&pool, tenant, &community, &relay_url, Some(&relay_pubkey)).await;
    let (reader_principal, reader_binding) =
        insert_binding(&pool, tenant, &reader.public_key().to_hex()).await;
    let (_author_principal, _author_binding) =
        insert_binding(&pool, tenant, &author.public_key().to_hex()).await;
    insert_admission(
        &pool,
        tenant,
        mapping_id,
        reader_binding,
        &reader.public_key().to_hex(),
    )
    .await;
    enable_chat(
        &pool,
        tenant,
        serde_json::json!({ "memory_projection": true }),
    )
    .await;

    let service = service(pool.clone());
    let (buzz_push, event) = created_push(
        &author,
        "channel secret",
        &community,
        ChatChannelKind::Workspace,
    );
    assert_eq!(
        ingest_push(&service, &buzz_push).await.unwrap(),
        IngestOutcome::FirstObservation
    );
    let message_id = event.id.to_hex();
    fake.state.lock().unwrap().register_event(state_entry(
        &event,
        &community,
        CHANNEL_ID,
        ChatChannelKind::Workspace,
        ObservedEventType::Created,
    ));
    // The channel exists with members that do NOT include the reader.
    fake.state
        .lock()
        .unwrap()
        .add_member(CHANNEL_ID, &author.public_key().to_hex());

    let gateway =
        Arc::new(BuzzGatewayClient::new(service_keys.clone(), Client::builder()).unwrap());
    let authorizer = authorizer_with_gateway(pool.clone(), gateway).await;
    let ctx = user_ctx(reader_principal, tenant);
    let reference = chat_ref(&message_id);

    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &reference)
            .await,
        Decision::Deny,
        "an Elembra-side admission is a coarse pre-filter only — the relay's \
         channel decision is final"
    );

    fake.stop().await;
    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 7. Stale Memory/observation state can never grant access
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn stale_memory_acl_cannot_grant_access() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    cleanup(&pool, tenant).await;

    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    let relay_keys = Keys::generate();
    let service_keys = Keys::generate();
    let fake = start_fake_buzz(relay_keys.clone(), service_keys.public_key().to_hex());
    let relay_url = format!("ws://127.0.0.1:{}", fake.addr.port());
    let env = setup_tenant_with_relay(
        &pool,
        tenant,
        &keys,
        &community,
        &relay_url,
        &relay_keys.public_key().to_hex(),
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;

    // Durable pipeline: push + project, so BOTH an observation row and a
    // Memory catalog record (with content) exist for the message.
    let store = outbox_store(pool.clone());
    register_consumer(&store).await;
    let service = service(pool.clone());
    let (buzz_push, event) = created_push(
        &keys,
        "indexed secret",
        &community,
        ChatChannelKind::Workspace,
    );
    assert_eq!(
        ingest_push(&service, &buzz_push).await.unwrap(),
        IngestOutcome::FirstObservation
    );
    let message_id = event.id.to_hex();
    dispatch_once(&pool, store).await;
    assert_eq!(
        catalog_count(&pool, tenant).await,
        1,
        "the record is projected"
    );
    {
        let mut state = fake.state.lock().unwrap();
        state.register_event(state_entry(
            &event,
            &community,
            CHANNEL_ID,
            ChatChannelKind::Workspace,
            ObservedEventType::Created,
        ));
        // The relay's CURRENT channel state denies the member.
        state.add_member(CHANNEL_ID, &hex64(0x03));
    }

    let gateway =
        Arc::new(BuzzGatewayClient::new(service_keys.clone(), Client::builder()).unwrap());
    let authorizer = authorizer_with_gateway(pool.clone(), gateway).await;
    let ctx = user_ctx(env.principal, tenant);
    let reference = chat_ref(&message_id);

    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &reference)
            .await,
        Decision::Deny,
        "a projected record + observation row must never grant access against a \
         current relay denial"
    );
    assert!(
        matches!(
            authorizer
                .fetch(&ctx, &reference, Representation::Raw)
                .await,
            Err(SourceError::NotFound)
        ),
        "fetch must not resurrect content from stale Memory/observation state"
    );

    fake.stop().await;
    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 8. Cross-tenant ref fails closed; wrong relay_pubkey pin fails closed
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn cross_tenant_and_community_denied() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant_a = TenantId::from(Uuid::new_v4());
    let tenant_b = TenantId::from(Uuid::new_v4());
    cleanup(&pool, tenant_a).await;
    cleanup(&pool, tenant_b).await;

    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    let relay_keys = Keys::generate();
    let service_keys = Keys::generate();
    let fake = start_fake_buzz(relay_keys.clone(), service_keys.public_key().to_hex());
    let relay_url = format!("ws://127.0.0.1:{}", fake.addr.port());
    let relay_pubkey = relay_keys.public_key().to_hex();
    let env = setup_tenant_with_relay(
        &pool,
        tenant_a,
        &keys,
        &community,
        &relay_url,
        &relay_pubkey,
        serde_json::json!({ "memory_projection": true }),
    )
    .await;

    let service = service(pool.clone());
    let (buzz_push, event) = created_push(
        &keys,
        "tenant a secret",
        &community,
        ChatChannelKind::Workspace,
    );
    assert_eq!(
        ingest_push(&service, &buzz_push).await.unwrap(),
        IngestOutcome::FirstObservation
    );
    let message_id = event.id.to_hex();
    {
        let mut state = fake.state.lock().unwrap();
        state.register_event(state_entry(
            &event,
            &community,
            CHANNEL_ID,
            ChatChannelKind::Workspace,
            ObservedEventType::Created,
        ));
        state.add_member(CHANNEL_ID, &keys.public_key().to_hex());
    }

    let gateway =
        Arc::new(BuzzGatewayClient::new(service_keys.clone(), Client::builder()).unwrap());
    let authorizer = authorizer_with_gateway(pool.clone(), gateway.clone()).await;

    // Case 1: tenant B's context through tenant A's authorizer — the lookup is
    // tenant-scoped, so the ref looks absent.
    let foreign_ctx = user_ctx(env.principal, tenant_b);
    assert_eq!(
        authorizer
            .authorize(&foreign_ctx, &chat_read_action(), &chat_ref(&message_id))
            .await,
        Decision::NotFound,
        "a cross-tenant ref must never be allowed"
    );
    assert!(
        matches!(
            authorizer
                .fetch(&foreign_ctx, &chat_ref(&message_id), Representation::Raw)
                .await,
            Err(SourceError::NotFound)
        ),
        "cross-tenant fetch must fail closed with the existence-hiding variant"
    );

    // Case 2: a mapping whose pinned relay_pubkey is the WRONG key — the relay
    // answers (NIP-98 + kind-19030 signed), the client rejects the signature
    // pin and fails closed to Deny.
    let wrong_pin = Keys::generate().public_key().to_hex();
    let env_b = setup_tenant_with_relay(
        &pool,
        tenant_b,
        &keys,
        &format!("community-{}", Uuid::new_v4()),
        &relay_url,
        &wrong_pin,
        serde_json::json!({ "memory_projection": true }),
    )
    .await;
    let community_b = env_b.community_id.clone();
    let (push_b, event_b) = created_push(
        &keys,
        "pinned wrongly",
        &community_b,
        ChatChannelKind::Workspace,
    );
    assert_eq!(
        ingest_push(&service, &push_b).await.unwrap(),
        IngestOutcome::FirstObservation
    );
    let message_b = event_b.id.to_hex();
    {
        let mut state = fake.state.lock().unwrap();
        state.register_event(state_entry(
            &event_b,
            &community_b,
            CHANNEL_ID,
            ChatChannelKind::Workspace,
            ObservedEventType::Created,
        ));
        state.add_member(CHANNEL_ID, &keys.public_key().to_hex());
    }

    let authorizer_b = authorizer_with_gateway(pool.clone(), gateway).await;
    assert_eq!(
        authorizer_b
            .authorize(
                &user_ctx(env_b.principal, tenant_b),
                &chat_read_action(),
                &chat_ref(&message_b)
            )
            .await,
        Decision::Deny,
        "a response not signed by the pinned relay key is an invalid response → Deny"
    );

    fake.stop().await;
    cleanup(&pool, tenant_a).await;
    cleanup(&pool, tenant_b).await;
}

// ---------------------------------------------------------------------------
// 9. An unreachable relay fails closed
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn buzz_unavailable_fails_closed() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    cleanup(&pool, tenant).await;

    // Reserve a port and release it: nothing is listening there.
    let dead_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dead_port = dead_listener.local_addr().unwrap().port();
    drop(dead_listener);

    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    let service_keys = Keys::generate();
    // Local admission passes; only the relay is unreachable.
    let env = setup_tenant_with_relay(
        &pool,
        tenant,
        &keys,
        &community,
        &format!("ws://127.0.0.1:{dead_port}"),
        &Keys::generate().public_key().to_hex(),
        serde_json::json!({ "memory_projection": true }),
    )
    .await;

    let service = service(pool.clone());
    let (buzz_push, event) =
        created_push(&keys, "unreachable", &community, ChatChannelKind::Workspace);
    assert_eq!(
        ingest_push(&service, &buzz_push).await.unwrap(),
        IngestOutcome::FirstObservation
    );
    let message_id = event.id.to_hex();

    let gateway =
        Arc::new(BuzzGatewayClient::new(service_keys.clone(), Client::builder()).unwrap());
    let authorizer = authorizer_with_gateway(pool.clone(), gateway).await;
    let ctx = user_ctx(env.principal, tenant);
    let reference = chat_ref(&message_id);

    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &reference)
            .await,
        Decision::Deny,
        "a transport failure must fail closed to Deny despite local admission"
    );
    assert!(
        matches!(
            authorizer
                .fetch(&ctx, &reference, Representation::Raw)
                .await,
            Err(SourceError::NotFound)
        ),
        "fetch fails closed when the relay is unreachable"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 10. No human user key is ever required server-side
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn no_user_key_required_server_side() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    cleanup(&pool, tenant).await;

    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    let relay_keys = Keys::generate();
    let service_keys = Keys::generate();
    let service_pubkey = service_keys.public_key().to_hex();
    let fake = start_fake_buzz(relay_keys.clone(), service_pubkey.clone());
    let relay_url = format!("ws://127.0.0.1:{}", fake.addr.port());
    let env = setup_tenant_with_relay(
        &pool,
        tenant,
        &keys,
        &community,
        &relay_url,
        &relay_keys.public_key().to_hex(),
        serde_json::json!({ "memory_projection": true }),
    )
    .await;
    fake.state
        .lock()
        .unwrap()
        .add_member(CHANNEL_ID, &keys.public_key().to_hex());

    let service = service(pool.clone());
    let (buzz_push, event) = created_push(
        &keys,
        "service key only",
        &community,
        ChatChannelKind::Workspace,
    );
    assert_eq!(
        ingest_push(&service, &buzz_push).await.unwrap(),
        IngestOutcome::FirstObservation
    );
    let message_id = event.id.to_hex();
    fake.state.lock().unwrap().register_event(state_entry(
        &event,
        &community,
        CHANNEL_ID,
        ChatChannelKind::Workspace,
        ObservedEventType::Created,
    ));

    // (a) The production request path signs with the SERVICE key: the fake
    // records the authenticated NIP-98 signer on every check.
    let gateway =
        Arc::new(BuzzGatewayClient::new(service_keys.clone(), Client::builder()).unwrap());
    let authorizer = authorizer_with_gateway(pool.clone(), gateway).await;
    let ctx = user_ctx(env.principal, tenant);
    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &chat_ref(&message_id))
            .await,
        Decision::Allow
    );
    let recorded = fake
        .state
        .lock()
        .unwrap()
        .access_check_requests
        .last()
        .expect("the check was recorded")
        .clone();
    assert_eq!(
        recorded["auth_pubkey"].as_str(),
        Some(service_pubkey.as_str()),
        "the relay authenticated the SERVICE pubkey, not any user key"
    );

    // (b) A forged request signed by a user-like (non-service) key is rejected
    // with 401 — the relay accepts only the provisioned service identity.
    let forged_url = Url::parse(&format!(
        "http://127.0.0.1:{}/api/v1/relay/access/check",
        fake.addr.port()
    ))
    .unwrap();
    let forged_body = serde_json::to_vec(&BuzzAccessCheckRequest {
        pubkey: keys.public_key().to_hex(),
        channel_id: CHANNEL_ID.to_string(),
        channel_kind: BuzzChannelKind::Workspace,
        message_id: Some(message_id.clone()),
        event_created_at: None,
    })
    .unwrap();
    let forged_keys = Keys::generate();
    let tags = vec![
        Tag::parse(["u", forged_url.as_str()]).expect("u tag"),
        Tag::parse(["method", "POST"]).expect("method tag"),
        Tag::parse(["payload", &hex::encode(Sha256::digest(&forged_body))]).expect("payload tag"),
    ];
    let forged_event = EventBuilder::new(Kind::HttpAuth, String::new())
        .tags(tags)
        .sign_with_keys(&forged_keys)
        .expect("sign the forged auth event");
    let forged_header = format!("Nostr {}", STANDARD.encode(forged_event.as_json()));
    let response = Client::new()
        .post(forged_url)
        .header("Authorization", forged_header)
        .header("Content-Type", "application/json")
        .body(forged_body)
        .send()
        .await
        .expect("the forged request reaches the fake relay");
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "a non-service NIP-98 signer must be rejected"
    );

    // (c) Structural: `chat_identity_bindings` stores ONLY the 64-hex pubkey —
    // the successful insert above already proves the schema accepts a bare
    // pubkey; assert there is no private-key column to hold a user secret.
    let rows = sqlx::query(
        "SELECT column_name FROM information_schema.columns
         WHERE table_schema = 'public' AND table_name = 'chat_identity_bindings'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let columns: Vec<String> = rows
        .iter()
        .map(|row| row.get::<String, _>("column_name"))
        .collect();
    assert!(
        columns.iter().any(|column| column == "buzz_pubkey"),
        "the binding stores the pubkey: {columns:?}"
    );
    assert!(
        columns
            .iter()
            .all(|column| !column.contains("secret") && !column.contains("private")),
        "no private-key/secret column may exist: {columns:?}"
    );

    fake.stop().await;
    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 11. Reconcile-from-Buzz repairs a missing Memory projection (no outbox writes)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn reconcile_repairs_missing_memory_projection() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    cleanup(&pool, tenant).await;

    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    let relay_keys = Keys::generate();
    let service_keys = Keys::generate();
    let fake = start_fake_buzz(relay_keys.clone(), service_keys.public_key().to_hex());
    let relay_url = format!("ws://127.0.0.1:{}", fake.addr.port());
    setup_tenant_with_relay(
        &pool,
        tenant,
        &keys,
        &community,
        &relay_url,
        &relay_keys.public_key().to_hex(),
        serde_json::json!({ "memory_projection": true }),
    )
    .await;

    // Register the entries in the fake FIRST, then push the same signed
    // events through the bridge + durable pipeline.
    let service = service(pool.clone());
    let (push1, event1) =
        created_push(&keys, "message one", &community, ChatChannelKind::Workspace);
    let (push2, event2) =
        created_push(&keys, "message two", &community, ChatChannelKind::Workspace);
    {
        let mut state = fake.state.lock().unwrap();
        state.register_event(state_entry(
            &event1,
            &community,
            CHANNEL_ID,
            ChatChannelKind::Workspace,
            ObservedEventType::Created,
        ));
        state.register_event(state_entry(
            &event2,
            &community,
            CHANNEL_ID,
            ChatChannelKind::Workspace,
            ObservedEventType::Created,
        ));
    }
    let store = outbox_store(pool.clone());
    register_consumer(&store).await;
    assert_eq!(
        ingest_push(&service, &push1).await.unwrap(),
        IngestOutcome::FirstObservation
    );
    assert_eq!(
        ingest_push(&service, &push2).await.unwrap(),
        IngestOutcome::FirstObservation
    );
    dispatch_once(&pool, store).await;
    assert_eq!(catalog_count(&pool, tenant).await, 2);
    let outbox_before = outbox_count(&pool, tenant).await;

    // Corrupt ONLY the projection: drop the catalog records. The observation
    // index (and the outbox) stay intact.
    sqlx::query("DELETE FROM memory_catalog WHERE tenant_id = $1")
        .bind(tenant.0)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(catalog_count(&pool, tenant).await, 0);

    let gateway = BuzzGatewayClient::new(service_keys.clone(), Client::builder()).unwrap();
    let counts = reconcile_from_buzz(&pool, tenant, &gateway, None).await;
    assert_eq!(counts.processed, 2, "both relay entries are examined");
    assert_eq!(counts.created, 2, "both records are rebuilt exactly once");
    assert_eq!(catalog_count(&pool, tenant).await, 2);
    assert_eq!(
        outbox_count(&pool, tenant).await,
        outbox_before,
        "reconcile-from-buzz must not write to the outbox"
    );

    fake.stop().await;
    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 12. Reconcile-from-Buzz repairs a missing observation index via replay
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn reconcile_repairs_missing_observation_via_buzz_replay() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    cleanup(&pool, tenant).await;

    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    let relay_keys = Keys::generate();
    let service_keys = Keys::generate();
    let fake = start_fake_buzz(relay_keys.clone(), service_keys.public_key().to_hex());
    let relay_url = format!("ws://127.0.0.1:{}", fake.addr.port());
    setup_tenant_with_relay(
        &pool,
        tenant,
        &keys,
        &community,
        &relay_url,
        &relay_keys.public_key().to_hex(),
        serde_json::json!({ "memory_projection": true }),
    )
    .await;

    let service = service(pool.clone());
    let (push1, event1) = created_push(&keys, "replay one", &community, ChatChannelKind::Workspace);
    let (push2, event2) = created_push(&keys, "replay two", &community, ChatChannelKind::Workspace);
    {
        let mut state = fake.state.lock().unwrap();
        state.register_event(state_entry(
            &event1,
            &community,
            CHANNEL_ID,
            ChatChannelKind::Workspace,
            ObservedEventType::Created,
        ));
        state.register_event(state_entry(
            &event2,
            &community,
            CHANNEL_ID,
            ChatChannelKind::Workspace,
            ObservedEventType::Created,
        ));
    }
    let store = outbox_store(pool.clone());
    register_consumer(&store).await;
    assert_eq!(
        ingest_push(&service, &push1).await.unwrap(),
        IngestOutcome::FirstObservation
    );
    assert_eq!(
        ingest_push(&service, &push2).await.unwrap(),
        IngestOutcome::FirstObservation
    );
    dispatch_once(&pool, store).await;
    assert_eq!(observation_count(&pool, tenant).await, 2);
    assert_eq!(catalog_count(&pool, tenant).await, 2);

    // Simulate corruption of BOTH the observation index and the projection.
    for table in ["chat_observed_events", "memory_catalog"] {
        sqlx::query(&format!("DELETE FROM {table} WHERE tenant_id = $1"))
            .bind(tenant.0)
            .execute(&pool)
            .await
            .unwrap();
    }
    assert_eq!(observation_count(&pool, tenant).await, 0);
    assert_eq!(catalog_count(&pool, tenant).await, 0);

    let gateway = BuzzGatewayClient::new(service_keys.clone(), Client::builder()).unwrap();
    let counts = reconcile_from_buzz(&pool, tenant, &gateway, None).await;
    assert_eq!(counts.processed, 2, "both relay entries are examined");
    assert_eq!(counts.created, 2, "both records are built");
    assert_eq!(
        observation_count(&pool, tenant).await,
        2,
        "the observation index is repaired from the relay's state over HTTP"
    );
    assert_eq!(
        catalog_count(&pool, tenant).await,
        2,
        "the projection is folded from the repaired index"
    );

    fake.stop().await;
    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 13. Reconcile-from-Buzz is idempotent
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn reconcile_duplicate_replay_is_idempotent() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    cleanup(&pool, tenant).await;

    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    let relay_keys = Keys::generate();
    let service_keys = Keys::generate();
    let fake = start_fake_buzz(relay_keys.clone(), service_keys.public_key().to_hex());
    let relay_url = format!("ws://127.0.0.1:{}", fake.addr.port());
    setup_tenant_with_relay(
        &pool,
        tenant,
        &keys,
        &community,
        &relay_url,
        &relay_keys.public_key().to_hex(),
        serde_json::json!({ "memory_projection": true }),
    )
    .await;

    let (_push1, event1) = created_push(&keys, "one", &community, ChatChannelKind::Workspace);
    let (_push2, event2) = created_push(&keys, "two", &community, ChatChannelKind::Workspace);
    {
        let mut state = fake.state.lock().unwrap();
        state.register_event(state_entry(
            &event1,
            &community,
            CHANNEL_ID,
            ChatChannelKind::Workspace,
            ObservedEventType::Created,
        ));
        state.register_event(state_entry(
            &event2,
            &community,
            CHANNEL_ID,
            ChatChannelKind::Workspace,
            ObservedEventType::Created,
        ));
    }
    let store = outbox_store(pool.clone());
    register_consumer(&store).await;

    let gateway = BuzzGatewayClient::new(service_keys.clone(), Client::builder()).unwrap();
    let first = reconcile_from_buzz(&pool, tenant, &gateway, None).await;
    assert_eq!(first.processed, 2);
    assert_eq!(first.created, 2);
    assert_eq!(observation_count(&pool, tenant).await, 2);
    assert_eq!(catalog_count(&pool, tenant).await, 2);

    let second = reconcile_from_buzz(&pool, tenant, &gateway, None).await;
    assert_eq!(
        second.processed, 2,
        "the same relay entries are re-examined"
    );
    assert_eq!(
        second.created, 0,
        "re-running must not re-create observations or records"
    );
    // `upsert_records` counts every conflict-path row as an `updated` row
    // (`ON CONFLICT DO UPDATE ... RETURNING (xmax = 0)`), so `updated` is not
    // an idempotency signal; the invariant is `created == 0` and unchanged
    // row counts (the same assertion the memory-projection suites use).
    assert_eq!(observation_count(&pool, tenant).await, 2);
    assert_eq!(
        catalog_count(&pool, tenant).await,
        2,
        "exactly one row per message"
    );

    fake.stop().await;
    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 14. The repair flows over the public HTTP contract — the relay has no DB
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn reconcile_flows_through_http_contract_without_any_db() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    cleanup(&pool, tenant).await;

    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    let relay_keys = Keys::generate();
    let service_keys = Keys::generate();
    let fake = start_fake_buzz(relay_keys.clone(), service_keys.public_key().to_hex());
    let relay_url = format!("ws://127.0.0.1:{}", fake.addr.port());
    setup_tenant_with_relay(
        &pool,
        tenant,
        &keys,
        &community,
        &relay_url,
        &relay_keys.public_key().to_hex(),
        serde_json::json!({ "memory_projection": true }),
    )
    .await;

    let (_push1, event1) = created_push(&keys, "http one", &community, ChatChannelKind::Workspace);
    let (_push2, event2) = created_push(&keys, "http two", &community, ChatChannelKind::Workspace);
    {
        let mut state = fake.state.lock().unwrap();
        state.register_event(state_entry(
            &event1,
            &community,
            CHANNEL_ID,
            ChatChannelKind::Workspace,
            ObservedEventType::Created,
        ));
        state.register_event(state_entry(
            &event2,
            &community,
            CHANNEL_ID,
            ChatChannelKind::Workspace,
            ObservedEventType::Created,
        ));
    }

    // Structural guarantee (documented, not just asserted): the fake relay has
    // NO database — `FakeBuzzState` is an in-memory HashMap/Vec topology and
    // there is no relay database anywhere in this test binary. The only way
    // the repair can read the relay's state is over the public HTTP endpoints.
    let gateway = BuzzGatewayClient::new(service_keys.clone(), Client::builder()).unwrap();
    let counts = reconcile_from_buzz(&pool, tenant, &gateway, None).await;
    assert_eq!(counts.created, 2);
    assert!(
        fake.state.lock().unwrap().state_requests >= 1,
        "the repair served state over the public HTTP contract"
    );
    assert_eq!(observation_count(&pool, tenant).await, 2);
    assert_eq!(catalog_count(&pool, tenant).await, 2);

    fake.stop().await;
    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// N1. Deleted messages are not_found end-to-end (tombstone semantics)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn deleted_message_is_not_found() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    cleanup(&pool, tenant).await;

    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    let relay_keys = Keys::generate();
    let service_keys = Keys::generate();
    let fake = start_fake_buzz(relay_keys.clone(), service_keys.public_key().to_hex());
    let relay_url = format!("ws://127.0.0.1:{}", fake.addr.port());
    let env = setup_tenant_with_relay(
        &pool,
        tenant,
        &keys,
        &community,
        &relay_url,
        &relay_keys.public_key().to_hex(),
        serde_json::json!({ "memory_projection": true }),
    )
    .await;
    fake.state
        .lock()
        .unwrap()
        .add_member(CHANNEL_ID, &keys.public_key().to_hex());

    let service = service(pool.clone());
    let (buzz_push, event) = created_push(&keys, "doomed", &community, ChatChannelKind::Workspace);
    assert_eq!(
        ingest_push(&service, &buzz_push).await.unwrap(),
        IngestOutcome::FirstObservation
    );
    let message_id = event.id.to_hex();
    fake.state.lock().unwrap().register_event(state_entry(
        &event,
        &community,
        CHANNEL_ID,
        ChatChannelKind::Workspace,
        ObservedEventType::Created,
    ));

    let gateway =
        Arc::new(BuzzGatewayClient::new(service_keys.clone(), Client::builder()).unwrap());
    let authorizer = authorizer_with_gateway(pool.clone(), gateway).await;
    let ctx = user_ctx(env.principal, tenant);
    let reference = chat_ref(&message_id);
    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &reference)
            .await,
        Decision::Allow,
        "before deletion the member is allowed"
    );

    fake.state.lock().unwrap().mark_deleted(&message_id);

    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &reference)
            .await,
        Decision::NotFound,
        "a deleted message must look absent"
    );
    assert!(
        matches!(
            authorizer
                .fetch(&ctx, &reference, Representation::Raw)
                .await,
            Err(SourceError::NotFound)
        ),
        "fetch fails closed after deletion"
    );

    fake.stop().await;
    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// N2. Binding rotation asks the relay for the NEW pubkey
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn binding_rotation_asks_new_pubkey() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    cleanup(&pool, tenant).await;

    let old_keys = Keys::generate();
    let new_keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    let relay_keys = Keys::generate();
    let service_keys = Keys::generate();
    let fake = start_fake_buzz(relay_keys.clone(), service_keys.public_key().to_hex());
    let relay_url = format!("ws://127.0.0.1:{}", fake.addr.port());
    let relay_pubkey = relay_keys.public_key().to_hex();

    // Initial state: the principal is bound + admitted with the OLD pubkey.
    let principal = PrincipalId::from(Uuid::new_v4());
    let mapping_id =
        insert_mapping(&pool, tenant, &community, &relay_url, Some(&relay_pubkey)).await;
    let old_binding =
        insert_binding_for_principal(&pool, tenant, principal, &old_keys.public_key().to_hex())
            .await;
    insert_admission(
        &pool,
        tenant,
        mapping_id,
        old_binding,
        &old_keys.public_key().to_hex(),
    )
    .await;
    enable_chat(
        &pool,
        tenant,
        serde_json::json!({ "memory_projection": true }),
    )
    .await;

    // Rotate: revoke the old binding, bind the SAME principal to the new
    // pubkey, admit the new pubkey. The relay's channel membership now
    // contains the NEW pubkey only — the OLD pubkey is not a member.
    revoke_bindings(&pool, tenant).await;
    let new_binding =
        insert_binding_for_principal(&pool, tenant, principal, &new_keys.public_key().to_hex())
            .await;
    insert_admission(
        &pool,
        tenant,
        mapping_id,
        new_binding,
        &new_keys.public_key().to_hex(),
    )
    .await;
    fake.state
        .lock()
        .unwrap()
        .add_member(CHANNEL_ID, &new_keys.public_key().to_hex());

    let service = service(pool.clone());
    let (buzz_push, event) = created_push(
        &new_keys,
        "after rotation",
        &community,
        ChatChannelKind::Workspace,
    );
    assert_eq!(
        ingest_push(&service, &buzz_push).await.unwrap(),
        IngestOutcome::FirstObservation
    );
    let message_id = event.id.to_hex();
    fake.state.lock().unwrap().register_event(state_entry(
        &event,
        &community,
        CHANNEL_ID,
        ChatChannelKind::Workspace,
        ObservedEventType::Created,
    ));

    let gateway =
        Arc::new(BuzzGatewayClient::new(service_keys.clone(), Client::builder()).unwrap());
    let authorizer = authorizer_with_gateway(pool.clone(), gateway.clone()).await;
    let ctx = user_ctx(principal, tenant);
    let reference = chat_ref(&message_id);

    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &reference)
            .await,
        Decision::Allow,
        "after rotation the principal is allowed via the NEW pubkey"
    );
    let recorded = fake
        .state
        .lock()
        .unwrap()
        .access_check_requests
        .last()
        .expect("the check was recorded")
        .clone();
    let new_pubkey = new_keys.public_key().to_hex();
    assert_eq!(
        recorded["request"]["pubkey"].as_str(),
        Some(new_pubkey.as_str()),
        "the relay was asked about the NEW pubkey, never the revoked old one"
    );

    // The fake allows the new pubkey and denies the old one: ask it directly
    // with the revoked pubkey to prove the old key carries no channel rights.
    let denied = gateway
        .check_access(
            &relay_url,
            &relay_pubkey,
            &BuzzAccessCheckRequest {
                pubkey: old_keys.public_key().to_hex(),
                channel_id: CHANNEL_ID.to_string(),
                channel_kind: BuzzChannelKind::Workspace,
                message_id: Some(message_id),
                event_created_at: None,
            },
        )
        .await
        .expect("the direct check reaches the relay");
    assert_eq!(
        denied,
        BuzzReadDecision::Deny,
        "the revoked (old) pubkey is not a member at the relay"
    );

    fake.stop().await;
    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// N3. Unknown channels are not_found
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn unknown_channel_returns_not_found() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    cleanup(&pool, tenant).await;

    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    let relay_keys = Keys::generate();
    let service_keys = Keys::generate();
    let fake = start_fake_buzz(relay_keys.clone(), service_keys.public_key().to_hex());
    let relay_url = format!("ws://127.0.0.1:{}", fake.addr.port());
    let env = setup_tenant_with_relay(
        &pool,
        tenant,
        &keys,
        &community,
        &relay_url,
        &relay_keys.public_key().to_hex(),
        serde_json::json!({ "memory_projection": true }),
    )
    .await;

    let service = service(pool.clone());
    let (buzz_push, event) = created_push(
        &keys,
        "ghost channel",
        &community,
        ChatChannelKind::Workspace,
    );
    assert_eq!(
        ingest_push(&service, &buzz_push).await.unwrap(),
        IngestOutcome::FirstObservation
    );
    let message_id = event.id.to_hex();
    // The message exists at the relay, but the channel was never registered:
    // an unknown channel is indistinguishable from a missing message.
    fake.state.lock().unwrap().register_event(state_entry(
        &event,
        &community,
        CHANNEL_ID,
        ChatChannelKind::Workspace,
        ObservedEventType::Created,
    ));

    let gateway =
        Arc::new(BuzzGatewayClient::new(service_keys.clone(), Client::builder()).unwrap());
    let authorizer = authorizer_with_gateway(pool.clone(), gateway).await;
    let ctx = user_ctx(env.principal, tenant);
    let reference = chat_ref(&message_id);

    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &reference)
            .await,
        Decision::NotFound,
        "an unknown channel is existence-hidden (not_found)"
    );
    assert!(
        matches!(
            authorizer
                .fetch(&ctx, &reference, Representation::Raw)
                .await,
            Err(SourceError::NotFound)
        ),
        "fetch fails closed for an unknown channel"
    );

    fake.stop().await;
    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// N4. The tenant-scope guard skips foreign-community entries on a shared relay
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn tenant_scope_guard_skips_foreign_community_entries() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant_a = TenantId::from(Uuid::new_v4());
    let tenant_b = TenantId::from(Uuid::new_v4());
    cleanup(&pool, tenant_a).await;
    cleanup(&pool, tenant_b).await;

    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    let foreign_community = format!("community-{}", Uuid::new_v4());
    let relay_keys = Keys::generate();
    let service_keys = Keys::generate();
    let fake = start_fake_buzz(relay_keys.clone(), service_keys.public_key().to_hex());
    let relay_url = format!("ws://127.0.0.1:{}", fake.addr.port());

    // Tenant A: the mapping being repaired. Tenant B: a DIFFERENT active
    // mapping for the foreign community + a binding for the author, so the
    // foreign entry WOULD be fully ingestible into B's observation index
    // without the tenant-scope guard (proving the guard is what skips it).
    setup_tenant_with_relay(
        &pool,
        tenant_a,
        &keys,
        &community,
        &relay_url,
        &relay_keys.public_key().to_hex(),
        serde_json::json!({ "memory_projection": true }),
    )
    .await;
    insert_mapping(
        &pool,
        tenant_b,
        &foreign_community,
        "wss://relay.example.test",
        None,
    )
    .await;
    insert_binding(&pool, tenant_b, &keys.public_key().to_hex()).await;

    // The relay (simulating a shared relay serving several communities) pages
    // one entry for tenant A's community and one for the foreign community.
    let (_push1, event1) = created_push(&keys, "mine", &community, ChatChannelKind::Workspace);
    let (_push2, event2) = created_push(
        &keys,
        "not mine",
        &foreign_community,
        ChatChannelKind::Workspace,
    );
    {
        let mut state = fake.state.lock().unwrap();
        state.register_event(state_entry(
            &event1,
            &community,
            CHANNEL_ID,
            ChatChannelKind::Workspace,
            ObservedEventType::Created,
        ));
        state.register_event(state_entry(
            &event2,
            &foreign_community,
            CHANNEL_ID,
            ChatChannelKind::Workspace,
            ObservedEventType::Created,
        ));
    }

    let gateway = BuzzGatewayClient::new(service_keys.clone(), Client::builder()).unwrap();
    let counts = reconcile_from_buzz(&pool, tenant_a, &gateway, None).await;
    assert_eq!(
        counts.processed, 2,
        "both entries are paged and counted (the foreign one is skipped)"
    );
    assert_eq!(counts.created, 1, "only the tenant's own entry is folded");
    assert_eq!(observation_count(&pool, tenant_a).await, 1);
    assert_eq!(
        observation_count(&pool, tenant_b).await,
        0,
        "the foreign-community entry must not leak into another tenant's index"
    );
    assert_eq!(catalog_count(&pool, tenant_a).await, 1);

    fake.stop().await;
    cleanup(&pool, tenant_a).await;
    cleanup(&pool, tenant_b).await;
}

/// A fixed 64-lowercase-hex id for placeholder (non-member) pubkeys.
fn hex64(seed: u8) -> String {
    format!("{seed:02x}").repeat(32)
}
