//! DB-backed integration suite: the zero-config Chat bootstrap security
//! matrix (ADR-0036, goal §5 of the implementation plan).
//!
//! The suite proves the 10 security proofs of the bootstrap feature end-to-end
//! against an in-test FAKE relay (the same fake-relay convention as
//! `buzz_authority_gateway_test.rs`, minus NIP-98 verification — the real
//! client under test sends the NIP-98 header; the fake deliberately ignores
//! it):
//!
//! 1.  a mapping is created only for the authenticated tenant's workspace
//!     (service: the row's `tenant_id` == the caller tenant; handler: 403 for
//!     non-admins and for foreign-workspace paths);
//! 2.  an untrusted discovery response (signed by a key different from the
//!     claimed `relay_pubkey`) fails closed with `Discovery` and creates no
//!     row;
//! 3.  a verified discovery stores the relay's pubkey (64 lowercase hex, the
//!     DB CHECK shape);
//! 4.  provisioning is idempotent — second call is `AlreadyConfigured`, one
//!     row, same community;
//! 5.  a cross-tenant community collision fails closed (`CommunityInUse`,
//!     the second tenant has no row);
//! 6.  an existing manual mapping is never overwritten (`AlreadyConfigured`
//!     on match, `CommunityMismatch` on conflict — row byte-identical both
//!     ways);
//! 7.  an unreachable relay leaves Chat safely unconfigured (`Discovery`, no
//!     row);
//! 8.  two workspaces cannot share one community (both unique constraints:
//!     `UNIQUE(tenant_id, community_id)` and the one-active-per-community
//!     partial index);
//! 9.  authorization continues after provisioning — the stored pin round-trips
//!     through `check_access` against the fake with `Allow`/`Deny`, never an
//!     error;
//! 10. no direct Buzz DB access — `scripts/guard-buzz-no-acl.sh` still holds.
//!
//! Handler-level tests drive the real `provision_community_mapping` and
//! `get_community_mapping` handlers over the real admin routes with real JWTs.
//!
//! DB-backed and `#[ignore]`d; run against the dev database (migrations
//! applied, including `20260810000006`) with `--test-threads=1`:
//!
//!   set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true \
//!     cargo test -p rustshare-server --test chat_bootstrap_test -- \
//!       --ignored --test-threads=1
//!
//! Every test takes the shared `SERIAL` guard and cleans up exactly the rows
//! it created under fresh tenants (same convention as the chat-owner and
//! buzz-memory-projection suites). Every test uses a fresh tenant and
//! community id (the active-community mapping index is global).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, LazyLock, Mutex};

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use nostr::{Event as NostrEvent, EventBuilder, JsonUtil, Keys, Kind};
use reqwest::Client;
use rustshare_core::domain::{ApplicationRegistry, TenantId, WorkspaceId};
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
use rustshare_resource_auth::{BuzzChannelKind, BuzzReadDecision, WorkspaceCommunityMapping};
use rustshare_server::authz::ChatResourceOwner;
use rustshare_server::buzz_gateway::{BuzzAccessCheckRequest, BuzzGatewayClient};
use rustshare_server::buzz_observation::BuzzObservationService;
use rustshare_server::config::ChatProvisioningMode;
use rustshare_server::handlers::collab::CollabRooms;
use rustshare_server::middleware::RateLimitConfig;
use rustshare_server::oidc_runtime::OidcRuntimeCache;
use rustshare_server::outbox_dispatcher::OutboxStatus;
use rustshare_server::routes::{admin_routes, chat_integration_routes};
use rustshare_server::services::ask_workspace::AskWorkspaceService;
use rustshare_server::services::chat_bootstrap::{
    ChatBootstrapError, ChatBootstrapService, ProvisionOutcome,
};
use rustshare_server::services::unified_search::UnifiedSearchService;
use rustshare_server::AppState;
use rustshare_storage::repos::ShareNotificationRepoImpl;
use rustshare_storage::{
    ChatIdentityStore, ChatObservationStore, EventStore, MemoryCatalogStore, MetadataStore,
    ObjectStore, OutboxStore,
};
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

/// Serializes the tests within this binary (same convention as the chat-owner
/// and buzz-memory-projection suites).
static SERIAL: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

/// Shared pool over `DATABASE_URL` with the same fallback the storage-layer
/// tests use.
async fn pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    PgPool::connect(&database_url)
        .await
        .expect("failed to connect to the dev database")
}

/// Remove every row the tests create for `tenant_id` (users included — the
/// admin handler tests insert real `users` rows).
async fn cleanup(pool: &PgPool, tenant_id: TenantId) {
    for table in [
        "memory_catalog",
        "chat_observed_events",
        "chat_buzz_admissions",
        "chat_workspace_communities",
        "chat_identity_bindings",
        "application_enablements",
        "integration_outbox",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE tenant_id = $1"))
            .bind(tenant_id.0)
            .execute(pool)
            .await
            .unwrap();
    }
    sqlx::query("DELETE FROM users WHERE tenant_id = $1")
        .bind(tenant_id.0)
        .execute(pool)
        .await
        .unwrap();
}

/// How many mapping rows exist for `tenant`/`workspace`.
async fn mapping_count(pool: &PgPool, tenant: TenantId, workspace: WorkspaceId) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM chat_workspace_communities
         WHERE tenant_id = $1 AND workspace_id = $2",
    )
    .bind(tenant.0)
    .bind(workspace.0)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Insert an active mapping row directly via SQL (e.g. the "another tenant
/// already owns the community" fixture).
async fn insert_mapping_sql(
    pool: &PgPool,
    tenant: TenantId,
    workspace: WorkspaceId,
    community_id: &str,
    relay_url: &str,
    relay_pubkey: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO chat_workspace_communities
            (mapping_id, tenant_id, workspace_id, community_id, relay_url, relay_pubkey, active)
         VALUES ($1, $2, $3, $4, $5, $6, true)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant.0)
    .bind(workspace.0)
    .bind(community_id)
    .bind(relay_url)
    .bind(relay_pubkey)
    .execute(pool)
    .await
    .unwrap();
}

