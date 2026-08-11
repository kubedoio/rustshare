//! DB-backed authorization tests for the Chat message source-owner adapter
//! ([`rustshare_server::authz::ChatResourceOwner`]).
//!
//! These exercise the `rustshare-resource-auth` contract (`ResourceRef` /
//! `PrincipalContext` / `SourceAuthorizer`) through the real Chat owner
//! adapter. They prove the revocation semantics of the Buzz → Elembra Memory
//! projection: Chat content exposure is gated strictly on CURRENT Chat/Buzz
//! state (active admission/binding + Application enablement), never on stored
//! Memory state, and revocations take effect immediately.
//!
//! DB-backed and `#[ignore]`d; run against the dev database (migrations
//! applied, including `20260810000004`) with `--test-threads=1`:
//!
//!   set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true \
//!     cargo test --test chat_owner_authorization_test -- --ignored --test-threads=1
//!
//! The chat tables are tenant-scoped, so every test takes a shared `SERIAL`
//! guard and cleans up exactly the rows it created (same convention as the
//! chat-observation and memory-projection suites). Every test uses a fresh
//! tenant and community, so the suites never interfere (the active-community
//! mapping index is global).

use chrono::{Duration, Utc};
use nostr::Keys;
use rustshare_core::domain::{
    ActionCapability, ApplicationId, ApplicationRegistry, PrincipalId, TenantId, WorkspaceId,
};
use rustshare_memory::event::ObservedEventType;
use rustshare_resource_auth::{
    Candidate, Decision, PrincipalContext, Purpose, Representation, ResourceOwnerRegistry,
    ResourceRef, SourceAuthorizer, SourceError, CHAT_READ,
};
use rustshare_server::authz::ChatResourceOwner;
use rustshare_storage::{ChatIdentityStore, ChatObservationStore};
use sqlx::PgPool;
use std::sync::{Arc, LazyLock};
use uuid::Uuid;

/// Serializes the tests within this binary (same convention as the outbox and
/// chat-observation suites).
static SERIAL: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

/// The `io.elembra.chat` Application identity.
fn chat_application() -> ApplicationId {
    ApplicationId::new("io.elembra.chat")
}

/// A canonical ref for a Chat message.
fn chat_ref(message_id: &str) -> ResourceRef {
    ResourceRef::new(chat_application(), "message", message_id)
}

fn chat_read_action() -> ActionCapability {
    ActionCapability::new(CHAT_READ)
}

/// A unique 64-lowercase-hex message id (two UUIDs concatenated).
fn unique_hex64() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

/// A fixed 64-lowercase-hex id for placeholder columns.
fn hex64(seed: u8) -> String {
    format!("{seed:02x}").repeat(32)
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
}

/// Build a fresh `SourceAuthorizer` seeded with the Chat owner adapter over
/// `pool`. Registration validates that the canonical first-party manifest
/// declares the `message`/`chat.read` surface.
async fn setup() -> (PgPool, SourceAuthorizer) {
    let pool = pool().await;
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
    (pool, SourceAuthorizer::new(owners))
}

/// A plain human-user principal context. RustShare maps tenant 1:1 to
/// workspace, so the workspace id is the tenant id.
fn user_ctx(principal: PrincipalId, tenant: TenantId) -> PrincipalContext {
    PrincipalContext::user(principal, tenant, WorkspaceId(tenant.0))
}

/// The ids a setup creates, for the assertions.
struct ChatEnv {
    tenant: TenantId,
    principal: PrincipalId,
    message_id: String,
}

/// Insert an active mapping row for `community_id` under `tenant` (workspace
/// == tenant, per the platform invariant). Returns the mapping id.
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

/// Insert an active binding for `principal` + `pubkey` and return the binding
/// id.
async fn insert_binding(
    pool: &PgPool,
    tenant: TenantId,
    principal: PrincipalId,
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
    .bind(principal.0)
    .bind(pubkey)
    .execute(pool)
    .await
    .unwrap();
    binding_id
}

async fn insert_admission(
    pool: &PgPool,
    tenant: TenantId,
    mapping_id: Uuid,
    binding_id: Uuid,
    pubkey: &str,
    active: bool,
) {
    sqlx::query(
        "INSERT INTO chat_buzz_admissions
            (admission_id, tenant_id, mapping_id, binding_id, buzz_pubkey, active)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant.0)
    .bind(mapping_id)
    .bind(binding_id)
    .bind(pubkey)
    .bind(active)
    .execute(pool)
    .await
    .unwrap();
}

