//! DB-backed E2E security suite for the permission-aware unified search
//! endpoint (`POST /api/v1/search` → [`UnifiedSearchService`]).
//!
//! Every returned result must be authorized by its CURRENT owning source at
//! response time: Files/Notes candidates are reauthorized against the Files
//! permission semantics (`FilesResourceOwner` → `PermissionResolver`), Chat
//! candidates against the CURRENT Buzz authority (`ChatResourceOwner` →
//! `BuzzAuthority`). Index rows and Memory records are never authorization:
//! a stale/malicious hint can only *propose* a candidate; only the authorized
//! `fetch` bytes may become a snippet.
//!
//! Harness:
//! * Real Postgres pool; `MetadataStore`, `ObjectStore` (RustFS,
//!   `auto_create_bucket`), `PermissionResolver` + repository,
//!   `ChatIdentityStore`, `ChatObservationStore`, `MemoryCatalogStore`.
//! * A `SourceAuthorizer` with BOTH `FilesResourceOwner` and
//!   `ChatResourceOwner::with_authority(...)` registered against
//!   `ApplicationRegistry::first_party()`.
//! * Two chat-authority wiring modes: an in-process [`ScriptedAuthority`]
//!   (Allow/Deny/NotFound/Error per message id, mutable mid-test) used by
//!   most tests, and the REAL [`BuzzGatewayAuthority`] against an in-test fake
//!   HTTP relay (copied from `buzz_authority_gateway_test.rs`) for the
//!   full-stack proof.
//! * Chat fixtures insert the observation row directly into
//!   `chat_observed_events` (no outbox machinery) and project the Memory
//!   catalog record via `MemoryCatalogStore::upsert_records` — this still
//!   exercises the REAL `ChatResourceOwner` gate (observation lookup,
//!   tombstone override, enablement, binding, admission, mapping, then the
//!   authority).
//! * The note index (when AI is enabled) is a `ContentIndexer` over
//!   `InMemoryVectorStore` + `SimpleEmbeddingGenerator`, populated via
//!   `index_note`; a direct-SQL `note_index_chunks` row documents the
//!   DB-level stale-ACL fixtures.
//!
//! DB-backed and `#[ignore]`d; run against the dev database (migrations
//! applied) with `--test-threads=1`:
//!
//!   set -a; . ./backend/.env; set +a; CARGO_INCREMENTAL=0 SQLX_OFFLINE=true \
//!     cargo test -p rustshare-server --test unified_search_test -- \
//!       --ignored --test-threads=1
//!
//! Every test takes the shared `SERIAL` guard and cleans up exactly the rows
//! it created under a fresh tenant (same convention as the chat-owner and
//! buzz-authority-gateway suites).

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, LazyLock, Mutex};

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode, Uri};
use axum::routing::{get, post};
use axum::{Json, Router};
use bytes::Bytes;
use chrono::{DateTime, Duration, Utc};
use nostr::nips::nip98::{verify_auth_header, HttpMethod};
use nostr::{Event as NostrEvent, EventBuilder, JsonUtil, Keys, Kind, Timestamp};
use reqwest::Client;
use rustshare_core::domain::{
    ActionCapability, ApplicationId, ApplicationRegistry, PrincipalId, Share, SharePermissions,
    TenantId, User, WorkspaceId,
};
use rustshare_core::services::ai::IndexPrincipal;
use rustshare_core::services::{
    AiService, ContentIndexer, EmbeddingPolicy, InMemoryVectorStore, IndexAclProjection,
    IndexVisibility, PermissionResolver, SimpleEmbeddingGenerator,
};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_memory::event::{ChatChannelKind, ObservedEventType};
use rustshare_memory::record::{
    AUTHORIZATION_SOURCE_BUZZ, DEFAULT_CLASSIFICATION, SOURCE_APPLICATION, SOURCE_TYPE_MESSAGE,
};
use rustshare_memory::{IndexingStatus, MemoryCatalogRecord};
use rustshare_resource_auth::{
    BuzzAuthority, BuzzAuthorityError, BuzzChannelKind, BuzzReadDecision, BuzzReadRequest,
    Decision, PrincipalContext, Representation, ResourceOwnerRegistry, ResourceRef,
    SourceAuthorizer, SourceError, CHAT_READ, FILES_READ,
};
use rustshare_server::authz::{ChatResourceOwner, FilesResourceOwner};
use rustshare_server::buzz_gateway::{
    BuzzGatewayAuthority, BuzzGatewayClient, BuzzStateContext, BuzzStateEntry,
};
#[cfg(feature = "test-recording-provider")]
use rustshare_server::services::ask_workspace::{
    AskWorkspaceService, LlmResult, RecordingLlmProvider, SYSTEM_POLICY,
};
#[cfg(feature = "test-recording-provider")]
use rustshare_server::services::unified_search::SearchScope;
use rustshare_server::services::unified_search::{
    SearchSource, UnifiedSearchResponse, UnifiedSearchService,
};
use rustshare_server::state::AppAiService;
use rustshare_storage::{
    ChatIdentityStore, ChatObservationStore, MemoryCatalogStore, MetadataStore, ObjectStore,
    ObjectStoreOptions,
};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use url::Url;
use uuid::Uuid;

/// Serializes the tests within this binary (same convention as the chat-owner
/// and buzz-authority-gateway suites).
static SERIAL: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

/// The fake relay's single channel id (matches the observation fixture).
const CHANNEL_ID: &str = "channel-1";

// ---------------------------------------------------------------------------
// Scripted Buzz authority (in-process test double)
// ---------------------------------------------------------------------------

/// A canned outcome for one message, or for every message (the fallback).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScriptedOutcome {
    Allow,
    Deny,
    // Part of the required outcome surface of the scripted authority; the
    // gate's own `NotFound` path is exercised separately via the observation
    // tombstone fixtures, so no test currently scripts it directly.
    #[allow(dead_code)]
    NotFound,
    Error,
}

/// Shared state behind [`ScriptedAuthority`] (cloned handles observe the same
/// outcomes, so the harness and the owner adapter always agree).
struct ScriptedState {
    outcomes: Mutex<HashMap<String, ScriptedOutcome>>,
    fallback: Mutex<ScriptedOutcome>,
    /// Every message id the authority was asked about, in call order (used to
    /// prove the decision came from the authority for the exact candidate).
    asked: Mutex<Vec<String>>,
}

/// In-process [`BuzzAuthority`] with a per-message outcome table plus a
/// fallback. Mutable mid-test to simulate revocation at Buzz; cheaply cloned
/// so the harness and the owner adapter share one state.
#[derive(Clone)]
struct ScriptedAuthority {
    inner: Arc<ScriptedState>,
}

impl ScriptedAuthority {
    fn new(fallback: ScriptedOutcome) -> Self {
        Self {
            inner: Arc::new(ScriptedState {
                outcomes: Mutex::new(HashMap::new()),
                fallback: Mutex::new(fallback),
                asked: Mutex::new(Vec::new()),
            }),
        }
    }

    fn set(&self, message_id: &str, outcome: ScriptedOutcome) {
        self.inner
            .outcomes
            .lock()
            .unwrap()
            .insert(message_id.to_string(), outcome);
    }

    fn set_fallback(&self, outcome: ScriptedOutcome) {
        *self.inner.fallback.lock().unwrap() = outcome;
    }