// ---------------------------------------------------------------------------
// Fake relay (in-memory only — no database)
// ---------------------------------------------------------------------------

/// What `GET /api/v1/relay/community` answers (configured per test). The fake
/// does NOT verify NIP-98 — the real client sends it; the fake ignores it.
enum CommunityMode {
    /// Valid: a kind-19030 event signed by the relay key whose pubkey is
    /// claimed in the content, fresh `evaluated_at`.
    Valid { community_id: String },
    /// Signed by a DIFFERENT key than the content's claimed `relay_pubkey`.
    WrongSigner { community_id: String },
    /// Signed correctly but with a stale `evaluated_at` (> 60s old).
    Stale { community_id: String },
    /// A kind-19030 event whose content is not a community identity.
    MalformedContent,
    /// HTTP 500.
    Http500,
}

/// In-memory state of the fake relay. There is deliberately NO database: the
/// only way production code can read it is over the public HTTP endpoints.
struct FakeRelayState {
    /// The relay's signing key; every kind-19030 response is signed with it.
    relay_keys: Keys,
    /// `GET /api/v1/relay/community` response mode.
    community_mode: CommunityMode,
    /// The decision string for `POST /api/v1/relay/access/check`
    /// (`allow`/`deny`/`not_found`), echoing the request verbatim.
    access_decision: String,
    /// How many access-check requests the relay served.
    access_requests: u64,
}

#[derive(Clone)]
struct FakeRelayHandle {
    state: Arc<Mutex<FakeRelayState>>,
    /// `host:port` of the bound listener (also the identity `host` field).
    host: String,
}

/// A running fake relay: address, shared state, the serve task, and a
/// shutdown trigger (dropping the struct closes the trigger and stops the
/// server gracefully).
struct FakeRelayServer {
    addr: SocketAddr,
    state: Arc<Mutex<FakeRelayState>>,
    task: tokio::task::JoinHandle<()>,
    shutdown: tokio::sync::oneshot::Sender<()>,
}

impl FakeRelayServer {
    async fn stop(self) {
        let _ = self.shutdown.send(());
        let _ = self.task.await;
    }
}

/// Bind and serve the fake relay on an ephemeral loopback port.
fn start_fake_relay(relay_keys: Keys) -> FakeRelayServer {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind the fake relay");
    listener
        .set_nonblocking(true)
        .expect("nonblocking listener");
    let listener = tokio::net::TcpListener::from_std(listener).expect("tokio listener");
    let addr = listener.local_addr().expect("fake relay address");
    let state = Arc::new(Mutex::new(FakeRelayState {
        relay_keys,
        community_mode: CommunityMode::Valid {
            community_id: Uuid::new_v4().to_string(),
        },
        access_decision: "allow".to_string(),
        access_requests: 0,
    }));
    let app = Router::new()
        .route("/api/v1/relay/community", get(community_identity))
        .route("/api/v1/relay/access/check", post(access_check))
        .with_state(FakeRelayHandle {
            state: state.clone(),
            host: addr.to_string(),
        });
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let server = axum::serve(listener, app).with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
        });
        if let Err(error) = server.await {
            tracing::error!(%error, "fake relay server failed");
        }
    });
    FakeRelayServer {
        addr,
        state,
        task,
        shutdown: shutdown_tx,
    }
}

/// Sign a kind-19030 response event from the relay key.
fn relay_19030(relay_keys: &Keys, content: serde_json::Value) -> NostrEvent {
    EventBuilder::new(Kind::from(19_030u16), content.to_string())
        .sign_with_keys(relay_keys)
        .expect("sign the relay response")
}

/// The raw JSON of a signed event (the wire shape the client parses).
fn event_json(event: &NostrEvent) -> Value {
    serde_json::from_str(&event.as_json()).expect("relay event serializes")
}

/// `GET /api/v1/relay/community`: answer per the configured `CommunityMode`.
/// NIP-98 is deliberately NOT verified (the fake ignores the header).
async fn community_identity(
    State(handle): State<FakeRelayHandle>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let state = handle.state.lock().unwrap();
    let now = Utc::now().timestamp();
    let event = match &state.community_mode {
        CommunityMode::Valid { community_id } => relay_19030(
            &state.relay_keys,
            serde_json::json!({
                "community_id": community_id,
                "host": handle.host,
                "relay_pubkey": state.relay_keys.public_key().to_hex(),
                "evaluated_at": now,
            }),
        ),
        CommunityMode::WrongSigner { community_id } => {
            // Signed by an attacker key; the content claims the REAL relay
            // pubkey — the client must reject the mismatch.
            let attacker = Keys::generate();
            relay_19030(
                &attacker,
                serde_json::json!({
                    "community_id": community_id,
                    "host": handle.host,
                    "relay_pubkey": state.relay_keys.public_key().to_hex(),
                    "evaluated_at": now,
                }),
            )
        }
        CommunityMode::Stale { community_id } => relay_19030(
            &state.relay_keys,
            serde_json::json!({
                "community_id": community_id,
                "host": handle.host,
                "relay_pubkey": state.relay_keys.public_key().to_hex(),
                "evaluated_at": now - 120,
            }),
        ),
        CommunityMode::MalformedContent => relay_19030(
            &state.relay_keys,
            serde_json::json!("not a community identity"),
        ),
        CommunityMode::Http500 => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "fake relay exploded".to_string(),
            ))
        }
    };
    Ok(Json(event_json(&event)))
}