/// Enable (or disable) the chat Application for `tenant` with the
/// memory-projection configuration (workspace == tenant).
async fn set_chat_enablement(pool: &PgPool, tenant: TenantId, enabled: bool) {
    sqlx::query(
        "INSERT INTO application_enablements
            (tenant_id, workspace_id, application_id, enabled, configuration)
         VALUES ($1, $2, 'io.elembra.chat', $3, '{\"memory_projection\": true}'::jsonb)
         ON CONFLICT (tenant_id, workspace_id, application_id)
         DO UPDATE SET enabled = EXCLUDED.enabled, updated_at = now()",
    )
    .bind(tenant.0)
    .bind(tenant.0)
    .bind(enabled)
    .execute(pool)
    .await
    .unwrap();
}

/// Insert one `chat_observed_events` row for `message_id` directly via SQL.
///
/// The row is the bridge's already-verified observation state: the owner
/// adapter does NOT re-verify signatures — it trusts the observation index as
/// the bridge's verified state — so `signature_verified` is set and the
/// checksum/signature columns carry placeholders that satisfy the NOT NULL
/// constraints. `event_id == message_id` (created-event semantics; the row is
/// the message's root) and `event_created_at = now()`.
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
/// (for the ordering/tie-break and tombstone-override tests).
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
    created_at: chrono::DateTime<Utc>,
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
    .bind("channel-1")
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

/// Full current-state setup: active mapping + active binding + active
/// admission + chat enablement + one observed row for `message_id` under a
/// fresh tenant and community. Returns the ids the assertions need.
async fn setup_env(
    pool: &PgPool,
    channel_kind: &str,
    event_type: &str,
    active: bool,
    body: Option<&str>,
) -> ChatEnv {
    let tenant = TenantId::from(Uuid::new_v4());
    let keys = Keys::generate();
    let principal = PrincipalId::from(Uuid::new_v4());
    let community_id = format!("community-{}", Uuid::new_v4());
    let message_id = unique_hex64();

    let mapping_id = insert_mapping(pool, tenant, &community_id).await;
    let binding_id = insert_binding(pool, tenant, principal, &keys.public_key().to_hex()).await;
    insert_admission(
        pool,
        tenant,
        mapping_id,
        binding_id,
        &keys.public_key().to_hex(),
        true,
    )
    .await;
    set_chat_enablement(pool, tenant, true).await;
    insert_observation(
        pool,
        tenant,
        &community_id,
        &message_id,
        channel_kind,
        event_type,
        active,
        body,
    )
    .await;

    ChatEnv {
        tenant,
        principal,
        message_id,
    }
}

/// Revoke every active admission for the tenant (as `revoke_principal` would
/// for a binding; a simple `active = false` is sufficient to prove immediate
/// revocation semantics).
async fn revoke_admissions(pool: &PgPool, tenant: TenantId) {
    sqlx::query("UPDATE chat_buzz_admissions SET active = false WHERE tenant_id = $1 AND active")
        .bind(tenant.0)
        .execute(pool)
        .await
        .unwrap();
}

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

// ---------------------------------------------------------------------------
// 1. Fully current state (binding + admission + enablement) allows a
//    workspace-channel message
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn authorize_allows_active_workspace_member() {
    let _guard = SERIAL.lock().await;
    let (pool, authorizer) = setup().await;
    let env = setup_env(&pool, "workspace", "created", true, Some("hello buzz")).await;

    let ctx = user_ctx(env.principal, env.tenant);
    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &chat_ref(&env.message_id))
            .await,
        Decision::Allow,
        "a bound, admitted principal in an enabled tenant may read a workspace message"
    );

    cleanup(&pool, env.tenant).await;
}

// ---------------------------------------------------------------------------
// 2. Revoking the admission blocks exposure immediately
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn revoked_admission_blocks_exposure() {
    let _guard = SERIAL.lock().await;
    let (pool, authorizer) = setup().await;
    let env = setup_env(&pool, "workspace", "created", true, Some("hello buzz")).await;

    let ctx = user_ctx(env.principal, env.tenant);
    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &chat_ref(&env.message_id))
            .await,
        Decision::Allow,
        "active admission allows"
    );

    revoke_admissions(&pool, env.tenant).await;

    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &chat_ref(&env.message_id))
            .await,
        Decision::Deny,
        "a revoked admission must deny immediately, without any Memory state change"
    );

    cleanup(&pool, env.tenant).await;
}