    fn asked(&self) -> Vec<String> {
        self.inner.asked.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl BuzzAuthority for ScriptedAuthority {
    async fn can_read(
        &self,
        req: &BuzzReadRequest,
    ) -> Result<BuzzReadDecision, BuzzAuthorityError> {
        let message_id = req.message_id.clone().unwrap_or_default();
        self.inner.asked.lock().unwrap().push(message_id.clone());
        let outcome = self
            .inner
            .outcomes
            .lock()
            .unwrap()
            .get(&message_id)
            .copied()
            .unwrap_or_else(|| *self.inner.fallback.lock().unwrap());
        match outcome {
            ScriptedOutcome::Allow => Ok(BuzzReadDecision::Allow),
            ScriptedOutcome::Deny => Ok(BuzzReadDecision::Deny),
            ScriptedOutcome::NotFound => Ok(BuzzReadDecision::NotFound),
            ScriptedOutcome::Error => Err(BuzzAuthorityError::Transport(
                "scripted transport failure".to_string(),
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Fake Buzz relay (in-memory only — no database; copied from the gateway suite)
// ---------------------------------------------------------------------------

/// In-memory state of the fake relay implementing the v1alpha1 upstream
/// contract: NIP-98 service auth, `POST /api/v1/relay/access/check` and
/// `GET /api/v1/relay/state/events`, with kind-19030 responses signed by the
/// relay key.
struct FakeBuzzState {
    relay_keys: Keys,
    service_pubkey: String,
    channels: HashMap<String, HashSet<String>>,
    messages: HashMap<String, String>,
    deleted: HashSet<String>,
    events: Vec<BuzzStateEntry>,
    access_check_requests: Vec<serde_json::Value>,
    state_requests: u64,
}

impl FakeBuzzState {
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
}

/// Cloneable server handle for the axum handlers.
#[derive(Clone)]
struct FakeServerHandle {
    state: Arc<Mutex<FakeBuzzState>>,
    host: String,
}

/// A running fake relay: address, shared state, the serve task, and a
/// shutdown trigger.
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
/// endpoint path (NIP-98 compares serialized URLs).
fn request_url(host: &str, headers: &HeaderMap, path: &str) -> Url {
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or(host);
    Url::parse(&format!("http://{host}{path}")).expect("fake relay URL parses")
}

/// Rebuild the exact state-events URL the client signed, appending the query
/// pairs in the same order the client uses (since, limit, cursor).
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
    let request: rustshare_server::buzz_gateway::BuzzAccessCheckRequest =
        serde_json::from_slice(&body_bytes).map_err(|e| {
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

/// Parse a query string into a map (values are plain digits/cursors).
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
/// page the in-memory event state. The response is a kind-19030 event signed
/// by the relay key whose content is `{ entries, cursor, complete }`.
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
// Small helpers
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

/// A fixed 64-lowercase-hex id for placeholder columns.
fn hex64(seed: u8) -> String {
    format!("{seed:02x}").repeat(32)
}

/// A unique 64-lowercase-hex message id (deterministic per seed).
fn message_id(seed: u64) -> String {
    format!("{seed:064x}")
}

/// A plain human-user principal context (workspace == tenant per the platform
/// invariant).
fn user_ctx(principal: PrincipalId, tenant: TenantId) -> PrincipalContext {
    PrincipalContext::user(principal, tenant, WorkspaceId(tenant.0))
}

/// A canonical ref for a Files file.
fn file_ref(file_id: Uuid) -> ResourceRef {
    ResourceRef::new(
        ApplicationId::new("io.elembra.files"),
        "file",
        file_id.to_string(),
    )
}

/// A canonical ref for a Chat message.
fn chat_ref(message_id: &str) -> ResourceRef {
    ResourceRef::new(ApplicationId::new("io.elembra.chat"), "message", message_id)
}

fn chat_read_action() -> ActionCapability {
    ActionCapability::new(CHAT_READ)
}

fn files_read_action() -> ActionCapability {
    ActionCapability::new(FILES_READ)
}

/// A real signed kind-1 (text note) event.
fn signed_note(keys: &Keys, content: &str) -> NostrEvent {
    EventBuilder::text_note(content)
        .sign_with_keys(keys)
        .expect("sign text note")
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

/// Build the relay's signed state entry for a signed event.
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

// ---------------------------------------------------------------------------
// Chat fixtures (direct SQL into the observation index + Memory projection)
// ---------------------------------------------------------------------------

/// The ids a chat tenant setup creates, for the assertions.
struct BuzzEnv {
    principal: PrincipalId,
    community_id: String,
}

/// Insert an active mapping for `community_id` under `tenant` (workspace ==
/// tenant) pointing at `relay_url`, pinned to `relay_pubkey`. Returns the
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

/// Insert an active binding for `pubkey` under an EXPLICIT principal id (so a
/// Files user can double as the Chat principal).
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

/// Full current-state tenant setup: active mapping, active binding + admission
/// for `keys`, and the chat Application enabled with `configuration`.
async fn setup_tenant_with_relay(
    pool: &PgPool,
    tenant: TenantId,
    keys: &Keys,
    community_id: &str,
    relay_url: &str,
    relay_pubkey: Option<&str>,
    configuration: serde_json::Value,
) -> BuzzEnv {
    let mapping_id = insert_mapping(pool, tenant, community_id, relay_url, relay_pubkey).await;
    let pubkey = keys.public_key().to_hex();
    let (principal, binding_id) = insert_binding(pool, tenant, &pubkey).await;
    insert_admission(pool, tenant, mapping_id, binding_id, &pubkey).await;
    enable_chat(pool, tenant, configuration).await;
    BuzzEnv {
        principal,
        community_id: community_id.to_string(),
    }
}

/// [`setup_tenant_with_relay`] binding an EXPLICIT principal id (so a Files
/// user doubles as the Chat principal and the same ctx authorizes both).
#[allow(clippy::too_many_arguments)]
async fn setup_tenant_with_relay_for_principal(
    pool: &PgPool,
    tenant: TenantId,
    principal: PrincipalId,
    keys: &Keys,
    community_id: &str,
    relay_url: &str,
    relay_pubkey: Option<&str>,
    configuration: serde_json::Value,
) -> BuzzEnv {
    let mapping_id = insert_mapping(pool, tenant, community_id, relay_url, relay_pubkey).await;
    let pubkey = keys.public_key().to_hex();
    let binding_id = insert_binding_for_principal(pool, tenant, principal, &pubkey).await;
    insert_admission(pool, tenant, mapping_id, binding_id, &pubkey).await;
    enable_chat(pool, tenant, configuration).await;
    BuzzEnv {
        principal,
        community_id: community_id.to_string(),
    }
}

/// Insert one `chat_observed_events` row directly via SQL. The row is the
/// bridge's already-verified observation state: `signature_verified` is set
/// and the checksum/signature columns carry placeholders satisfying the NOT
/// NULL constraints. `event_id == message_id` (created-event semantics).
#[allow(clippy::too_many_arguments)]
async fn insert_observation(
    pool: &PgPool,
    tenant: TenantId,
    community_id: &str,
    message_id: &str,
    channel_kind: &str,
    event_type: &str,
    active: bool,
    body: Option<&str>,
) {
    insert_observation_at(
        pool,
        tenant,
        community_id,
        message_id,
        message_id,
        channel_kind,
        event_type,
        active,
        body,
        Utc::now(),
    )
    .await;
}

/// [`insert_observation`] with an explicit `event_id` and `event_created_at`
/// (for tombstone-ordering fixtures).
#[allow(clippy::too_many_arguments)]
async fn insert_observation_at(
    pool: &PgPool,
    tenant: TenantId,
    community_id: &str,
    message_id: &str,
    event_id: &str,
    channel_kind: &str,
    event_type: &str,
    active: bool,
    body: Option<&str>,
    created_at: DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO chat_observed_events
            (tenant_id, workspace_id, event_id, message_id, event_type,
             community_id, channel_id, channel_kind, author_pubkey,
             event_created_at, observed_at, checksum, signature,
             signature_verified, body, active)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, now(),
                 $11, $12, true, $13, $14)",
    )
    .bind(tenant.0)
    .bind(tenant.0)
    .bind(event_id)
    .bind(message_id)
    .bind(event_type)
    .bind(community_id)
    .bind(CHANNEL_ID)
    .bind(channel_kind)
    .bind(hex64(0xbb))
    .bind(created_at)
    .bind(format!("sha256:{event_id}"))
    .bind("c".repeat(128))
    .bind(body)
    .bind(active)
    .execute(pool)
    .await
    .unwrap();
}

/// Build a Memory catalog record mirroring a created workspace message with
/// its body indexed (`IndexingStatus::ContentStored`).
fn catalog_record(
    tenant: TenantId,
    community_id: &str,
    message_id: &str,
    body: &str,
    occurred_at: DateTime<Utc>,
) -> MemoryCatalogRecord {
    MemoryCatalogRecord {
        record_id: Uuid::new_v4(),
        tenant_id: tenant,
        workspace_id: WorkspaceId(tenant.0),
        source_application: SOURCE_APPLICATION.to_string(),
        source_type: SOURCE_TYPE_MESSAGE.to_string(),
        source_ref: MemoryCatalogRecord::source_ref_for(message_id),
        message_id: message_id.to_string(),
        latest_event_id: message_id.to_string(),
        event_type: ObservedEventType::Created,
        community_id: community_id.to_string(),
        channel_id: CHANNEL_ID.to_string(),
        channel_kind: ChatChannelKind::Workspace,
        author_pubkey: hex64(0xbb),
        author_principal_id: None,
        occurred_at,
        observed_at: occurred_at,
        checksum: format!("sha256:{message_id}"),
        signature: "c".repeat(128),
        signature_verified: true,
        provenance: Vec::new(),
        classification: DEFAULT_CLASSIFICATION.to_string(),
        retention_policy_ref: None,
        legal_hold_ref: None,
        authorization_source: AUTHORIZATION_SOURCE_BUZZ.to_string(),
        authorization_ref: MemoryCatalogRecord::authorization_ref_for(community_id, &hex64(0xbb)),
        content_indexing: true,
        content: Some(body.to_string()),
        indexing_status: IndexingStatus::ContentStored,
        tombstoned_at: None,
        created_at: occurred_at,
        updated_at: occurred_at,
    }
}

/// Insert a searchable chat message WITHOUT the outbox machinery: one active
/// created observation row plus the Memory catalog projection.
async fn insert_chat_message(
    pool: &PgPool,
    catalog: &MemoryCatalogStore,
    tenant: TenantId,
    community_id: &str,
    message_id: &str,
    body: &str,
    occurred_at: DateTime<Utc>,
) {
    insert_observation_at(
        pool,
        tenant,
        community_id,
        message_id,
        message_id,
        "workspace",
        "created",
        true,
        Some(body),
        occurred_at,
    )
    .await;
    catalog
        .upsert_records(&[catalog_record(
            tenant,
            community_id,
            message_id,
            body,
            occurred_at,
        )])
        .await
        .unwrap();
}

// ---------------------------------------------------------------------------
// Files fixtures
// ---------------------------------------------------------------------------

/// Create a `users` row for `tenant` and return the user.
async fn create_user(metadata_store: &MetadataStore, username: &str, tenant_id: Uuid) -> User {
    let unique_username = format!("{}_{}", username, Uuid::new_v4());
    let user = User::new(
        unique_username.clone(),
        format!("{} Display", username),
        "test_password_hash".to_string(),
        format!("{}@test.local", unique_username),
        false,
        10_737_418_240, // 10GB
        tenant_id,
    );
    metadata_store
        .create_user(&user)
        .await
        .expect("Failed to create test user");
    user
}

/// Create a file row (tenant-scoped) + object-storage blob so
/// `FilesResourceOwner::fetch` returns the content.
async fn create_file_with_content(
    harness: &TestHarness,
    owner_id: Uuid,
    name: &str,
    path: &str,
    content: &[u8],
) -> rustshare_core::domain::File {
    let content_hash = hex::encode(Sha256::digest(content));
    let file = rustshare_core::domain::File::new(
        name.to_string(),
        path.to_string(),
        content_hash,
        content.len() as i64,
        "text/plain".to_string(),
        None,
        owner_id,
        harness.tenant,
    );
    harness
        .object_store
        .put(&file.storage_key(), Bytes::copy_from_slice(content))
        .await
        .expect("Failed to store test file blob");
    harness
        .metadata_store
        .create_file(&file)
        .await
        .expect("Failed to create test file");
    file
}

/// Create a user-to-user share of a file, mirroring what the share
/// repository's `create_user_share` persists.
async fn share_file_to_user(
    harness: &TestHarness,
    file_id: Uuid,
    owner_id: Uuid,
    recipient_id: Uuid,
    permissions: SharePermissions,
) -> Share {
    let share = Share {
        id: Uuid::new_v4(),
        file_id: Some(file_id),
        folder_id: None,
        share_token: None,
        permissions,
        password_hash: None,
        expires_at: None,
        upload_only: false,
        access_count: 0,
        recipient_user_id: Some(recipient_id),
        recipient_group_id: None,
        created_by: owner_id,
        created_at: Utc::now(),
        revoked_at: None,
        tenant_id: harness.tenant,
    };
    harness
        .metadata_store
        .create_share(&share)
        .await
        .expect("create user share");
    share
}

/// A private ACL projection for an owner-indexed note.
fn owner_acl(tenant: Uuid, file_id: Uuid, owner_id: Uuid) -> IndexAclProjection {
    IndexAclProjection {
        tenant_id: tenant,
        workspace_id: tenant,
        object_id: file_id,
        source_folder_id: None,
        owner_id,
        read_principals: vec![IndexPrincipal::Owner(owner_id)],
        visibility: IndexVisibility::Private,
        acl_hash: "test-acl-hash".to_string(),
        acl_version: 1,
        embedding_policy: EmbeddingPolicy::Allowed,
    }
}

/// Insert a `note_index_chunks` row directly via SQL (the DB-level index
/// fixture; the effective candidate for AI-enabled tests lives in the
/// in-memory store seeded with the same ACL/content).
async fn insert_note_index_row(
    pool: &PgPool,
    tenant: Uuid,
    file: &rustshare_core::domain::File,
    owner_id: Uuid,
    content: &str,
    read_acl: &[String],
) {
    let embedding = format!("[{}]", vec!["0"; 768].join(","));
    sqlx::query(
        "INSERT INTO note_index_chunks
            (id, tenant_id, workspace_id, note_id, source_file_id, file_name, file_path,
             content, mime_type, owner_id, embedding, acl_hash, acl_version, read_acl,
             visibility, embedding_policy, indexed_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11::vector, $12, $13, $14,
                 $15, $16, now())",
    )
    .bind(Uuid::new_v4())
    .bind(tenant)
    .bind(tenant)
    .bind(file.id)
    .bind(file.id)
    .bind(&file.name)
    .bind(&file.path)
    .bind(content)
    .bind(&file.mime_type)
    .bind(owner_id)
    .bind(&embedding)
    .bind("")
    .bind(1i64)
    .bind(read_acl.to_vec())
    .bind("private")
    .bind("allowed")
    .execute(pool)
    .await
    .unwrap();
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// The stores + authorizer + service wiring shared by every test. Some stores
/// exist for completeness of the harness contract (the owner adapters receive
/// their own handles at construction) and are intentionally not re-read.
#[allow(dead_code)]
struct TestHarness {
    pool: PgPool,
    tenant: Uuid,
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
    permission_resolver: Arc<PermissionResolver<PermissionResolverRepository>>,
    permission_repo: Arc<PermissionResolverRepository>,
    memory_catalog_store: Arc<MemoryCatalogStore>,
    chat_identity_store: ChatIdentityStore,
    chat_observation_store: ChatObservationStore,
    authorizer: Arc<SourceAuthorizer>,
    /// Present when the test double is wired as the chat authority.
    scripted: Option<ScriptedAuthority>,
    /// Present for AI-enabled tests (note-index keyword/vector candidates).
    indexer: Option<Arc<ContentIndexer<SimpleEmbeddingGenerator>>>,
    ai: Option<Arc<AppAiService>>,
}

impl TestHarness {
    /// A fresh `UnifiedSearchService` over this harness's stores.
    fn service(&self) -> UnifiedSearchService {
        UnifiedSearchService::new(
            Arc::clone(&self.authorizer),
            Arc::clone(&self.metadata_store),
            self.ai.clone(),
            Arc::clone(&self.memory_catalog_store),
        )
    }
}

/// Build the metadata/object/permission stores over `pool`.
async fn build_stores(
    pool: PgPool,
) -> (
    Arc<MetadataStore>,
    Arc<ObjectStore>,
    Arc<PermissionResolver<PermissionResolverRepository>>,
    Arc<PermissionResolverRepository>,
) {
    let metadata_store = Arc::new(MetadataStore::new(pool.clone()));
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
    let permission_repo = Arc::new(PermissionResolverRepository::new(pool.clone()));
    let permission_resolver = Arc::new(PermissionResolver::new(Arc::clone(&permission_repo)));
    (
        metadata_store,
        object_store,
        permission_resolver,
        permission_repo,
    )
}

/// Build a `SourceAuthorizer` with the Files owner AND the Chat owner wired to
/// `chat_authority`, both registered against the canonical first-party
/// ApplicationRegistry.
async fn build_authorizer(
    pool: PgPool,
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
    permission_resolver: Arc<PermissionResolver<PermissionResolverRepository>>,
    permission_repo: Arc<PermissionResolverRepository>,
    chat_authority: Box<dyn rustshare_resource_auth::BuzzAuthority>,
) -> SourceAuthorizer {
    let registry = ApplicationRegistry::first_party().expect("first-party manifests are valid");
    let mut owners = ResourceOwnerRegistry::new();
    owners
        .register(
            Arc::new(FilesResourceOwner::new(
                Arc::clone(&permission_resolver),
                Arc::clone(&permission_repo),
                Arc::clone(&metadata_store),
                Arc::clone(&object_store),
            )),
            &registry,
        )
        .expect("the io.elembra.files owner registers against the canonical registry");
    owners
        .register(
            Arc::new(ChatResourceOwner::with_authority(
                ChatIdentityStore::new(pool.clone()),
                ChatObservationStore::new(pool.clone()),
                chat_authority,
            )),
            &registry,
        )
        .expect("the io.elembra.chat owner registers against the canonical registry");
    SourceAuthorizer::new(owners)
}

/// The shared harness constructor: Chat owner wired to an in-process
/// [`ScriptedAuthority`] (fallback `Allow`; flip per message mid-test).
async fn harness_with_scripted_authority(pool: PgPool, with_ai: bool) -> TestHarness {
    let (metadata_store, object_store, permission_resolver, permission_repo) =
        build_stores(pool.clone()).await;
    let chat_identity_store = ChatIdentityStore::new(pool.clone());
    let chat_observation_store = ChatObservationStore::new(pool.clone());
    let memory_catalog_store = Arc::new(MemoryCatalogStore::new(pool.clone()));
    let scripted = ScriptedAuthority::new(ScriptedOutcome::Allow);
    let (indexer, ai) = if with_ai {
        let generator = Arc::new(SimpleEmbeddingGenerator::new());
        let store = Arc::new(InMemoryVectorStore::new());
        let indexer = Arc::new(ContentIndexer::new(generator, store));
        let ai = Arc::new(AiService::new(
            Arc::clone(&indexer),
            Arc::clone(&permission_resolver),
        ));
        (Some(indexer), Some(ai))
    } else {
        (None, None)
    };
    let authorizer = Arc::new(
        build_authorizer(
            pool.clone(),
            Arc::clone(&metadata_store),
            Arc::clone(&object_store),
            Arc::clone(&permission_resolver),
            Arc::clone(&permission_repo),
            Box::new(scripted.clone()),
        )
        .await,
    );
    TestHarness {
        pool,
        tenant: Uuid::new_v4(),
        metadata_store,
        object_store,
        permission_resolver,
        permission_repo,
        memory_catalog_store,
        chat_identity_store,
        chat_observation_store,
        authorizer,
        scripted: Some(scripted),
        indexer,
        ai,
    }
}

/// The full-stack harness constructor: Chat owner wired to the REAL
/// [`BuzzGatewayAuthority`] pointing at an in-test fake HTTP relay.
async fn harness_with_real_gateway(
    pool: PgPool,
    gateway: Arc<BuzzGatewayClient>,
    with_ai: bool,
) -> TestHarness {
    let (metadata_store, object_store, permission_resolver, permission_repo) =
        build_stores(pool.clone()).await;
    let chat_identity_store = ChatIdentityStore::new(pool.clone());
    let chat_observation_store = ChatObservationStore::new(pool.clone());
    let memory_catalog_store = Arc::new(MemoryCatalogStore::new(pool.clone()));
    let (indexer, ai) = if with_ai {
        let generator = Arc::new(SimpleEmbeddingGenerator::new());
        let store = Arc::new(InMemoryVectorStore::new());
        let indexer = Arc::new(ContentIndexer::new(generator, store));
        let ai = Arc::new(AiService::new(
            Arc::clone(&indexer),
            Arc::clone(&permission_resolver),
        ));
        (Some(indexer), Some(ai))
    } else {
        (None, None)
    };
    let authorizer = Arc::new(
        build_authorizer(
            pool.clone(),
            Arc::clone(&metadata_store),
            Arc::clone(&object_store),
            Arc::clone(&permission_resolver),
            Arc::clone(&permission_repo),
            Box::new(BuzzGatewayAuthority(gateway)),
        )
        .await,
    );
    TestHarness {
        pool,
        tenant: Uuid::new_v4(),
        metadata_store,
        object_store,
        permission_resolver,
        permission_repo,
        memory_catalog_store,
        chat_identity_store,
        chat_observation_store,
        authorizer,
        scripted: None,
        indexer,
        ai,
    }
}

// ---------------------------------------------------------------------------
// Cleanup and response rendering
// ---------------------------------------------------------------------------

/// Remove every row the tests create for `tenant_id` — the chat tables, the
/// chat Application enablement, the note index, shares, files, folders and
/// users (FK-safe order).
async fn cleanup(pool: &PgPool, tenant_id: TenantId) {
    for table in [
        "memory_catalog",
        "chat_observed_events",
        "chat_buzz_admissions",
        "chat_workspace_communities",
        "chat_identity_bindings",
        "application_enablements",
        "note_index_chunks",
        "shares",
        "files",
        "folders",
        "users",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE tenant_id = $1"))
            .bind(tenant_id.0)
            .execute(pool)
            .await
            .unwrap();
    }
}

/// Tenant-scoped row counts for the tables a search can touch (used to prove
/// the search never writes).
async fn tenant_table_counts(pool: &PgPool, tenant_id: TenantId) -> Vec<(String, i64)> {
    let mut counts = Vec::new();
    for table in [
        "memory_catalog",
        "chat_observed_events",
        "chat_identity_bindings",
        "chat_buzz_admissions",
        "chat_workspace_communities",
        "application_enablements",
        "note_index_chunks",
        "shares",
        "files",
        "users",
    ] {
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*)::bigint FROM {table} WHERE tenant_id = $1"
        ))
        .bind(tenant_id.0)
        .fetch_one(pool)
        .await
        .unwrap();
        counts.push((table.to_string(), count));
    }
    counts
}

/// Render a response's user-visible surface (ref, title, location, snippet,
/// provenance) into one string, for marker-leak assertions.
fn render_response(response: &UnifiedSearchResponse) -> String {
    let mut out = String::new();
    for result in &response.results {
        out.push_str(&result.resource_ref);
        out.push('\n');
        out.push_str(&result.title);
        out.push('\n');
        if let Some(location) = &result.location {
            out.push_str(location);
            out.push('\n');
        }
        if let Some(snippet) = &result.snippet {
            out.push_str(snippet);
            out.push('\n');
        }
        out.push_str(&serde_json::to_string(&result.provenance).unwrap_or_default());
        out.push('\n');
    }
    out
}

// ---------------------------------------------------------------------------
// 1. One query returns Files AND Chat results, both authorized
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn one_query_returns_files_and_chat_results() {
    let _guard = SERIAL.lock().await;
    let harness = harness_with_scripted_authority(pool().await, false).await;
    let pool = harness.pool.clone();
    let tenant = TenantId::from(harness.tenant);
    cleanup(&pool, tenant).await;

    let owner = create_user(&harness.metadata_store, "owner", tenant.0).await;
    let file = create_file_with_content(
        &harness,
        owner.id,
        "quarterly-plan.md",
        "/docs/quarterly-plan.md",
        b"the quarterly plan was approved by the board",
    )
    .await;

    let community = format!("community-{}", Uuid::new_v4());
    let keys = Keys::generate();
    let env = setup_tenant_with_relay_for_principal(
        &pool,
        tenant,
        PrincipalId::from(owner.id),
        &keys,
        &community,
        "wss://relay.example.test",
        None,
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;
    let msg_id = message_id(1);
    insert_chat_message(
        &pool,
        &harness.memory_catalog_store,
        tenant,
        &env.community_id,
        &msg_id,
        "the quarterly plan was approved",
        Utc::now(),
    )
    .await;

    let ctx = user_ctx(env.principal, tenant);
    let response = harness
        .service()
        .search(
            &ctx,
            "quarterly",
            &[SearchSource::Files, SearchSource::Chat],
            50,
        )
        .await
        .expect("the search succeeds");

    let files: Vec<_> = response
        .results
        .iter()
        .filter(|r| r.source_application == "io.elembra.files")
        .collect();
    let chats: Vec<_> = response
        .results
        .iter()
        .filter(|r| r.source_application == "io.elembra.chat")
        .collect();
    assert_eq!(files.len(), 1, "exactly one Files result: {response:?}");
    assert_eq!(chats.len(), 1, "exactly one Chat result: {response:?}");

    assert_eq!(
        files[0].resource_ref,
        format!("elembra://io.elembra.files/file/{}", file.id),
        "the Files result carries the canonical resource ref"
    );
    let file_snippet = files[0]
        .snippet
        .as_deref()
        .expect("the authorized object content must produce a snippet");
    assert!(
        file_snippet.contains("quarterly"),
        "the Files snippet comes from the AUTHORIZED object content, got: {file_snippet}"
    );

    assert_eq!(
        chats[0].resource_ref,
        format!("elembra://io.elembra.chat/message/{msg_id}"),
        "the Chat result carries the canonical resource ref"
    );
    let chat_snippet = chats[0]
        .snippet
        .as_deref()
        .expect("the authorized body must produce a snippet");
    assert!(
        chat_snippet.contains("quarterly"),
        "the Chat snippet comes from the authorized body, got: {chat_snippet}"
    );
    assert_eq!(
        chats[0].provenance.message_id.as_deref(),
        Some(msg_id.as_str()),
        "chat provenance carries the message id"
    );
    assert!(
        chats[0]
            .provenance
            .community_id
            .as_deref()
            .is_some_and(|c| c == env.community_id),
        "chat provenance carries the community id"
    );
    assert_eq!(
        chats[0].provenance.channel_id.as_deref(),
        Some(CHANNEL_ID),
        "chat provenance carries the channel id"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 2. Cross-tenant candidates never appear
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn cross_tenant_candidates_never_appear() {
    let _guard = SERIAL.lock().await;
    let harness = harness_with_scripted_authority(pool().await, false).await;
    let pool = harness.pool.clone();
    let tenant_a = TenantId::from(harness.tenant);
    let tenant_b = TenantId::from(Uuid::new_v4());
    cleanup(&pool, tenant_a).await;
    cleanup(&pool, tenant_b).await;

    // Tenant A: its own file + chat matching "confidential".
    let owner_a = create_user(&harness.metadata_store, "owner_a", tenant_a.0).await;
    let file_a = create_file_with_content(
        &harness,
        owner_a.id,
        "a-confidential.md",
        "/a/a-confidential.md",
        b"confidential a stuff",
    )
    .await;
    let keys_a = Keys::generate();
    let env_a = setup_tenant_with_relay_for_principal(
        &pool,
        tenant_a,
        PrincipalId::from(owner_a.id),
        &keys_a,
        &format!("community-{}", Uuid::new_v4()),
        "wss://relay.example.test",
        None,
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;
    let msg_a = message_id(10);
    insert_chat_message(
        &pool,
        &harness.memory_catalog_store,
        tenant_a,
        &env_a.community_id,
        &msg_a,
        "confidential a chat",
        Utc::now(),
    )
    .await;

    // Tenant B: its own file + chat matching "confidential".
    let owner_b = create_user(&harness.metadata_store, "owner_b", tenant_b.0).await;
    let file_b = create_file_with_content(
        &harness,
        owner_b.id,
        "b-confidential.md",
        "/b/b-confidential.md",
        b"confidential b stuff",
    )
    .await;
    let keys_b = Keys::generate();
    let env_b = setup_tenant_with_relay_for_principal(
        &pool,
        tenant_b,
        PrincipalId::from(owner_b.id),
        &keys_b,
        &format!("community-{}", Uuid::new_v4()),
        "wss://relay.example.test",
        None,
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;
    let msg_b = message_id(11);
    insert_chat_message(
        &pool,
        &harness.memory_catalog_store,
        tenant_b,
        &env_b.community_id,
        &msg_b,
        "confidential b chat",
        Utc::now(),
    )
    .await;

    let ctx_a = user_ctx(env_a.principal, tenant_a);
    let response = harness
        .service()
        .search(
            &ctx_a,
            "confidential",
            &[SearchSource::Files, SearchSource::Chat],
            50,
        )
        .await
        .expect("the search succeeds");
    assert_eq!(
        response.results.len(),
        2,
        "tenant A sees exactly its own file and chat results: {response:?}"
    );
    let rendered = render_response(&response);
    assert!(
        rendered.contains(&file_a.id.to_string()),
        "tenant A's own file appears"
    );
    assert!(
        rendered.contains(&msg_a),
        "tenant A's own chat message appears"
    );
    assert!(
        !rendered.contains(&file_b.id.to_string()),
        "tenant B's file candidate must never surface in A's search"
    );
    assert!(
        !rendered.contains(&msg_b),
        "tenant B's chat candidate must never surface in A's search"
    );

    cleanup(&pool, tenant_a).await;
    cleanup(&pool, tenant_b).await;
}

// ---------------------------------------------------------------------------
// 3. Revoking a Files share removes the result immediately
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn revoked_files_share_disappears() {
    let _guard = SERIAL.lock().await;
    let harness = harness_with_scripted_authority(pool().await, false).await;
    let pool = harness.pool.clone();
    let tenant = TenantId::from(harness.tenant);
    cleanup(&pool, tenant).await;

    let owner = create_user(&harness.metadata_store, "owner", tenant.0).await;
    let recipient = create_user(&harness.metadata_store, "recipient", tenant.0).await;
    let file = create_file_with_content(
        &harness,
        owner.id,
        "shared-plan.md",
        "/docs/shared-plan.md",
        b"shared plan contents",
    )
    .await;
    let share = share_file_to_user(
        &harness,
        file.id,
        owner.id,
        recipient.id,
        SharePermissions::View,
    )
    .await;

    let owner_ctx = user_ctx(PrincipalId::from(owner.id), tenant);
    let recipient_ctx = user_ctx(PrincipalId::from(recipient.id), tenant);
    let service = harness.service();

    let before = service
        .search(
            &recipient_ctx,
            "shared",
            &[SearchSource::Files, SearchSource::Chat],
            50,
        )
        .await
        .expect("the search succeeds");
    assert_eq!(
        before.results.len(),
        1,
        "the recipient sees the shared file while the share is active"
    );
    assert_eq!(
        before.results[0].resource_ref,
        format!("elembra://io.elembra.files/file/{}", file.id),
        "the shared file is the result"
    );

    harness
        .metadata_store
        .revoke_share(share.id, owner.id)
        .await
        .expect("revoke the share");

    let after = service
        .search(
            &recipient_ctx,
            "shared",
            &[SearchSource::Files, SearchSource::Chat],
            50,
        )
        .await
        .expect("the search succeeds");
    assert!(
        after.results.is_empty(),
        "the revoked recipient must no longer see the file: {after:?}"
    );

    let owner_result = service
        .search(
            &owner_ctx,
            "shared",
            &[SearchSource::Files, SearchSource::Chat],
            50,
        )
        .await
        .expect("the search succeeds");
    assert_eq!(
        owner_result.results.len(),
        1,
        "the owner still sees their own file"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 4. A stale Files ACL hint can never leak content
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn stale_files_acl_cannot_leak_content() {
    let _guard = SERIAL.lock().await;
    let harness = harness_with_scripted_authority(pool().await, true).await;
    let pool = harness.pool.clone();
    let tenant = TenantId::from(harness.tenant);
    cleanup(&pool, tenant).await;

    let owner = create_user(&harness.metadata_store, "owner", tenant.0).await;
    let stranger = create_user(&harness.metadata_store, "stranger", tenant.0).await;
    let file = create_file_with_content(
        &harness,
        owner.id,
        "plan.md",
        "/docs/plan.md",
        b"the board approved the plan",
    )
    .await;

    // The STALE index hint: the in-memory store is seeded with an ACL that
    // grants `user:<stranger>` (the effective stale candidate the AI service
    // sees) and a physical `note_index_chunks` row documents the DB-level
    // stale state. The current Files state has NO share to the stranger.
    let stale_acl = IndexAclProjection {
        tenant_id: tenant.0,
        workspace_id: tenant.0,
        object_id: file.id,
        source_folder_id: None,
        owner_id: owner.id,
        read_principals: vec![IndexPrincipal::User(stranger.id)],
        visibility: IndexVisibility::Private,
        acl_hash: "stale-acl".to_string(),
        acl_version: 1,
        embedding_policy: EmbeddingPolicy::Allowed,
    };
    harness
        .indexer
        .as_ref()
        .expect("AI harness")
        .index_note(
            file.id,
            file.name.clone(),
            file.path.clone(),
            "STALE-SECRET-MARKER appears only in the stale index".to_string(),
            file.mime_type.clone(),
            owner.id,
            stale_acl,
        )
        .await
        .expect("index the stale note");
    insert_note_index_row(
        &pool,
        tenant.0,
        &file,
        owner.id,
        "STALE-SECRET-MARKER appears only in the stale index",
        &[format!("user:{}", stranger.id)],
    )
    .await;

    // The SourceAuthorizer itself denies the stranger on this file.
    assert_eq!(
        harness
            .authorizer
            .authorize(
                &user_ctx(PrincipalId::from(stranger.id), tenant),
                &files_read_action(),
                &file_ref(file.id),
            )
            .await,
        Decision::Deny,
        "no share ⇒ the Files source denies the stranger"
    );

    let stranger_ctx = user_ctx(PrincipalId::from(stranger.id), tenant);
    let response = harness
        .service()
        .search(
            &stranger_ctx,
            "STALE-SECRET-MARKER",
            &[SearchSource::Files, SearchSource::Chat],
            50,
        )
        .await
        .expect("the search succeeds");
    assert!(
        response.results.is_empty(),
        "the stale index hint must not produce a result: {response:?}"
    );
    let rendered = render_response(&response);
    assert!(
        !rendered.contains("STALE-SECRET-MARKER"),
        "the stale marker must never reach the response"
    );
    assert!(
        !rendered.contains("plan.md"),
        "the denied file's name must never appear"
    );
    assert!(
        !rendered.contains("the board approved the plan"),
        "the denied file's content must never appear"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 5. Revoking Chat membership at Buzz removes the result
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn revoked_chat_membership_removes_result() {
    let _guard = SERIAL.lock().await;
    let harness = harness_with_scripted_authority(pool().await, false).await;
    let pool = harness.pool.clone();
    let tenant = TenantId::from(harness.tenant);
    cleanup(&pool, tenant).await;

    let owner = create_user(&harness.metadata_store, "owner", tenant.0).await;
    let file = create_file_with_content(
        &harness,
        owner.id,
        "alpha.md",
        "/docs/alpha.md",
        b"alpha file content",
    )
    .await;
    let keys = Keys::generate();
    let env = setup_tenant_with_relay_for_principal(
        &pool,
        tenant,
        PrincipalId::from(owner.id),
        &keys,
        &format!("community-{}", Uuid::new_v4()),
        "wss://relay.example.test",
        None,
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;
    let msg_id = message_id(20);
    insert_chat_message(
        &pool,
        &harness.memory_catalog_store,
        tenant,
        &env.community_id,
        &msg_id,
        "alpha discussion",
        Utc::now(),
    )
    .await;
    let scripted = harness.scripted.as_ref().expect("scripted authority");
    scripted.set(&msg_id, ScriptedOutcome::Allow);
    scripted.set_fallback(ScriptedOutcome::Deny);

    let ctx = user_ctx(env.principal, tenant);
    let service = harness.service();
    let first = service
        .search(
            &ctx,
            "alpha",
            &[SearchSource::Files, SearchSource::Chat],
            50,
        )
        .await
        .expect("the search succeeds");
    assert_eq!(
        first.results.len(),
        2,
        "both the file and the chat message appear while membership is active"
    );
    assert!(
        first
            .results
            .iter()
            .any(|r| r.resource_ref == format!("elembra://io.elembra.chat/message/{msg_id}")),
        "the chat result is present"
    );

    // Membership revoked at Buzz: the SAME message id now denies.
    scripted.set(&msg_id, ScriptedOutcome::Deny);

    let second = service
        .search(
            &ctx,
            "alpha",
            &[SearchSource::Files, SearchSource::Chat],
            50,
        )
        .await
        .expect("the search succeeds");
    assert_eq!(
        second.results.len(),
        1,
        "the chat result is gone after revocation, Files results still appear: {second:?}"
    );
    assert!(
        second
            .results
            .iter()
            .all(|r| r.source_application == "io.elembra.files"),
        "only Files results remain"
    );
    assert!(
        second
            .results
            .iter()
            .any(|r| r.resource_ref == format!("elembra://io.elembra.files/file/{}", file.id)),
        "the file result survives the chat revocation"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 6. Stale Chat Memory can never override Buzz
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn stale_chat_memory_cannot_override_buzz() {
    let _guard = SERIAL.lock().await;
    let harness = harness_with_scripted_authority(pool().await, false).await;
    let pool = harness.pool.clone();
    let tenant = TenantId::from(harness.tenant);
    cleanup(&pool, tenant).await;

    let owner = create_user(&harness.metadata_store, "owner", tenant.0).await;
    let file = create_file_with_content(
        &harness,
        owner.id,
        "confidential-notes.md",
        "/docs/confidential-notes.md",
        b"confidential notes for the team",
    )
    .await;
    let keys = Keys::generate();
    let env = setup_tenant_with_relay_for_principal(
        &pool,
        tenant,
        PrincipalId::from(owner.id),
        &keys,
        &format!("community-{}", Uuid::new_v4()),
        "wss://relay.example.test",
        None,
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;
    let msg_id = message_id(30);
    // The record EXISTS with content and the observation row is ACTIVE — but
    // the Buzz authority says Deny for the message.
    insert_chat_message(
        &pool,
        &harness.memory_catalog_store,
        tenant,
        &env.community_id,
        &msg_id,
        "confidential briefing material",
        Utc::now(),
    )
    .await;
    harness
        .scripted
        .as_ref()
        .expect("scripted authority")
        .set_fallback(ScriptedOutcome::Deny);

    let ctx = user_ctx(env.principal, tenant);
    let response = harness
        .service()
        .search(
            &ctx,
            "confidential",
            &[SearchSource::Files, SearchSource::Chat],
            50,
        )
        .await
        .expect("the search succeeds");
    assert_eq!(
        response.results.len(),
        1,
        "only the Files result appears: {response:?}"
    );
    assert_eq!(
        response.results[0].source_application, "io.elembra.files",
        "the sole result is the file"
    );
    assert_eq!(
        response.results[0].resource_ref,
        format!("elembra://io.elembra.files/file/{}", file.id),
        "the surviving result is the own file"
    );
    assert!(
        response
            .results
            .iter()
            .all(|r| r.source_application != "io.elembra.chat"),
        "Memory never authorizes: the live record with content must not surface"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 7. Deleted / tombstoned Chat content never appears
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn deleted_or_tombstoned_chat_content_does_not_appear() {
    let _guard = SERIAL.lock().await;
    let harness = harness_with_scripted_authority(pool().await, false).await;
    let pool = harness.pool.clone();
    let tenant = TenantId::from(harness.tenant);
    cleanup(&pool, tenant).await;

    let keys = Keys::generate();
    let env = setup_tenant_with_relay(
        &pool,
        tenant,
        &keys,
        &format!("community-{}", Uuid::new_v4()),
        "wss://relay.example.test",
        None,
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;
    let scripted = harness.scripted.as_ref().expect("scripted authority");
    scripted.set_fallback(ScriptedOutcome::Allow);

    // (a) A TOMBSTONED catalog record is not even a candidate.
    let doomed_id = message_id(40);
    insert_observation(
        &pool,
        tenant,
        &env.community_id,
        &doomed_id,
        "workspace",
        "created",
        true,
        Some("deleted message content"),
    )
    .await;
    let mut tombstoned = catalog_record(
        tenant,
        &env.community_id,
        &doomed_id,
        "deleted message content",
        Utc::now(),
    );
    tombstoned.indexing_status = IndexingStatus::Tombstoned;
    tombstoned.tombstoned_at = Some(Utc::now());
    harness
        .memory_catalog_store
        .upsert_records(&[tombstoned])
        .await
        .unwrap();

    let resp_a = harness
        .service()
        .search(
            &user_ctx(env.principal, tenant),
            "deleted",
            &[SearchSource::Files, SearchSource::Chat],
            50,
        )
        .await
        .expect("the search succeeds");
    assert!(
        resp_a.results.is_empty(),
        "a tombstoned record must not surface, even with an active observation row: {resp_a:?}"
    );

    // (b) A LIVE record + an active created observation PLUS a Deleted
    // observation at-or-after the create: the gate returns NotFound, so the
    // candidate is dropped despite the live Memory record.
    let revoked_id = message_id(41);
    let base = Utc::now() - Duration::seconds(60);
    insert_observation_at(
        &pool,
        tenant,
        &env.community_id,
        &revoked_id,
        &revoked_id,
        "workspace",
        "created",
        true,
        Some("doomed content"),
        base,
    )
    .await;
    insert_observation_at(
        &pool,
        tenant,
        &env.community_id,
        &revoked_id,
        &hex64(0x01),
        "workspace",
        "deleted",
        false,
        None,
        base + Duration::seconds(10),
    )
    .await;
    harness
        .memory_catalog_store
        .upsert_records(&[catalog_record(
            tenant,
            &env.community_id,
            &revoked_id,
            "doomed content",
            base,
        )])
        .await
        .unwrap();

    let ctx = user_ctx(env.principal, tenant);
    assert_eq!(
        harness
            .authorizer
            .authorize(&ctx, &chat_read_action(), &chat_ref(&revoked_id))
            .await,
        Decision::NotFound,
        "the gate must report the deleted message as absent"
    );
    let resp_b = harness
        .service()
        .search(
            &ctx,
            "doomed",
            &[SearchSource::Files, SearchSource::Chat],
            50,
        )
        .await
        .expect("the search succeeds");
    assert!(
        resp_b.results.is_empty(),
        "a message deleted at-or-after the create must not surface: {resp_b:?}"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 8. Unauthorized snippets never enter the response
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn unauthorized_snippets_never_enter_response() {
    let _guard = SERIAL.lock().await;
    let harness = harness_with_scripted_authority(pool().await, true).await;
    let pool = harness.pool.clone();
    let tenant = TenantId::from(harness.tenant);
    cleanup(&pool, tenant).await;

    let owner = create_user(&harness.metadata_store, "owner", tenant.0).await;
    let file = create_file_with_content(
        &harness,
        owner.id,
        "notes.md",
        "/notes/notes.md",
        b"notes on the project BLOB-MARKER-ONLY approved by the board",
    )
    .await;
    // The index for the ALLOWED file carries a marker that exists ONLY in the
    // stale index, never in the object blob.
    harness
        .indexer
        .as_ref()
        .expect("AI harness")
        .index_note(
            file.id,
            file.name.clone(),
            file.path.clone(),
            "STALE-INDEX-MARKER appears only in the index".to_string(),
            file.mime_type.clone(),
            owner.id,
            owner_acl(tenant.0, file.id, owner.id),
        )
        .await
        .expect("index the allowed note");
    insert_note_index_row(
        &pool,
        tenant.0,
        &file,
        owner.id,
        "STALE-INDEX-MARKER appears only in the index",
        &[format!("owner:{}", owner.id)],
    )
    .await;

    // A Chat message whose record content is TOP-SECRET, denied by the
    // authority (its observation body is innocuous — the leak would have to
    // come from the Memory record).
    let keys = Keys::generate();
    let env = setup_tenant_with_relay_for_principal(
        &pool,
        tenant,
        PrincipalId::from(owner.id),
        &keys,
        &format!("community-{}", Uuid::new_v4()),
        "wss://relay.example.test",
        None,
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;
    let msg_id = message_id(50);
    insert_chat_message(
        &pool,
        &harness.memory_catalog_store,
        tenant,
        &env.community_id,
        &msg_id,
        "TOP-SECRET classified chat message",
        Utc::now(),
    )
    .await;
    let scripted = harness.scripted.as_ref().expect("scripted authority");
    scripted.set(&msg_id, ScriptedOutcome::Deny);

    let ctx = user_ctx(env.principal, tenant);
    let service = harness.service();

    // (a) The denied Chat candidate: searching for its marker returns nothing
    // and the marker never enters the response.
    let denied = service
        .search(
            &ctx,
            "TOP-SECRET",
            &[SearchSource::Files, SearchSource::Chat],
            50,
        )
        .await
        .expect("the search succeeds");
    assert!(
        denied.results.is_empty(),
        "the denied chat candidate must not produce a result: {denied:?}"
    );
    assert!(
        !render_response(&denied).contains("TOP-SECRET"),
        "denied index/record content must never enter the response"
    );

    // (b) The ALLOWED file: the snippet is built from the object-storage
    // content (BLOB marker), never from the stale index (INDEX marker).
    let allowed = service
        .search(
            &ctx,
            "notes",
            &[SearchSource::Files, SearchSource::Chat],
            50,
        )
        .await
        .expect("the search succeeds");
    assert_eq!(
        allowed.results.len(),
        1,
        "exactly the file result: {allowed:?}"
    );
    let snippet = allowed.results[0]
        .snippet
        .as_deref()
        .expect("the authorized file fetch produces a snippet");
    assert!(
        snippet.contains("BLOB-MARKER-ONLY"),
        "the snippet comes from the authorized object content, got: {snippet}"
    );
    let rendered = render_response(&allowed);
    assert!(
        !rendered.contains("STALE-INDEX-MARKER"),
        "the stale index marker must never appear in the response"
    );
    assert!(
        !rendered.contains("TOP-SECRET"),
        "the denied chat content must never appear in the response"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 9. One source's authorization failure never corrupts the other results
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn one_source_failure_does_not_corrupt_other_results() {
    // A Chat authority failure (transport `Error`, which the Chat owner maps
    // to `Deny` — fail closed) must only drop the Chat candidates: the
    // request succeeds and the Files results are intact. The service's
    // `authorize_batch` Err→drop-chunk branch is a defensive guard for
    // oversized batches (unreachable through this surface — the service
    // chunks at `MAX_BATCH_SIZE`); the per-candidate deny path is what a real
    // source outage exercises.
    let _guard = SERIAL.lock().await;
    let harness = harness_with_scripted_authority(pool().await, false).await;
    let pool = harness.pool.clone();
    let tenant = TenantId::from(harness.tenant);
    cleanup(&pool, tenant).await;

    let owner = create_user(&harness.metadata_store, "owner", tenant.0).await;
    let file = create_file_with_content(
        &harness,
        owner.id,
        "alpha.md",
        "/docs/alpha.md",
        b"alpha file content",
    )
    .await;
    let keys = Keys::generate();
    let env = setup_tenant_with_relay_for_principal(
        &pool,
        tenant,
        PrincipalId::from(owner.id),
        &keys,
        &format!("community-{}", Uuid::new_v4()),
        "wss://relay.example.test",
        None,
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;
    let msg_id = message_id(60);
    insert_chat_message(
        &pool,
        &harness.memory_catalog_store,
        tenant,
        &env.community_id,
        &msg_id,
        "alpha chat message",
        Utc::now(),
    )
    .await;
    // Every chat authority check fails with a transport error: the Chat
    // source is unavailable, so Chat candidates fail closed.
    harness
        .scripted
        .as_ref()
        .expect("scripted authority")
        .set_fallback(ScriptedOutcome::Error);

    let ctx = user_ctx(env.principal, tenant);
    let response = harness
        .service()
        .search(
            &ctx,
            "alpha",
            &[SearchSource::Files, SearchSource::Chat],
            50,
        )
        .await
        .expect("a broken Chat source must never fail the whole search");
    assert_eq!(
        response.results.len(),
        1,
        "only the Files result survives: {response:?}"
    );
    assert_eq!(
        response.results[0].source_application, "io.elembra.files",
        "the surviving result is the file"
    );
    assert_eq!(
        response.results[0].resource_ref,
        format!("elembra://io.elembra.files/file/{}", file.id),
        "the file result is intact"
    );
    assert!(
        response
            .results
            .iter()
            .all(|r| r.source_application != "io.elembra.chat"),
        "the failed Chat candidates are dropped, never leaked"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 10. Duplicate candidates collapse deterministically
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn duplicate_candidates_collapse_deterministically() {
    let _guard = SERIAL.lock().await;
    let harness = harness_with_scripted_authority(pool().await, true).await;
    let pool = harness.pool.clone();
    let tenant = TenantId::from(harness.tenant);
    cleanup(&pool, tenant).await;

    let owner = create_user(&harness.metadata_store, "owner", tenant.0).await;
    // The file's NAME matches the query AND the note index's CONTENT matches
    // it: name/path + keyword + vector candidates all target the same ref.
    let file = create_file_with_content(
        &harness,
        owner.id,
        "quarterly-plan.md",
        "/docs/quarterly-plan.md",
        b"the quarterly plan was approved",
    )
    .await;
    harness
        .indexer
        .as_ref()
        .expect("AI harness")
        .index_note(
            file.id,
            file.name.clone(),
            file.path.clone(),
            "the quarterly plan was approved".to_string(),
            file.mime_type.clone(),
            owner.id,
            owner_acl(tenant.0, file.id, owner.id),
        )
        .await
        .expect("index the note");

    let ctx = user_ctx(PrincipalId::from(owner.id), tenant);
    let service = harness.service();
    let first = service
        .search(
            &ctx,
            "quarterly",
            &[SearchSource::Files, SearchSource::Chat],
            50,
        )
        .await
        .expect("the search succeeds");
    assert_eq!(
        first.results.len(),
        1,
        "name and note-content matches must dedupe to one Files result: {first:?}"
    );
    assert_eq!(
        first.results[0].source_application, "io.elembra.files",
        "the deduped result is the file"
    );
    assert_eq!(
        first.results[0].resource_ref,
        format!("elembra://io.elembra.files/file/{}", file.id),
        "the canonical ref is the file ref"
    );

    let second = service
        .search(
            &ctx,
            "quarterly",
            &[SearchSource::Files, SearchSource::Chat],
            50,
        )
        .await
        .expect("the search succeeds");
    assert_eq!(
        format!("{first:?}"),
        format!("{second:?}"),
        "two identical searches must produce identical result lists"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 11. Keyword search works without embeddings
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn keyword_search_works_without_embeddings() {
    let _guard = SERIAL.lock().await;
    // `ai = None`: only Files name/path + Chat Memory candidates exist.
    let harness = harness_with_scripted_authority(pool().await, false).await;
    let pool = harness.pool.clone();
    let tenant = TenantId::from(harness.tenant);
    cleanup(&pool, tenant).await;

    let owner = create_user(&harness.metadata_store, "owner", tenant.0).await;
    // Name/path match.
    let named = create_file_with_content(
        &harness,
        owner.id,
        "budget.md",
        "/docs/budget.md",
        b"budget overview",
    )
    .await;
    // Content-only match (invisible without embeddings: no name/path hit).
    let content_only = create_file_with_content(
        &harness,
        owner.id,
        "operations.md",
        "/docs/operations.md",
        b"the secret budget figure is 42",
    )
    .await;
    let keys = Keys::generate();
    let env = setup_tenant_with_relay_for_principal(
        &pool,
        tenant,
        PrincipalId::from(owner.id),
        &keys,
        &format!("community-{}", Uuid::new_v4()),
        "wss://relay.example.test",
        None,
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;
    let msg_id = message_id(70);
    insert_chat_message(
        &pool,
        &harness.memory_catalog_store,
        tenant,
        &env.community_id,
        &msg_id,
        "budget discussion",
        Utc::now(),
    )
    .await;

    let ctx = user_ctx(env.principal, tenant);
    let response = harness
        .service()
        .search(
            &ctx,
            "budget",
            &[SearchSource::Files, SearchSource::Chat],
            50,
        )
        .await
        .expect("the request succeeds");
    assert_eq!(
        response.results.len(),
        2,
        "the named file and the chat message appear without embeddings: {response:?}"
    );
    let rendered = render_response(&response);
    assert!(
        rendered.contains(&named.id.to_string()),
        "the name/path-matched file appears"
    );
    assert!(rendered.contains(&msg_id), "the chat message appears");
    assert!(
        !rendered.contains(&content_only.id.to_string()),
        "a content-only match (no name/path hit) yields nothing without embeddings"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 12. Ranking is deterministic and matches the documented order
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn ranking_is_deterministic() {
    let _guard = SERIAL.lock().await;
    let harness = harness_with_scripted_authority(pool().await, false).await;
    let pool = harness.pool.clone();
    let tenant = TenantId::from(harness.tenant);
    cleanup(&pool, tenant).await;

    let owner = create_user(&harness.metadata_store, "owner", tenant.0).await;
    // name_path_score: exact-name 1.0, name-prefix 0.9, substring 0.6.
    let alpha =
        create_file_with_content(&harness, owner.id, "alpha", "/docs/alpha", b"alpha file").await;
    let alphabet = create_file_with_content(
        &harness,
        owner.id,
        "alphabet.md",
        "/docs/alphabet.md",
        b"alphabet file",
    )
    .await;
    let team_alpha = create_file_with_content(
        &harness,
        owner.id,
        "team-alpha.md",
        "/docs/team-alpha.md",
        b"team alpha file",
    )
    .await;
    // Chat candidates score 0.8 on content match; occurred_at breaks the tie.
    let keys = Keys::generate();
    let env = setup_tenant_with_relay_for_principal(
        &pool,
        tenant,
        PrincipalId::from(owner.id),
        &keys,
        &format!("community-{}", Uuid::new_v4()),
        "wss://relay.example.test",
        None,
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;
    let msg_older = message_id(80);
    let msg_newer = message_id(81);
    insert_chat_message(
        &pool,
        &harness.memory_catalog_store,
        tenant,
        &env.community_id,
        &msg_older,
        "alpha one",
        Utc::now() - Duration::seconds(60),
    )
    .await;
    insert_chat_message(
        &pool,
        &harness.memory_catalog_store,
        tenant,
        &env.community_id,
        &msg_newer,
        "alpha two",
        Utc::now() - Duration::seconds(30),
    )
    .await;

    let ctx = user_ctx(env.principal, tenant);
    let service = harness.service();
    let first = service
        .search(
            &ctx,
            "alpha",
            &[SearchSource::Files, SearchSource::Chat],
            50,
        )
        .await
        .expect("the search succeeds");
    let refs: Vec<&str> = first
        .results
        .iter()
        .map(|r| r.resource_ref.as_str())
        .collect();
    assert_eq!(
        refs,
        vec![
            format!("elembra://io.elembra.files/file/{}", alpha.id),
            format!("elembra://io.elembra.files/file/{}", alphabet.id),
            format!("elembra://io.elembra.chat/message/{msg_newer}"),
            format!("elembra://io.elembra.chat/message/{msg_older}"),
            format!("elembra://io.elembra.files/file/{}", team_alpha.id),
        ],
        "order: score desc, then occurred_at desc, then source application, then ref"
    );

    let second = service
        .search(
            &ctx,
            "alpha",
            &[SearchSource::Files, SearchSource::Chat],
            50,
        )
        .await
        .expect("the search succeeds");
    assert_eq!(
        format!("{first:?}"),
        format!("{second:?}"),
        "two runs of the identical query must rank identically"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 13. The citation open path reauthorizes on every fetch
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn citation_open_path_reauthorizes() {
    let _guard = SERIAL.lock().await;
    let harness = harness_with_scripted_authority(pool().await, false).await;
    let pool = harness.pool.clone();
    let tenant = TenantId::from(harness.tenant);
    cleanup(&pool, tenant).await;

    // Chat half: principal is a channel member while the authority allows.
    let keys = Keys::generate();
    let env = setup_tenant_with_relay(
        &pool,
        tenant,
        &keys,
        &format!("community-{}", Uuid::new_v4()),
        "wss://relay.example.test",
        None,
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;
    let msg_id = message_id(90);
    insert_chat_message(
        &pool,
        &harness.memory_catalog_store,
        tenant,
        &env.community_id,
        &msg_id,
        "cited chat message",
        Utc::now(),
    )
    .await;
    let scripted = harness.scripted.as_ref().expect("scripted authority");
    scripted.set(&msg_id, ScriptedOutcome::Allow);
    scripted.set_fallback(ScriptedOutcome::Deny);
    let chat_ctx = user_ctx(env.principal, tenant);

    let service = harness.service();
    let chat_response = service
        .search(
            &chat_ctx,
            "cited",
            &[SearchSource::Files, SearchSource::Chat],
            50,
        )
        .await
        .expect("the search succeeds");
    let chat_result = chat_response
        .results
        .iter()
        .find(|r| r.source_application == "io.elembra.chat")
        .expect("the chat result is present");
    let chat_ref_uri = ResourceRef::from_uri(&chat_result.resource_ref)
        .expect("the returned URI parses back into a ref");
    assert_eq!(chat_ref_uri, chat_ref(&msg_id), "the URI round-trips");

    let fetched_chat = harness
        .authorizer
        .fetch(&chat_ctx, &chat_ref_uri, Representation::Text)
        .await
        .expect("while allowed, the returned ref fetches");
    assert_eq!(
        fetched_chat.data.as_ref(),
        b"cited chat message",
        "the authorized fetch returns the body"
    );

    // Membership revoked: the SAME ref must no longer fetch.
    scripted.set(&msg_id, ScriptedOutcome::Deny);
    assert!(
        matches!(
            harness
                .authorizer
                .fetch(&chat_ctx, &chat_ref_uri, Representation::Text)
                .await,
            Err(SourceError::NotFound)
        ),
        "after revocation the citation ref must fail closed (existence-hiding)"
    );

    // Files half: a shared file fetched through its returned ref.
    let owner = create_user(&harness.metadata_store, "owner", tenant.0).await;
    let recipient = create_user(&harness.metadata_store, "recipient", tenant.0).await;
    let file = create_file_with_content(
        &harness,
        owner.id,
        "cited.md",
        "/docs/cited.md",
        b"cited file content",
    )
    .await;
    let share = share_file_to_user(
        &harness,
        file.id,
        owner.id,
        recipient.id,
        SharePermissions::View,
    )
    .await;
    let recipient_ctx = user_ctx(PrincipalId::from(recipient.id), tenant);

    let files_response = service
        .search(
            &recipient_ctx,
            "cited",
            &[SearchSource::Files, SearchSource::Chat],
            50,
        )
        .await
        .expect("the search succeeds");
    let file_result = files_response
        .results
        .iter()
        .find(|r| r.source_application == "io.elembra.files")
        .expect("the file result is present");
    let file_ref_uri = ResourceRef::from_uri(&file_result.resource_ref)
        .expect("the returned URI parses back into a ref");
    assert_eq!(file_ref_uri, file_ref(file.id), "the URI round-trips");

    harness
        .authorizer
        .fetch(&recipient_ctx, &file_ref_uri, Representation::Text)
        .await
        .expect("while shared, the returned ref fetches");

    harness
        .metadata_store
        .revoke_share(share.id, owner.id)
        .await
        .expect("revoke the share");
    assert!(
        matches!(
            harness
                .authorizer
                .fetch(&recipient_ctx, &file_ref_uri, Representation::Text)
                .await,
            Err(SourceError::Unauthorized)
        ),
        "after share revocation the citation ref must fail closed"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 14. Chat search has no outbox/receipt dependency, never writes, and the
//     authorization decision comes from the authority
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn chat_search_has_no_outbox_or_write_dependency() {
    // What this proves (and only this): a search needs no outbox/delivery/
    // receipt rows (the chat tables contain only the gate's own rows), the
    // search never writes to any table (row counts unchanged before/after),
    // and the Chat decision came from the authority (asked for the exact
    // message id). The Chat owner legitimately reads `chat_observed_events`
    // for routing/existence and `memory_catalog` supplies candidates only —
    // that boundary is enforced by construction, not by this test.
    let _guard = SERIAL.lock().await;
    let harness = harness_with_scripted_authority(pool().await, false).await;
    let pool = harness.pool.clone();
    let tenant = TenantId::from(harness.tenant);
    cleanup(&pool, tenant).await;

    let owner = create_user(&harness.metadata_store, "owner", tenant.0).await;
    let file = create_file_with_content(
        &harness,
        owner.id,
        "alpha.md",
        "/docs/alpha.md",
        b"alpha file content",
    )
    .await;
    let keys = Keys::generate();
    let env = setup_tenant_with_relay_for_principal(
        &pool,
        tenant,
        PrincipalId::from(owner.id),
        &keys,
        &format!("community-{}", Uuid::new_v4()),
        "wss://relay.example.test",
        None,
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;
    let msg_id = message_id(100);
    insert_chat_message(
        &pool,
        &harness.memory_catalog_store,
        tenant,
        &env.community_id,
        &msg_id,
        "alpha chat message",
        Utc::now(),
    )
    .await;
    let scripted = harness.scripted.as_ref().expect("scripted authority");
    scripted.set(&msg_id, ScriptedOutcome::Allow);

    // (a) The Chat search must work from ONLY the gate's rows: no outbox,
    // delivery, receipt or consumer rows exist for this tenant.
    let outbox: i64 =
        sqlx::query_scalar("SELECT count(*)::bigint FROM integration_outbox WHERE tenant_id = $1")
            .bind(tenant.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(outbox, 0, "the outbox must be untouched by the fixture");
    let deliveries: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM integration_deliveries WHERE tenant_id = $1",
    )
    .bind(tenant.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(deliveries, 0, "no delivery rows may exist");

    let before = tenant_table_counts(&pool, tenant).await;
    let ctx = user_ctx(env.principal, tenant);
    let response = harness
        .service()
        .search(
            &ctx,
            "alpha",
            &[SearchSource::Files, SearchSource::Chat],
            50,
        )
        .await
        .expect("the search succeeds");
    assert_eq!(
        response.results.len(),
        2,
        "chat results appear with only the gate-needed rows present: {response:?}"
    );
    assert!(
        response
            .results
            .iter()
            .any(|r| r.resource_ref == format!("elembra://io.elembra.files/file/{}", file.id)),
        "the file result is present"
    );
    assert!(
        response
            .results
            .iter()
            .any(|r| r.resource_ref == format!("elembra://io.elembra.chat/message/{msg_id}")),
        "the chat result is present"
    );

    // (b) The search never writes: every tenant row count is unchanged.
    let after = tenant_table_counts(&pool, tenant).await;
    assert_eq!(before, after, "the search must not write to any table");

    // (c) The chat authorization decision came from the authority: it was
    // asked about the exact message id from the request.
    let asked = scripted.asked();
    assert!(
        asked.contains(&msg_id),
        "the authority must have been asked about the exact message id, got: {asked:?}"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 15. Full-stack: search → SourceAuthorizer → Chat owner → Buzz gateway →
//     fake relay decision → snippet, with immediate relay revocation
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn full_stack_buzz_gateway_end_to_end() {
    let _guard = SERIAL.lock().await;
    let keys = Keys::generate();
    let community = format!("community-{}", Uuid::new_v4());
    let relay_keys = Keys::generate();
    let service_keys = Keys::generate();
    let fake = start_fake_buzz(relay_keys.clone(), service_keys.public_key().to_hex());
    let relay_url = format!("ws://127.0.0.1:{}", fake.addr.port());
    let relay_pubkey = relay_keys.public_key().to_hex();

    let gateway =
        Arc::new(BuzzGatewayClient::new_for_test(service_keys.clone(), Client::builder()).unwrap());
    let harness = harness_with_real_gateway(pool().await, gateway, false).await;
    let pool = harness.pool.clone();
    let tenant = TenantId::from(harness.tenant);
    cleanup(&pool, tenant).await;

    let env = setup_tenant_with_relay(
        &pool,
        tenant,
        &keys,
        &community,
        &relay_url,
        Some(&relay_pubkey),
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;

    // A signed message: the message id IS the event id, registered at the
    // relay as a signed state entry; the principal is a channel member.
    let event = signed_note(&keys, "quarterly gateway approved");
    let message_id = event.id.to_hex();
    insert_chat_message(
        &pool,
        &harness.memory_catalog_store,
        tenant,
        &env.community_id,
        &message_id,
        "quarterly gateway approved",
        Utc::now(),
    )
    .await;
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

    let ctx = user_ctx(env.principal, tenant);
    let service = harness.service();

    let first = service
        .search(
            &ctx,
            "quarterly",
            &[SearchSource::Files, SearchSource::Chat],
            50,
        )
        .await
        .expect("the search succeeds");
    assert_eq!(
        first.results.len(),
        1,
        "the real gateway path authorizes the chat result: {first:?}"
    );
    assert_eq!(
        first.results[0].source_application, "io.elembra.chat",
        "the result is the chat message"
    );
    assert_eq!(
        first.results[0].resource_ref,
        format!("elembra://io.elembra.chat/message/{message_id}"),
        "the canonical chat ref"
    );
    let snippet = first.results[0]
        .snippet
        .as_deref()
        .expect("the authorized body produces a snippet");
    assert!(
        snippet.contains("quarterly"),
        "the snippet comes from the authorized fetch, got: {snippet}"
    );

    // The decision came from the RELAY over HTTP, not from local state.
    let requests = fake.state.lock().unwrap().access_check_requests.clone();
    assert!(
        requests
            .iter()
            .any(|r| r["request"]["message_id"] == serde_json::json!(message_id)),
        "the gateway must have asked the relay about the exact message, got: {requests:?}"
    );

    // Membership removed at the relay: the very next search reflects it.
    fake.state
        .lock()
        .unwrap()
        .remove_member(CHANNEL_ID, &keys.public_key().to_hex());
    let second = service
        .search(
            &ctx,
            "quarterly",
            &[SearchSource::Files, SearchSource::Chat],
            50,
        )
        .await
        .expect("the search succeeds");
    assert!(
        second.results.is_empty(),
        "relay-side membership removal must remove the chat result immediately: {second:?}"
    );
    let requests_after = fake.state.lock().unwrap().access_check_requests.len();
    assert!(
        requests_after > requests.len(),
        "the second search must again ask the relay (no local caching)"
    );

    fake.stop().await;
    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// Ask Workspace provider boundary
// ---------------------------------------------------------------------------

/// Proves the exact generation input: both owning sources are freshly fetched,
/// stable source IDs are assigned by the server, and the provider sees no
/// candidate snippets or Memory-only content.
#[cfg(feature = "test-recording-provider")]
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn ask_workspace_records_exact_authorized_files_and_chat_context() {
    let _guard = SERIAL.lock().await;
    let harness = harness_with_scripted_authority(pool().await, false).await;
    let pool = harness.pool.clone();
    let tenant = TenantId::from(harness.tenant);
    cleanup(&pool, tenant).await;

    let owner = create_user(&harness.metadata_store, "ask-owner", tenant.0).await;
    let file = create_file_with_content(
        &harness,
        owner.id,
        "ask-plan.md",
        "/docs/ask-plan.md",
        b"FILE-AUTHORIZED-BYTES: What is the plan?",
    )
    .await;
    let keys = Keys::generate();
    let env = setup_tenant_with_relay_for_principal(
        &pool,
        tenant,
        PrincipalId::from(owner.id),
        &keys,
        &format!("community-{}", Uuid::new_v4()),
        "wss://relay.example.test",
        None,
        serde_json::json!({ "memory_projection": true, "content_indexing": true }),
    )
    .await;
    let msg_id = message_id(9001);
    insert_chat_message(
        &pool,
        &harness.memory_catalog_store,
        tenant,
        &env.community_id,
        &msg_id,
        "CHAT-AUTHORIZED-BYTES: What is the plan?",
        Utc::now(),
    )
    .await;

    let provider = Arc::new(RecordingLlmProvider::new(LlmResult {
        answer: "Both sources agree.".into(),
        citations: vec!["src-001".into(), "src-002".into()],
    }));
    let ask = AskWorkspaceService::new(Arc::new(harness.service()), Some(provider.clone()));
    let answer = ask
        .ask(
            &user_ctx(env.principal, tenant),
            "plan",
            &[SearchSource::Files, SearchSource::Chat],
            50,
        )
        .await
        .expect("provider-backed Ask succeeds");
    assert!(answer.grounded);
    assert_eq!(answer.citations.len(), 2);
    assert!(answer.citations.iter().any(|citation| citation
        .resource_ref
        .ends_with(file.id.to_string().as_str())));
    assert!(answer
        .citations
        .iter()
        .any(|citation| citation.resource_ref.ends_with(msg_id.as_str())));

    let calls = provider.calls().await;
    assert_eq!(calls.len(), 1);
    let call = &calls[0];
    assert_eq!(call.system_policy, SYSTEM_POLICY);
    assert_eq!(call.user_question, "plan");
    assert_eq!(call.sources.len(), 2);
    assert_eq!(call.sources[0].source_id, "src-001");
    assert_eq!(call.sources[1].source_id, "src-002");
    let context = call
        .sources
        .iter()
        .map(|source| source.text.as_str())
        .collect::<Vec<_>>();
    assert!(context
        .iter()
        .any(|text| text.contains("FILE-AUTHORIZED-BYTES")));
    assert!(context
        .iter()
        .any(|text| text.contains("CHAT-AUTHORIZED-BYTES")));
    assert!(!context.iter().any(|text| text.contains("UNAUTHORIZED")));

    let note_provider = Arc::new(RecordingLlmProvider::new(LlmResult {
        answer: "The note contains the plan.".into(),
        citations: vec!["src-001".into()],
    }));
    let note_ask =
        AskWorkspaceService::new(Arc::new(harness.service()), Some(note_provider.clone()));
    let note_answer = note_ask
        .ask_scoped(
            &user_ctx(env.principal, tenant),
            "unrelated question",
            &[SearchSource::Files],
            8,
            &SearchScope::Resource(file_ref(file.id)),
        )
        .await
        .expect("exact note Ask succeeds");
    assert!(note_answer.grounded);
    assert_eq!(note_answer.source_count, 1);
    assert_eq!(note_provider.calls().await[0].sources.len(), 1);
    assert!(note_provider.calls().await[0].sources[0]
        .text
        .contains("FILE-AUTHORIZED-BYTES"));

    let folder_id = Uuid::new_v4();
    let outside = create_file_with_content(
        &harness,
        owner.id,
        "ask-outside.md",
        "/ask-outside.md",
        b"OUTSIDE-FOLDER-BYTES",
    )
    .await;
    sqlx::query(
        "INSERT INTO folders (id, name, path, owner_id, tenant_id) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(folder_id)
    .bind("Ask folder")
    .bind("/Ask folder")
    .bind(owner.id)
    .bind(tenant.0)
    .execute(&pool)
    .await
    .expect("create Ask folder");
    sqlx::query("UPDATE files SET parent_folder_id = $1 WHERE id = $2")
        .bind(folder_id)
        .bind(file.id)
        .execute(&pool)
        .await
        .expect("place note in Ask folder");
    let folder_provider = Arc::new(RecordingLlmProvider::new(LlmResult {
        answer: "The folder contains the plan.".into(),
        citations: vec!["src-001".into()],
    }));
    let folder_ask =
        AskWorkspaceService::new(Arc::new(harness.service()), Some(folder_provider.clone()));
    let folder_answer = folder_ask
        .ask_scoped(
            &user_ctx(env.principal, tenant),
            "ask",
            &[SearchSource::Files],
            8,
            &SearchScope::Folder(ResourceRef::new(
                ApplicationId::new("io.elembra.files"),
                "folder",
                folder_id.to_string(),
            )),
        )
        .await
        .expect("folder Ask succeeds");
    assert!(folder_answer.grounded);
    assert_eq!(folder_answer.source_count, 1);
    let folder_calls = folder_provider.calls().await;
    assert_eq!(folder_calls[0].sources.len(), 1);
    assert!(folder_calls[0].sources[0]
        .text
        .contains("FILE-AUTHORIZED-BYTES"));
    assert!(!folder_calls[0].sources[0]
        .text
        .contains("OUTSIDE-FOLDER-BYTES"));
    assert_ne!(file.id, outside.id);

    let child_folder_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO folders (id, name, path, parent_folder_id, owner_id, tenant_id) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(child_folder_id)
    .bind("Nested")
    .bind("/Ask folder/Nested")
    .bind(folder_id)
    .bind(owner.id)
    .bind(tenant.0)
    .execute(&pool)
    .await
    .expect("create nested Ask folder");
    let child_file = create_file_with_content(
        &harness,
        owner.id,
        "ask-child.md",
        "/Ask folder/Nested/ask-child.md",
        b"CHILD-FOLDER-BYTES",
    )
    .await;
    sqlx::query("UPDATE files SET parent_folder_id = $1 WHERE id = $2")
        .bind(child_folder_id)
        .bind(child_file.id)
        .execute(&pool)
        .await
        .expect("place note in nested Ask folder");
    let nested_provider = Arc::new(RecordingLlmProvider::new(LlmResult {
        answer: "The folder tree contains both plans.".into(),
        citations: vec!["src-001".into(), "src-002".into()],
    }));
    let nested_answer =
        AskWorkspaceService::new(Arc::new(harness.service()), Some(nested_provider.clone()))
            .ask_scoped(
                &user_ctx(env.principal, tenant),
                "ask",
                &[SearchSource::Files],
                8,
                &SearchScope::Folder(ResourceRef::new(
                    ApplicationId::new("io.elembra.files"),
                    "folder",
                    folder_id.to_string(),
                )),
            )
            .await
            .expect("nested folder Ask succeeds");
    assert!(nested_answer.grounded);
    assert_eq!(nested_answer.source_count, 2);
    let nested_calls = nested_provider.calls().await;
    let nested_context = &nested_calls[0].sources;
    assert!(nested_context
        .iter()
        .any(|source| source.text.contains("FILE-AUTHORIZED-BYTES")));
    assert!(nested_context
        .iter()
        .any(|source| source.text.contains("CHILD-FOLDER-BYTES")));
    assert!(!nested_context
        .iter()
        .any(|source| source.text.contains("OUTSIDE-FOLDER-BYTES")));

    let channel_provider = Arc::new(RecordingLlmProvider::new(LlmResult {
        answer: "The channel contains the plan.".into(),
        citations: vec!["src-001".into()],
    }));
    let channel_ask =
        AskWorkspaceService::new(Arc::new(harness.service()), Some(channel_provider.clone()));
    let channel_answer = channel_ask
        .ask_scoped(
            &user_ctx(env.principal, tenant),
            "plan",
            &[SearchSource::Chat],
            8,
            &SearchScope::ChatChannel {
                community_id: env.community_id.clone(),
                channel_id: CHANNEL_ID.into(),
            },
        )
        .await
        .expect("channel Ask succeeds");
    assert!(channel_answer.grounded);
    assert_eq!(channel_answer.source_count, 1);
    let channel_calls = channel_provider.calls().await;
    let channel_call = &channel_calls[0];
    assert_eq!(channel_call.sources.len(), 1);
    assert!(channel_call.sources[0]
        .text
        .contains("CHAT-AUTHORIZED-BYTES"));

    harness
        .scripted
        .as_ref()
        .expect("scripted authority")
        .set(&msg_id, ScriptedOutcome::Deny);
    let revoked_channel_provider = Arc::new(RecordingLlmProvider::new(LlmResult {
        answer: "must not be used".into(),
        citations: vec!["src-001".into()],
    }));
    let revoked_channel = AskWorkspaceService::new(
        Arc::new(harness.service()),
        Some(revoked_channel_provider.clone()),
    )
    .ask_scoped(
        &user_ctx(env.principal, tenant),
        "plan",
        &[SearchSource::Chat],
        8,
        &SearchScope::ChatChannel {
            community_id: env.community_id.clone(),
            channel_id: CHANNEL_ID.into(),
        },
    )
    .await
    .expect("revoked channel Ask degrades cleanly");
    assert!(revoked_channel.insufficient_evidence);
    assert!(revoked_channel_provider.calls().await.is_empty());

    sqlx::query("UPDATE files SET deleted_at = $1 WHERE id = $2")
        .bind(Utc::now())
        .bind(file.id)
        .execute(&pool)
        .await
        .expect("tombstone selected note");
    let deleted_note_provider = Arc::new(RecordingLlmProvider::new(LlmResult {
        answer: "must not be used".into(),
        citations: vec!["src-001".into()],
    }));
    let deleted_note = AskWorkspaceService::new(
        Arc::new(harness.service()),
        Some(deleted_note_provider.clone()),
    )
    .ask_scoped(
        &user_ctx(env.principal, tenant),
        "plan",
        &[SearchSource::Files],
        8,
        &SearchScope::Resource(file_ref(file.id)),
    )
    .await
    .expect("deleted note Ask degrades cleanly");
    assert!(deleted_note.insufficient_evidence);
    assert!(deleted_note_provider.calls().await.is_empty());

    cleanup(&pool, tenant).await;
}