/// `POST /api/v1/relay/access/check`: ignore NIP-98, echo the request
/// verbatim in a kind-19030 event signed by the relay key, with the configured
/// decision and a fresh `evaluated_at`.
async fn access_check(
    State(handle): State<FakeRelayHandle>,
    body: Body,
) -> Result<Json<Value>, (StatusCode, String)> {
    let body_bytes = axum::body::to_bytes(body, 1024 * 1024)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("cannot read body: {e}")))?;
    let request: BuzzAccessCheckRequest = serde_json::from_slice(&body_bytes).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("invalid check request: {e}"),
        )
    })?;
    let (decision, evaluated_at) = {
        let mut state = handle.state.lock().unwrap();
        state.access_requests += 1;
        (state.access_decision.clone(), Utc::now().timestamp())
    };
    let content = serde_json::json!({
        "decision": decision,
        "reason": "fake relay decision",
        "evaluated_at": evaluated_at,
        "pubkey": request.pubkey,
        "channel_id": request.channel_id,
        "message_id": request.message_id,
    });
    let event = {
        let state = handle.state.lock().unwrap();
        relay_19030(&state.relay_keys, content)
    };
    Ok(Json(event_json(&event)))
}

// ---------------------------------------------------------------------------
// Service harness (real ChatBootstrapService + real ChatIdentityStore + fake)
// ---------------------------------------------------------------------------

struct BootstrapEnv {
    service: Arc<ChatBootstrapService>,
    gateway: Arc<BuzzGatewayClient>,
    store: Arc<ChatIdentityStore>,
    fake: FakeRelayServer,
    relay_url: String,
    relay_pubkey: String,
}

/// A `ChatBootstrapService` over the REAL store, a test-only gateway client
/// (private targets allowed), and a fresh fake relay.
async fn bootstrap_env(pool: PgPool) -> BootstrapEnv {
    let relay_keys = Keys::generate();
    let service_keys = Keys::generate();
    let fake = start_fake_relay(relay_keys.clone());
    let relay_url = format!("ws://127.0.0.1:{}", fake.addr.port());
    let gateway = Arc::new(
        BuzzGatewayClient::new_for_test(service_keys, Client::builder())
            .expect("gateway for the test fake relay"),
    );
    let store = Arc::new(ChatIdentityStore::new(pool.clone()));
    let service = Arc::new(ChatBootstrapService::new(
        gateway.clone(),
        store.clone(),
        relay_url.clone(),
    ));
    BootstrapEnv {
        service,
        gateway,
        store,
        fake,
        relay_url,
        relay_pubkey: relay_keys.public_key().to_hex(),
    }
}

/// A fresh tenant/workspace/community triple, and configure the fake to serve
/// a VALID identity for it.
fn fresh_community(env: &BootstrapEnv) -> (TenantId, WorkspaceId, String) {
    let tenant = TenantId::from(Uuid::new_v4());
    let workspace = WorkspaceId::from(Uuid::new_v4());
    let community = Uuid::new_v4().to_string();
    env.fake.state.lock().unwrap().community_mode = CommunityMode::Valid {
        community_id: community.clone(),
    };
    (tenant, workspace, community)
}

// ---------------------------------------------------------------------------
// Proof 1 — the mapping is created only for the authenticated tenant's
// workspace (service half: the row's tenant_id == the caller tenant)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn bootstrap_maps_only_authenticated_tenant_workspace() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let env = bootstrap_env(pool.clone()).await;
    let (tenant, workspace, community) = fresh_community(&env);

    let outcome = env
        .service
        .provision(tenant, workspace)
        .await
        .expect("provision must succeed against the valid fake identity");
    assert_eq!(
        outcome,
        ProvisionOutcome::Inserted {
            community_id: community.clone(),
            relay_url: env.relay_url.clone(),
            relay_pubkey: env.relay_pubkey.clone(),
        }
    );

    let row = env
        .store
        .mapping(tenant, workspace)
        .await
        .expect("store read must succeed")
        .expect("a mapping row must exist after provisioning");
    assert_eq!(
        row.tenant_id, tenant,
        "the mapping row is owned by the provisioning tenant"
    );
    assert_eq!(row.workspace_id, workspace);
    assert_eq!(row.community_id, community);

    env.fake.stop().await;
    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// Proof 2 — an untrusted discovery response fails closed, no row
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn bootstrap_rejects_untrusted_discovery_response() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let env = bootstrap_env(pool.clone()).await;
    let tenant = TenantId::from(Uuid::new_v4());
    let workspace = WorkspaceId::from(Uuid::new_v4());
    let community = Uuid::new_v4().to_string();
    env.fake.state.lock().unwrap().community_mode = CommunityMode::WrongSigner {
        community_id: community.clone(),
    };

    let result = env.service.provision(tenant, workspace).await;
    assert!(
        matches!(result, Err(ChatBootstrapError::Discovery(_))),
        "a discovery response signed by a different key must fail closed: {result:?}"
    );
    assert!(
        env.store
            .mapping(tenant, workspace)
            .await
            .expect("store read must succeed")
            .is_none(),
        "no mapping row may be created from an untrusted discovery response"
    );

    env.fake.stop().await;
    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// Proof 2b — stale, malformed and erroring discovery responses also fail