// ---------------------------------------------------------------------------
// 3. Revoking the binding blocks exposure immediately
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn revoked_binding_blocks_exposure() {
    let _guard = SERIAL.lock().await;
    let (pool, authorizer) = setup().await;
    let env = setup_env(&pool, "workspace", "created", true, Some("hello buzz")).await;

    let ctx = user_ctx(env.principal, env.tenant);
    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &chat_ref(&env.message_id))
            .await,
        Decision::Allow,
        "active binding allows"
    );

    revoke_bindings(&pool, env.tenant).await;

    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &chat_ref(&env.message_id))
            .await,
        Decision::Deny,
        "a revoked binding must deny immediately (no active binding => no exposure)"
    );

    cleanup(&pool, env.tenant).await;
}

// ---------------------------------------------------------------------------
// 4. Disabling the chat Application blocks exposure immediately
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn chat_disabled_blocks_exposure() {
    let _guard = SERIAL.lock().await;
    let (pool, authorizer) = setup().await;
    let env = setup_env(&pool, "workspace", "created", true, Some("hello buzz")).await;

    let ctx = user_ctx(env.principal, env.tenant);
    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &chat_ref(&env.message_id))
            .await,
        Decision::Allow,
        "an enabled chat Application allows"
    );

    set_chat_enablement(&pool, env.tenant, false).await;

    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &chat_ref(&env.message_id))
            .await,
        Decision::Deny,
        "a disabled chat Application must deny immediately"
    );

    cleanup(&pool, env.tenant).await;
}

// ---------------------------------------------------------------------------
// 5. Unknown message id -> NotFound (existence-hiding)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn unknown_message_is_not_found() {
    let _guard = SERIAL.lock().await;
    let (pool, authorizer) = setup().await;
    let env = setup_env(&pool, "workspace", "created", true, Some("hello buzz")).await;

    let ctx = user_ctx(env.principal, env.tenant);
    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &chat_ref(&unique_hex64()))
            .await,
        Decision::NotFound,
        "an unknown message must look absent"
    );

    cleanup(&pool, env.tenant).await;
}

// ---------------------------------------------------------------------------
// 6. Tombstoned (deleted) message -> NotFound
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn tombstoned_message_is_not_found() {
    let _guard = SERIAL.lock().await;
    let (pool, authorizer) = setup().await;
    // The latest observed row for the message is a Deleted event (inactive).
    let env = setup_env(&pool, "workspace", "deleted", false, None).await;

    let ctx = user_ctx(env.principal, env.tenant);
    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &chat_ref(&env.message_id))
            .await,
        Decision::NotFound,
        "a tombstoned message must not be exposable"
    );

    cleanup(&pool, env.tenant).await;
}

// ---------------------------------------------------------------------------
// 7. Cross-tenant ref fails closed (tenant-scoped lookup -> NotFound)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn cross_tenant_ref_fails_closed() {
    let _guard = SERIAL.lock().await;
    let (pool, authorizer) = setup().await;
    let env = setup_env(&pool, "workspace", "created", true, Some("hello buzz")).await;

    // Same principal id, different tenant: the lookup is tenant-scoped, so the
    // message row in the other tenant is invisible.
    let foreign = TenantId::from(Uuid::new_v4());
    let ctx = user_ctx(env.principal, foreign);
    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &chat_ref(&env.message_id))
            .await,
        Decision::NotFound,
        "a cross-tenant ref must never be allowed"
    );
    assert!(
        matches!(
            authorizer
                .fetch(&ctx, &chat_ref(&env.message_id), Representation::Raw)
                .await,
            Err(SourceError::NotFound)
        ),
        "cross-tenant fetch must fail closed with the existence-hiding variant"
    );

    cleanup(&pool, env.tenant).await;
}