// closed (harness modes from plan Step 1)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn bootstrap_rejects_stale_malformed_and_erroring_discovery() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let env = bootstrap_env(pool.clone()).await;
    let tenant = TenantId::from(Uuid::new_v4());
    let workspace = WorkspaceId::from(Uuid::new_v4());

    for mode in [
        CommunityMode::Stale {
            community_id: Uuid::new_v4().to_string(),
        },
        CommunityMode::MalformedContent,
        CommunityMode::Http500,
    ] {
        env.fake.state.lock().unwrap().community_mode = mode;
        let result = env.service.provision(tenant, workspace).await;
        assert!(
            matches!(result, Err(ChatBootstrapError::Discovery(_))),
            "an invalid discovery response must fail closed: {result:?}"
        );
        assert!(
            env.store
                .mapping(tenant, workspace)
                .await
                .expect("store read must succeed")
                .is_none(),
            "no mapping row may be created from an invalid discovery response"
        );
    }

    env.fake.stop().await;
    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// Proof 3 — the verified relay pubkey is pinned (64 lowercase hex)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn bootstrap_pins_verified_relay_pubkey() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let env = bootstrap_env(pool.clone()).await;
    let (tenant, workspace, community) = fresh_community(&env);

    env.service
        .provision(tenant, workspace)
        .await
        .expect("provision must succeed");

    let row = env
        .store
        .mapping(tenant, workspace)
        .await
        .expect("store read must succeed")
        .expect("a mapping row must exist after provisioning");
    assert_eq!(row.community_id, community);
    let pin = row
        .relay_pubkey
        .expect("the verified relay pubkey must be pinned");
    assert_eq!(
        pin, env.relay_pubkey,
        "the stored pin is the fake relay's key"
    );
    assert_eq!(pin.len(), 64, "the pin is 64 hex chars");
    assert!(
        pin.bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')),
        "the pin matches ^[0-9a-f]{{64}}$ (the DB CHECK): {pin:?}"
    );

    env.fake.stop().await;
    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// Proof 4 — provisioning is idempotent
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn bootstrap_is_idempotent() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let env = bootstrap_env(pool.clone()).await;
    let (tenant, workspace, community) = fresh_community(&env);

    let first = env
        .service
        .provision(tenant, workspace)
        .await
        .expect("first provision must succeed");
    assert!(
        matches!(first, ProvisionOutcome::Inserted { .. }),
        "the first call creates the row: {first:?}"
    );

    let second = env
        .service
        .provision(tenant, workspace)
        .await
        .expect("the second provision must succeed, not error");
    assert_eq!(
        second,
        ProvisionOutcome::AlreadyConfigured {
            community_id: community.clone(),
            relay_url: env.relay_url.clone(),
            relay_pubkey: Some(env.relay_pubkey.clone()),
        }
    );

    assert_eq!(
        mapping_count(&pool, tenant, workspace).await,
        1,
        "exactly one mapping row exists after two provisions"
    );
    let row = env
        .store
        .mapping(tenant, workspace)
        .await
        .unwrap()
        .expect("the row still exists");
    assert_eq!(row.community_id, community);

    env.fake.stop().await;
    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// Proof 4b — two CONCURRENT provisions for the same tenant+workspace race to
// exactly one row: one Inserted + one AlreadyConfigured. Exercises the
// `ProvisionMappingOutcome::AlreadyExists` winner re-read path.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn concurrent_provisions_race_to_one_mapping() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let env = bootstrap_env(pool.clone()).await;
    let (tenant, workspace, community) = fresh_community(&env);

    let (first, second) = tokio::join!(
        env.service.provision(tenant, workspace),
        env.service.provision(tenant, workspace),
    );
    let first = first.expect("the first provision must succeed");
    let second = second.expect("the second provision must succeed");
    let inserted = usize::from(matches!(first, ProvisionOutcome::Inserted { .. }))
        + usize::from(matches!(second, ProvisionOutcome::Inserted { .. }));
    let already = usize::from(matches!(first, ProvisionOutcome::AlreadyConfigured { .. }))
        + usize::from(matches!(second, ProvisionOutcome::AlreadyConfigured { .. }));
    assert_eq!(
        inserted, 1,
        "exactly one concurrent call inserts: {first:?} / {second:?}"
    );
    assert_eq!(
        already, 1,
        "the other call reports AlreadyConfigured via the winner re-read: {first:?} / {second:?}"
    );

    assert_eq!(
        mapping_count(&pool, tenant, workspace).await,
        1,
        "exactly one mapping row survives the race"
    );
    let row = env
        .store
        .mapping(tenant, workspace)
        .await
        .expect("store read must succeed")
        .expect("the row still exists");
    assert_eq!(row.community_id, community);

    env.fake.stop().await;
    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// Proof 5 — a cross-tenant community collision fails closed
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn cross_tenant_community_collision_fails_closed() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let env = bootstrap_env(pool.clone()).await;
    let (tenant_a, workspace_a, community) = fresh_community(&env);

    // Tenant A maps community X.
    let first = env
        .service
        .provision(tenant_a, workspace_a)
        .await
        .expect("tenant A's provision must succeed");
    assert!(matches!(first, ProvisionOutcome::Inserted { .. }));

    // Tenant B tries to provision the SAME community X.
    let tenant_b = TenantId::from(Uuid::new_v4());
    let workspace_b = WorkspaceId::from(Uuid::new_v4());
    let result = env.service.provision(tenant_b, workspace_b).await;
    match result {
        Err(ChatBootstrapError::CommunityInUse { community_id }) => {
            assert_eq!(community_id, community, "the conflict names community X");
        }
        other => {
            panic!("tenant B must fail closed when the community is already mapped: {other:?}")
        }
    }
    assert!(
        env.store
            .mapping(tenant_b, workspace_b)
            .await
            .expect("store read must succeed")
            .is_none(),
        "tenant B has no mapping row"
    );

    env.fake.stop().await;
    cleanup(&pool, tenant_a).await;
    cleanup(&pool, tenant_b).await;
}

// ---------------------------------------------------------------------------
// Proof 6 — an existing manual mapping is never overwritten
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn existing_manual_mapping_is_never_overwritten() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let env = bootstrap_env(pool.clone()).await;
    let tenant = TenantId::from(Uuid::new_v4());
    let workspace = WorkspaceId::from(Uuid::new_v4());
    let community_m = Uuid::new_v4().to_string();
    let community_n = Uuid::new_v4().to_string();
    let manual_relay_url = "wss://manual.example.test".to_string();
    let manual_pubkey = "ab".repeat(32);

    // Pre-existing manual mapping: community M, custom relay URL + pin.
    let manual = WorkspaceCommunityMapping {
        tenant_id: tenant,
        workspace_id: workspace,
        community_id: community_m.clone(),
        relay_url: manual_relay_url.clone(),
        relay_pubkey: Some(manual_pubkey.clone()),
        active: true,
    };
    env.store
        .insert_mapping(&manual)
        .await
        .expect("manual mapping insert must succeed");

    // The fake discovers M → AlreadyConfigured, row untouched.
    env.fake.state.lock().unwrap().community_mode = CommunityMode::Valid {
        community_id: community_m.clone(),
    };
    let outcome = env
        .service
        .provision(tenant, workspace)
        .await
        .expect("matching the manual mapping must be an idempotent success");
    assert_eq!(
        outcome,
        ProvisionOutcome::AlreadyConfigured {
            community_id: community_m.clone(),
            relay_url: manual_relay_url.clone(),
            relay_pubkey: Some(manual_pubkey.clone()),
        }
    );
    let row = env
        .store
        .mapping(tenant, workspace)
        .await
        .unwrap()
        .expect("the manual row still exists");
    assert_eq!(row.community_id, community_m);
    assert_eq!(row.relay_url, manual_relay_url, "relay_url is unchanged");
    assert_eq!(
        row.relay_pubkey,
        Some(manual_pubkey.clone()),
        "relay_pubkey is unchanged"
    );

    // The fake discovers N → CommunityMismatch, row still untouched.
    env.fake.state.lock().unwrap().community_mode = CommunityMode::Valid {
        community_id: community_n.clone(),
    };
    let result = env.service.provision(tenant, workspace).await;
    assert!(
        matches!(result, Err(ChatBootstrapError::CommunityMismatch { .. })),
        "a conflicting discovery must fail closed: {result:?}"
    );
    let row = env
        .store
        .mapping(tenant, workspace)
        .await
        .unwrap()
        .expect("the manual row still exists");
    assert_eq!(row.community_id, community_m);
    assert_eq!(row.relay_url, manual_relay_url, "relay_url is unchanged");
    assert_eq!(
        row.relay_pubkey,
        Some(manual_pubkey),
        "relay_pubkey is unchanged"
    );

    env.fake.stop().await;
    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// Proof 7 — an unreachable relay leaves Chat safely unconfigured
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn unreachable_relay_leaves_chat_safely_unconfigured() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    let workspace = WorkspaceId::from(Uuid::new_v4());

    // A free port: bind, record, drop — nothing listens there.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind a probe port");
    let port = listener.local_addr().expect("probe port address").port();
    drop(listener);
    let unreachable_url = format!("ws://127.0.0.1:{port}");

    let gateway = Arc::new(
        BuzzGatewayClient::new_for_test(Keys::generate(), Client::builder())
            .expect("gateway for the test"),
    );
    let store = Arc::new(ChatIdentityStore::new(pool.clone()));
    let service = ChatBootstrapService::new(gateway, store.clone(), unreachable_url);

    let result = service.provision(tenant, workspace).await;
    assert!(
        matches!(result, Err(ChatBootstrapError::Discovery(_))),
        "an unreachable relay must fail closed: {result:?}"
    );
    assert!(
        store
            .mapping(tenant, workspace)
            .await
            .expect("store read must succeed")
            .is_none(),
        "no mapping row may be created from an unreachable relay"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// Proof 8 — two workspaces cannot share one community
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn workspaces_cannot_share_one_community() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let env = bootstrap_env(pool.clone()).await;
    let (tenant_a, workspace_a, community) = fresh_community(&env);

    // Workspace 1 of tenant A maps X.
    let first = env
        .service
        .provision(tenant_a, workspace_a)
        .await
        .expect("workspace 1's provision must succeed");
    assert!(matches!(first, ProvisionOutcome::Inserted { .. }));

    // Workspace 2 of the SAME tenant: UNIQUE(tenant_id, community_id) fires.
    let workspace_b = WorkspaceId::from(Uuid::new_v4());
    let result = env.service.provision(tenant_a, workspace_b).await;
    match result {
        Err(ChatBootstrapError::CommunityInUse { community_id }) => {
            assert_eq!(community_id, community, "the conflict names community X");
        }
        other => {
            panic!("a second workspace of the same tenant must not share the community: {other:?}")
        }
    }

    // A second tenant: the one-active-per-community partial index fires.
    let tenant_b = TenantId::from(Uuid::new_v4());
    let workspace_c = WorkspaceId::from(Uuid::new_v4());
    let result = env.service.provision(tenant_b, workspace_c).await;
    match result {
        Err(ChatBootstrapError::CommunityInUse { community_id }) => {
            assert_eq!(community_id, community, "the conflict names community X");
        }
        other => panic!("a second tenant must not share the community either: {other:?}"),
    }
    assert!(
        env.store
            .mapping(tenant_b, workspace_c)
            .await
            .expect("store read must succeed")
            .is_none(),
        "the second tenant has no mapping row"
    );

    env.fake.stop().await;
    cleanup(&pool, tenant_a).await;
    cleanup(&pool, tenant_b).await;
}

// ---------------------------------------------------------------------------
// Proof 9 — authorization continues after provisioning
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn authorization_continues_after_provisioning() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let env = bootstrap_env(pool.clone()).await;
    let (tenant, workspace, community) = fresh_community(&env);
    let user_pubkey = Keys::generate().public_key().to_hex();

    env.service
        .provision(tenant, workspace)
        .await
        .expect("provision must succeed");

    // The stored mapping carries the discovered community/relay/pin.
    let row = env
        .store
        .mapping(tenant, workspace)
        .await
        .expect("store read must succeed")
        .expect("a mapping row must exist after provisioning");
    assert_eq!(row.community_id, community);
    assert_eq!(row.relay_url, env.relay_url);
    let pin = row.relay_pubkey.expect("the pin is stored");

    // The pinned relay answers an access check with a signed decision — the
    // same signature-verified round-trip the read path uses. Allow…
    env.fake.state.lock().unwrap().access_decision = "allow".to_string();
    let request = BuzzAccessCheckRequest {
        pubkey: user_pubkey.clone(),
        channel_id: "channel-allow".to_string(),
        channel_kind: BuzzChannelKind::Workspace,
        message_id: None,
        event_created_at: None,
    };
    let decision = env
        .gateway
        .check_access(&env.relay_url, &pin, &request)
        .await
        .expect("authorization must continue after provisioning (Allow)");
    assert_eq!(decision, BuzzReadDecision::Allow);

    // …and Deny, still a decision, never an error.
    env.fake.state.lock().unwrap().access_decision = "deny".to_string();
    let decision = env
        .gateway
        .check_access(&env.relay_url, &pin, &request)
        .await
        .expect("authorization must continue after provisioning (Deny)");
    assert_eq!(decision, BuzzReadDecision::Deny);
    assert_eq!(
        env.fake.state.lock().unwrap().access_requests,
        2,
        "both decisions came from real relay round-trips"
    );

    env.fake.stop().await;
    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// Proof 10 — no direct Buzz DB access (structural guard)