// ---------------------------------------------------------------------------
// 8. dm/private channels are never candidate-exposable (coarse Deny)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn dm_and_private_channels_are_denied() {
    let _guard = SERIAL.lock().await;
    let (pool, authorizer) = setup().await;
    for channel_kind in ["dm", "private"] {
        let env = setup_env(&pool, channel_kind, "created", true, Some("hello buzz")).await;
        let ctx = user_ctx(env.principal, env.tenant);
        assert_eq!(
            authorizer
                .authorize(&ctx, &chat_read_action(), &chat_ref(&env.message_id))
                .await,
            Decision::Deny,
            "{channel_kind} channel messages must never be candidate-exposable"
        );
        cleanup(&pool, env.tenant).await;
    }
}

// ---------------------------------------------------------------------------
// 9. fetch returns the body only while allowed; revocation makes fetch fail
//    with the existence-hiding variant
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn fetch_returns_body_only_when_allowed() {
    let _guard = SERIAL.lock().await;
    let (pool, authorizer) = setup().await;
    let env = setup_env(&pool, "workspace", "created", true, Some("hello buzz body")).await;
    let ctx = user_ctx(env.principal, env.tenant);
    let reference = chat_ref(&env.message_id);

    let resolved = authorizer
        .resolve(&ctx, &reference, Purpose::RagContext)
        .await
        .expect("active member resolves the message");
    assert_eq!(
        resolved.display_name,
        format!("buzz message {}", &env.message_id[..8])
    );
    assert_eq!(resolved.media_type.as_deref(), Some("text/plain"));
    assert!(
        resolved.available,
        "a stored body must resolve as available"
    );

    let fetched = authorizer
        .fetch(&ctx, &reference, Representation::Raw)
        .await
        .expect("active member fetches the body");
    assert_eq!(fetched.data.as_ref(), b"hello buzz body");
    assert_eq!(fetched.media_type.as_deref(), Some("text/plain"));
    assert_eq!(fetched.size, Some(15));

    revoke_admissions(&pool, env.tenant).await;

    assert!(
        matches!(
            authorizer
                .fetch(&ctx, &reference, Representation::Raw)
                .await,
            Err(SourceError::NotFound)
        ),
        "after revocation, fetch must fail with the existence-hiding variant"
    );

    cleanup(&pool, env.tenant).await;
}