// ---------------------------------------------------------------------------

#[test]
#[ignore = "structural guard (no DB)"]
fn no_direct_buzz_db_access() {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/guard-buzz-no-acl.sh");
    let output = Command::new(&script)
        .output()
        .expect("the guard script must run");
    assert!(
        output.status.success(),
        "guard-buzz-no-acl.sh must pass:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

// ---------------------------------------------------------------------------
// Handler-level tests (real routes + real JWTs over the real AppState)
// ---------------------------------------------------------------------------

/// Build the full `AppState` (same service graph the other DB-backed handler
/// suites construct) with the Chat bootstrap service wired in and the
/// provisioning mode set to `Auto`.
#[allow(clippy::too_many_lines)]
async fn setup_app_state(pool: PgPool, bootstrap: Option<Arc<ChatBootstrapService>>) -> AppState {
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
            rustshare_storage::ObjectStoreOptions {
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

    let note_service = Arc::new(rustshare_server::services::note_service::NoteService::new(
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

    let chat_owner = Arc::new(ChatResourceOwner::new(
        chat_identity_store.clone(),
        (*chat_observation_store).clone(),
    ));
    let mut owners = rustshare_resource_auth::ResourceOwnerRegistry::new();
    let chat_owner_registered: Arc<dyn rustshare_resource_auth::ResourceOwner> = chat_owner.clone();
    owners
        .register(
            chat_owner_registered,
            &ApplicationRegistry::first_party().unwrap(),
        )
        .expect("the io.elembra.chat owner registers against the canonical registry");
    let source_authorizer = Arc::new(rustshare_resource_auth::SourceAuthorizer::new(owners));

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

    AppState {
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
        poll_rate_limiter: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
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
        collab_rooms: Arc::new(CollabRooms::new()),
        vault_sync_service,
        chat_integration_service,
        mail_service,
        outbox_store,
        chat_observation_store,
        memory_catalog_store,
        unified_search_service,
        ask_workspace_service,
        buzz_observation_service,
        chat_owner,
        buzz_gateway: None,
        chat_bootstrap: bootstrap,
        chat_provisioning: ChatProvisioningMode::Auto,
        outbox_status: Arc::new(OutboxStatus::default()),
        outbox_worker_enabled: false,
        outbox_readiness_staleness_secs: 60,
        shutdown_tx: tokio::sync::broadcast::channel(1).0,
        prometheus_handle: rustshare_server::metrics::init_metrics(),
    }
}

/// Insert a user with an explicit tenant (the handler's tenant scope checks
/// compare the token claim AND the `users.tenant_id` row).
async fn create_user(pool: &PgPool, tenant_id: Uuid, is_admin: bool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users
            (id, username, email, password_hash, display_name, is_admin, storage_quota, tenant_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(format!("user-{}", Uuid::new_v4().simple()))
    .bind(format!("user-{}@example.test", Uuid::new_v4().simple()))
    .bind("$argon2id$v=19$m=4096,t=3,p=1$placeholder_hash")
    .bind("test user")
    .bind(is_admin)
    .bind(10_737_418_240i64)
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("insert user");
    id
}

/// A bearer token for `user_id` whose `tenant_id` claim matches the row.
fn bearer_token(state: &AppState, user_id: Uuid, tenant_id: Uuid) -> String {
    state
        .jwt_manager
        .generate(user_id, "admin@test.local", tenant_id)
        .expect("generate token")
}

/// One admin provision call over the real routes; returns the HTTP status.
///
/// The router is typed `Router<()>`: `with_state` has already baked the
/// `AppState` into every route, and only `Router<()>` implements
/// `tower::Service` (axum 0.8).
async fn provision_status(router: &Router<()>, token: &str, workspace_id: Uuid) -> StatusCode {
    router
        .clone()
        .oneshot(
            Request::post(format!(
                "/api/v1/admin/applications/chat/workspaces/{workspace_id}/provision"
            ))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

/// One admin provision call over the real routes; returns status + JSON body.
async fn provision_response(
    router: &Router<()>,
    token: &str,
    workspace_id: Uuid,
) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::post(format!(
                "/api/v1/admin/applications/chat/workspaces/{workspace_id}/provision"
            ))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read response body");
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("response body is JSON")
    };
    (status, body)
}

/// One admin get-mapping call over the real routes; returns the HTTP status.
async fn get_mapping_status(router: &Router<()>, token: &str, workspace_id: Uuid) -> StatusCode {
    router
        .clone()
        .oneshot(
            Request::get(format!(
                "/api/v1/admin/applications/chat/workspaces/{workspace_id}/community"
            ))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

/// The provision endpoint requires an admin: a non-admin gets 403 and no
/// mapping row is created.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn provision_endpoint_requires_admin() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let env = bootstrap_env(pool.clone()).await;
    let state = setup_app_state(pool.clone(), Some(env.service.clone())).await;
    let router = chat_integration_routes().with_state(state.clone());

    let tenant = Uuid::new_v4();
    let non_admin = create_user(&pool, tenant, false).await;
    let token = bearer_token(&state, non_admin, tenant);

    let status = provision_status(&router, &token, tenant).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "non-admin provision is 403");
    assert_eq!(
        mapping_count(&pool, TenantId(tenant), WorkspaceId(tenant)).await,
        0,
        "no mapping row may be created by a non-admin"
    );

    env.fake.stop().await;
    cleanup(&pool, TenantId(tenant)).await;
}

/// The provision endpoint rejects a workspace outside the caller's tenant
/// (proof 1 handler half): admin of tenant A provisioning tenant B's
/// workspace id → 403, and no row is created.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn provision_endpoint_rejects_foreign_workspace() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let env = bootstrap_env(pool.clone()).await;
    let state = setup_app_state(pool.clone(), Some(env.service.clone())).await;
    let router = chat_integration_routes().with_state(state.clone());

    let tenant = Uuid::new_v4();
    let admin = create_user(&pool, tenant, true).await;
    let token = bearer_token(&state, admin, tenant);
    let foreign_workspace = Uuid::new_v4();
    assert_ne!(foreign_workspace, tenant);

    let status = provision_status(&router, &token, foreign_workspace).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "provisioning a foreign workspace is 403"
    );
    assert_eq!(
        mapping_count(&pool, TenantId(tenant), WorkspaceId(tenant)).await,
        0,
        "no mapping row may be created for the caller's own workspace either"
    );

    env.fake.stop().await;
    cleanup(&pool, TenantId(tenant)).await;
}

/// The provision endpoint returns 409 when the discovered community is
/// already mapped by another tenant.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn provision_endpoint_returns_409_on_community_in_use() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let env = bootstrap_env(pool.clone()).await;
    let community = Uuid::new_v4().to_string();
    env.fake.state.lock().unwrap().community_mode = CommunityMode::Valid {
        community_id: community.clone(),
    };
    let state = setup_app_state(pool.clone(), Some(env.service.clone())).await;
    let router = chat_integration_routes().with_state(state.clone());

    // Another tenant already owns the community.
    let other_tenant = TenantId::from(Uuid::new_v4());
    insert_mapping_sql(
        &pool,
        other_tenant,
        WorkspaceId(other_tenant.0),
        &community,
        &env.relay_url,
        Some(&env.relay_pubkey),
    )
    .await;

    let tenant = Uuid::new_v4();
    let admin = create_user(&pool, tenant, true).await;
    let token = bearer_token(&state, admin, tenant);

    let (status, body) = provision_response(&router, &token, tenant).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "a community owned by another tenant is a 409 conflict"
    );
    assert!(
        body["error"].as_str().is_some(),
        "the conflict carries an error message: {body}"
    );
    assert_eq!(
        mapping_count(&pool, TenantId(tenant), WorkspaceId(tenant)).await,
        0,
        "the caller gets no mapping row"
    );

    env.fake.stop().await;
    cleanup(&pool, TenantId(tenant)).await;
    cleanup(&pool, other_tenant).await;
}

/// The provision endpoint returns 201 with the discovered identity.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn provision_endpoint_returns_201_with_identity() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let env = bootstrap_env(pool.clone()).await;
    let community = Uuid::new_v4().to_string();
    env.fake.state.lock().unwrap().community_mode = CommunityMode::Valid {
        community_id: community.clone(),
    };
    let state = setup_app_state(pool.clone(), Some(env.service.clone())).await;
    let router = chat_integration_routes().with_state(state.clone());

    let tenant = Uuid::new_v4();
    let admin = create_user(&pool, tenant, true).await;
    let token = bearer_token(&state, admin, tenant);

    let (status, body) = provision_response(&router, &token, tenant).await;
    assert_eq!(status, StatusCode::CREATED, "provision returns 201");
    assert_eq!(body["status"], "created");
    assert_eq!(body["community_id"], community);
    assert_eq!(body["relay_url"], env.relay_url);
    assert_eq!(body["relay_pubkey"], env.relay_pubkey);

    env.fake.stop().await;
    cleanup(&pool, TenantId(tenant)).await;
}

/// The provision endpoint is idempotent: the second call is still 201 with
/// `status == "already_configured"` and exactly one row.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn provision_endpoint_idempotent() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let env = bootstrap_env(pool.clone()).await;
    let community = Uuid::new_v4().to_string();
    env.fake.state.lock().unwrap().community_mode = CommunityMode::Valid {
        community_id: community.clone(),
    };
    let state = setup_app_state(pool.clone(), Some(env.service.clone())).await;
    let router = chat_integration_routes().with_state(state.clone());

    let tenant = Uuid::new_v4();
    let admin = create_user(&pool, tenant, true).await;
    let token = bearer_token(&state, admin, tenant);

    let (status, body) = provision_response(&router, &token, tenant).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["status"], "created");
    assert_eq!(body["community_id"], community);

    let (status, body) = provision_response(&router, &token, tenant).await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "the second provision is still 201"
    );
    assert_eq!(body["status"], "already_configured");
    assert_eq!(body["community_id"], community);
    assert_eq!(
        mapping_count(&pool, TenantId(tenant), WorkspaceId(tenant)).await,
        1,
        "exactly one row after two provision calls"
    );

    env.fake.stop().await;
    cleanup(&pool, TenantId(tenant)).await;
}

/// The get-mapping endpoint returns the pin for an admin after provisioning.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn get_community_mapping_returns_pin_for_admin() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let env = bootstrap_env(pool.clone()).await;
    let community = Uuid::new_v4().to_string();
    env.fake.state.lock().unwrap().community_mode = CommunityMode::Valid {
        community_id: community.clone(),
    };
    let state = setup_app_state(pool.clone(), Some(env.service.clone())).await;
    let router = chat_integration_routes().with_state(state.clone());

    let tenant = Uuid::new_v4();
    let admin = create_user(&pool, tenant, true).await;
    let token = bearer_token(&state, admin, tenant);

    // Provision first, then read the mapping back.
    let (status, _) = provision_response(&router, &token, tenant).await;
    assert_eq!(status, StatusCode::CREATED);

    let response = router
        .clone()
        .oneshot(
            Request::get(format!(
                "/api/v1/admin/applications/chat/workspaces/{tenant}/community"
            ))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read response body");
    let body: Value = serde_json::from_slice(&bytes).expect("response body is JSON");
    assert_eq!(body["community_id"], community);
    assert_eq!(body["relay_url"], env.relay_url);
    assert_eq!(body["relay_pubkey"], env.relay_pubkey);
    assert_eq!(body["active"], true);

    env.fake.stop().await;
    cleanup(&pool, TenantId(tenant)).await;
}