// ---------------------------------------------------------------------------
// 10. materialize drops denied/unknown candidates; only the allowed candidate
//     materializes, and cached_text never enters the output
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn materialize_drops_denied_candidates() {
    let _guard = SERIAL.lock().await;
    let (pool, authorizer) = setup().await;

    // One tenant shared by the allowed + denied messages so the dm row hits
    // the channel Deny path (not the tenant-scoped NotFound path).
    let tenant = TenantId::from(Uuid::new_v4());
    let keys = Keys::generate();
    let principal = PrincipalId::from(Uuid::new_v4());
    let community_id = format!("community-{}", Uuid::new_v4());
    let mapping_id = insert_mapping(&pool, tenant, &community_id).await;
    let binding_id = insert_binding(&pool, tenant, principal, &keys.public_key().to_hex()).await;
    insert_admission(
        &pool,
        tenant,
        mapping_id,
        binding_id,
        &keys.public_key().to_hex(),
        true,
    )
    .await;
    set_chat_enablement(&pool, tenant, true).await;

    let allowed_id = unique_hex64();
    let dm_id = unique_hex64();
    let unknown_id = unique_hex64();
    insert_observation(
        &pool,
        tenant,
        &community_id,
        &allowed_id,
        "workspace",
        "created",
        true,
        Some("allowed content"),
    )
    .await;
    insert_observation(
        &pool,
        tenant,
        &community_id,
        &dm_id,
        "dm",
        "created",
        true,
        Some("dm secret"),
    )
    .await;

    let ctx = user_ctx(principal, tenant);
    let candidates = vec![
        Candidate {
            resource: chat_ref(&allowed_id),
            cached_text: Some("stale cached hint".into()),
        },
        Candidate {
            resource: chat_ref(&dm_id),
            cached_text: Some("ATTACKER INDEXED SECRET".into()),
        },
        Candidate {
            resource: chat_ref(&unknown_id),
            cached_text: Some("ghost secret".into()),
        },
    ];

    let materialized = authorizer
        .materialize(&ctx, &chat_read_action(), candidates)
        .await
        .expect("materialization succeeds");
    assert_eq!(
        materialized.len(),
        1,
        "only the allowed candidate may materialize"
    );
    assert_eq!(materialized[0].resource, chat_ref(&allowed_id));
    assert_eq!(
        materialized[0].data.as_ref(),
        b"allowed content",
        "materialized data is the real authorized source content"
    );
    let output = String::from_utf8_lossy(&materialized[0].data);
    assert!(
        !output.contains("ATTACKER INDEXED SECRET"),
        "stale index text must never materialize"
    );
    assert!(
        !output.contains("stale cached hint"),
        "cached hints must never materialize"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 11. A later-pushed edit must not resurrect a deleted message
//     (authorizer-level tombstone override)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn post_delete_edit_does_not_reexpose() {
    let _guard = SERIAL.lock().await;
    let (pool, authorizer) = setup().await;
    let tenant = TenantId::from(Uuid::new_v4());
    let keys = Keys::generate();
    let principal = PrincipalId::from(Uuid::new_v4());
    let community_id = format!("community-{}", Uuid::new_v4());
    let message_id = unique_hex64();

    let mapping_id = insert_mapping(&pool, tenant, &community_id).await;
    let binding_id = insert_binding(&pool, tenant, principal, &keys.public_key().to_hex()).await;
    insert_admission(
        &pool,
        tenant,
        mapping_id,
        binding_id,
        &keys.public_key().to_hex(),
        true,
    )
    .await;
    set_chat_enablement(&pool, tenant, true).await;

    // The message root (created), then a Deleted observation at `t1`.
    let base = Utc::now() - Duration::seconds(60);
    let t1 = base + Duration::seconds(30);
    insert_observation_at(
        &pool,
        tenant,
        &community_id,
        &message_id,
        &message_id,
        "workspace",
        "created",
        true,
        Some("hello buzz"),
        base,
    )
    .await;
    insert_observation_at(
        &pool,
        tenant,
        &community_id,
        &message_id,
        &hex64(0x01),
        "workspace",
        "deleted",
        false,
        None,
        t1,
    )
    .await;
    // A later-pushed edit whose `event_created_at` TIES the delete at `t1`
    // (Nostr timestamps are second-resolution) and whose event id wins the
    // deterministic `event_id DESC` tie-break: `lookup_for_auth` returns the
    // ACTIVE edited row, so without the tombstone override the gate would
    // re-expose the deleted message. The override must keep it NotFound.
    insert_observation_at(
        &pool,
        tenant,
        &community_id,
        &message_id,
        &hex64(0x02),
        "workspace",
        "edited",
        true,
        Some("edited after delete"),
        t1,
    )
    .await;

    let ctx = user_ctx(principal, tenant);
    assert_eq!(
        authorizer
            .authorize(&ctx, &chat_read_action(), &chat_ref(&message_id))
            .await,
        Decision::NotFound,
        "a message with a Deleted observation at-or-after the candidate row must \
         never be re-exposed by a later-pushed edit"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 12. `lookup_for_auth` tie-break: same created_at → higher event_id wins
//     (deterministic ORDER BY, covered at the store level)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn lookup_for_auth_ties_break_by_event_id_desc() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    let community_id = format!("community-{}", Uuid::new_v4());
    let message_id = unique_hex64();
    let same_second = Utc::now();

    // Two rows for the same message with an identical `event_created_at`; the
    // deterministic `ORDER BY event_created_at DESC, event_id DESC` must pick
    // the higher event id (Nostr created_at is second-resolution, so this tie
    // is real in production and must not resolve arbitrarily).
    insert_observation_at(
        &pool,
        tenant,
        &community_id,
        &message_id,
        &hex64(0x01),
        "workspace",
        "created",
        true,
        None,
        same_second,
    )
    .await;
    insert_observation_at(
        &pool,
        tenant,
        &community_id,
        &message_id,
        &hex64(0x02),
        "workspace",
        "edited",
        true,
        None,
        same_second,
    )
    .await;

    let store = ChatObservationStore::new(pool.clone());
    let row = store
        .lookup_for_auth(tenant, &message_id)
        .await
        .expect("lookup must succeed")
        .expect("a row must be found");
    assert_eq!(
        row.event_id,
        hex64(0x02),
        "same-second rows must tie-break on event_id DESC"
    );
    assert_eq!(row.event_type, ObservedEventType::Edited);
    assert!(row.active);

    cleanup(&pool, tenant).await;
}