/// The get-mapping endpoint is 404 when the workspace is unconfigured.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn get_community_mapping_404_when_unconfigured() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let env = bootstrap_env(pool.clone()).await;
    let state = setup_app_state(pool.clone(), Some(env.service.clone())).await;
    let router = chat_integration_routes().with_state(state.clone());

    let tenant = Uuid::new_v4();
    let admin = create_user(&pool, tenant, true).await;
    let token = bearer_token(&state, admin, tenant);

    let status = get_mapping_status(&router, &token, tenant).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "an unconfigured workspace is 404"
    );

    env.fake.stop().await;
    cleanup(&pool, TenantId(tenant)).await;
}

/// The get-mapping endpoint requires an admin: non-admin → 403.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn get_community_mapping_requires_admin() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let env = bootstrap_env(pool.clone()).await;
    let state = setup_app_state(pool.clone(), Some(env.service.clone())).await;
    let router = chat_integration_routes().with_state(state.clone());

    let tenant = Uuid::new_v4();
    let non_admin = create_user(&pool, tenant, false).await;
    let token = bearer_token(&state, non_admin, tenant);

    let status = get_mapping_status(&router, &token, tenant).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a non-admin cannot read the mapping"
    );

    env.fake.stop().await;
    cleanup(&pool, TenantId(tenant)).await;
}

// ---------------------------------------------------------------------------
// Enable-hook tests (real admin routes + real AppState): in auto mode the
// enable handler provisions inline after the enable succeeds (ADR-0036 §4).
// ---------------------------------------------------------------------------

/// One admin enable call over the real admin routes; returns the HTTP status.
async fn enable_status(router: &Router<()>, token: &str, key: &str) -> StatusCode {
    router
        .clone()
        .oneshot(
            Request::post(format!("/api/v1/admin/applications/{key}/enable"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

/// Seed the build-time application enablement rows for a fresh tenant (the
/// enable route 404s without them).
async fn seed_applications(state: &AppState, tenant: Uuid) {
    state
        .application_service
        .ensure_default_applications(tenant)
        .await
        .expect("default application enablements must seed");
}

/// Enabling Chat in auto mode provisions inline: 200 and a mapping row for
/// the admin's tenant (community = the fake relay's).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn enable_chat_in_auto_mode_provisions_mapping() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let env = bootstrap_env(pool.clone()).await;
    let community = Uuid::new_v4().to_string();
    env.fake.state.lock().unwrap().community_mode = CommunityMode::Valid {
        community_id: community.clone(),
    };
    let state = setup_app_state(pool.clone(), Some(env.service.clone())).await;
    let router = admin_routes().with_state(state.clone());

    let tenant = Uuid::new_v4();
    let admin = create_user(&pool, tenant, true).await;
    let token = bearer_token(&state, admin, tenant);
    seed_applications(&state, tenant).await;

    let status = enable_status(&router, &token, "io.elembra.chat").await;
    assert_eq!(status, StatusCode::OK, "enable succeeds in auto mode");
    let row = env
        .store
        .mapping(TenantId(tenant), WorkspaceId(tenant))
        .await
        .expect("store read must succeed")
        .expect("enabling Chat in auto mode must create the mapping row");
    assert_eq!(
        row.community_id, community,
        "the mapping is the fake relay's community"
    );

    env.fake.stop().await;
    cleanup(&pool, TenantId(tenant)).await;
}

/// Enabling a NON-chat application in auto mode creates no mapping row.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn enable_non_chat_application_creates_no_mapping() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let env = bootstrap_env(pool.clone()).await;
    let state = setup_app_state(pool.clone(), Some(env.service.clone())).await;
    let router = admin_routes().with_state(state.clone());

    let tenant = Uuid::new_v4();
    let admin = create_user(&pool, tenant, true).await;
    let token = bearer_token(&state, admin, tenant);
    seed_applications(&state, tenant).await;

    let status = enable_status(&router, &token, "io.elembra.notes").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "enabling a non-chat application succeeds"
    );
    assert_eq!(
        mapping_count(&pool, TenantId(tenant), WorkspaceId(tenant)).await,
        0,
        "no mapping row may be created for a non-chat application"
    );

    env.fake.stop().await;
    cleanup(&pool, TenantId(tenant)).await;
}

/// Enabling Chat with an UNREACHABLE relay still succeeds (200) and leaves no
/// mapping row: an auto-provisioning failure is logged, never fatal.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn enable_chat_with_unreachable_relay_leaves_chat_unconfigured() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;

    // A free port with no listener: bind, record, drop.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind a probe port");
    let port = listener.local_addr().expect("probe port address").port();
    drop(listener);
    let gateway = Arc::new(
        BuzzGatewayClient::new_for_test(Keys::generate(), Client::builder())
            .expect("gateway for the test"),
    );
    let store = Arc::new(ChatIdentityStore::new(pool.clone()));
    let bootstrap = Arc::new(ChatBootstrapService::new(
        gateway,
        store,
        format!("ws://127.0.0.1:{port}"),
    ));
    let state = setup_app_state(pool.clone(), Some(bootstrap)).await;
    let router = admin_routes().with_state(state.clone());

    let tenant = Uuid::new_v4();
    let admin = create_user(&pool, tenant, true).await;
    let token = bearer_token(&state, admin, tenant);
    seed_applications(&state, tenant).await;

    let status = enable_status(&router, &token, "io.elembra.chat").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "enable still succeeds when the bootstrap relay is unreachable"
    );
    assert_eq!(
        mapping_count(&pool, TenantId(tenant), WorkspaceId(tenant)).await,
        0,
        "a failed discovery leaves Chat safely unconfigured"
    );

    cleanup(&pool, TenantId(tenant)).await;
}
