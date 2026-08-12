//! DB-backed authorization tests for the Chat application read surface
//! (`/api/v1/applications/chat/status|channels|messages...`).
//!
//! These drive the real handlers (`chat_status`, `list_channels`,
//! `list_messages`, `get_message`) against a fully constructed `AppState`
//! whose Chat owner is the same `ChatResourceOwner` registered in the
//! `SourceAuthorizer` (local fallback authority — the unconfigured mode).
//! They prove the read-surface security matrix:
//!
//! * unconfigured tenants see an empty status shape;
//! * the channel list is gated channel-by-channel (workspace visible, `dm`
//!   denied in local mode);
//! * the timeline fold is tenant-scoped (another tenant's observation rows
//!   for the same community are never visible) and hides deleted/inactive
//!   messages;
//! * `get_message` is existence-hiding (404 for deleted and cross-tenant
//!   ids);
//! * revoking the principal's binding/admission empties reads immediately
//!   (proof #6 read half).
//!
//! DB-backed and `#[ignore]`d; run against the dev database (migrations
//! applied) with `--test-threads=1`:
//!
//!   DATABASE_URL=postgres://test:test@localhost:5432/test \
//!     cargo test --workspace --all-features --test chat_app_read_test -- \
//!       --ignored --test-threads=1
//!
//! Every test takes the shared `SERIAL` guard and cleans up exactly the rows
//! it created under fresh tenants (same convention as the chat-owner and
//! unified-search suites). Seeding inserts observation rows directly via SQL
//! — the bridge's already-verified state; the read surface trusts the
//! observation index and never re-verifies signatures.

use std::sync::{Arc, LazyLock};

use axum::extract::{Path, Query, State};
use chrono::{DateTime, Duration, Utc};
use nostr::Keys;
use rustshare_core::domain::{PrincipalId, TenantId};
use rustshare_core::events::EventBroadcaster;
use rustshare_core::services::{
    ChatIntegrationService, FileService, FolderService, HttpWebhookDispatcher, NotificationService,
    PermissionResolver, ShareService, ThumbnailService,
};
use rustshare_infrastructure::repositories::{
    FileRepository, FolderRepository, NotificationRepository, PermissionResolverRepository,
    ShareRepository, UserRepository,
};
use rustshare_server::authz::ChatResourceOwner;
use rustshare_server::handlers::chat_app::{
    chat_status, get_message, list_channels, list_messages, ListMessagesQuery,
};
use rustshare_server::handlers::{AppError, AuthenticatedUser};
use rustshare_server::AppState;
use rustshare_storage::repos::ShareNotificationRepoImpl;
use rustshare_storage::{
    ChatIdentityStore, ChatObservationStore, EventStore, MemoryCatalogStore, MetadataStore,
    ObjectStore, OutboxStore,
};
use sqlx::PgPool;
use uuid::Uuid;

/// Serializes the tests within this binary (same convention as the outbox and
/// chat-observation suites).
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

/// Remove every row the tests create for `tenant_id` (plus the
/// principal-keyed security events `revoke_principal` writes).
async fn cleanup(pool: &PgPool, tenant_id: TenantId, principal_id: PrincipalId) {
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
    sqlx::query("DELETE FROM user_security_events WHERE user_id = $1")
        .bind(principal_id.0)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE tenant_id = $1")
        .bind(tenant_id.0)
        .execute(pool)
        .await
        .unwrap();
}

/// A unique 64-lowercase-hex message id (two UUIDs concatenated).
fn unique_hex64() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

/// A fixed 64-lowercase-hex id for placeholder columns.
fn hex64(seed: u8) -> String {
    format!("{seed:02x}").repeat(32)
}

/// The `AuthenticatedUser` the handlers see.
fn auth(principal: PrincipalId, tenant: TenantId) -> AuthenticatedUser {
    AuthenticatedUser {
        user_id: principal.0,
        tenant_id: tenant.0,
    }
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

/// Enable the chat Application for `tenant` with the memory-projection
/// configuration (workspace == tenant).
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

/// Insert the minimal `users` row the security-event write in
/// `ChatIdentityStore::revoke_principal` requires (`user_security_events.user_id`
/// is a foreign key to `users.id`). Production revocations always act on real
/// users; the harness must provide one.
async fn insert_user(pool: &PgPool, tenant: TenantId, principal: PrincipalId) {
    sqlx::query(
        "INSERT INTO users
            (id, username, email, password_hash, display_name, is_admin, storage_quota, tenant_id)
         VALUES ($1, $2, $3, 'x', 'test', false, 0, $4)",
    )
    .bind(principal.0)
    .bind(format!("user-{}", Uuid::new_v4().simple()))
    .bind(format!("user-{}@example.test", Uuid::new_v4().simple()))
    .bind(tenant.0)
    .execute(pool)
    .await
    .unwrap();
}

/// Insert one `chat_observed_events` row directly via SQL (the bridge's
/// already-verified observation state; the read surface never re-verifies
/// signatures). `event_id == message_id` for created roots.
#[allow(clippy::too_many_arguments)]
async fn insert_observation(
    pool: &PgPool,
    tenant: TenantId,
    community_id: &str,
    channel_id: &str,
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
    .bind(channel_id)
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

/// The ids a fully configured tenant's assertions need.
struct ChatEnv {
    tenant: TenantId,
    principal: PrincipalId,
    community_id: String,
}

/// Full current-state setup: active mapping + active binding + active
/// admission + chat enablement for a fresh tenant and community. No
/// observation rows are seeded here — tests add exactly the rows they need.
async fn setup_env(pool: &PgPool) -> ChatEnv {
    let tenant = TenantId::from(Uuid::new_v4());
    let keys = Keys::generate();
    let principal = PrincipalId::from(Uuid::new_v4());
    let community_id = format!("community-{}", Uuid::new_v4());

    let mapping_id = insert_mapping(pool, tenant, &community_id).await;
    let binding_id = insert_binding(pool, tenant, principal, &keys.public_key().to_hex()).await;
    insert_admission(
        pool,
        tenant,
        mapping_id,
        binding_id,
        &keys.public_key().to_hex(),
    )
    .await;
    set_chat_enablement(pool, tenant, true).await;
    insert_user(pool, tenant, principal).await;

    ChatEnv {
        tenant,
        principal,
        community_id,
    }
}

/// Build the full `AppState` (same service graph the other DB-backed handler
/// suites construct) with the Chat owner wired BOTH into the source
/// authorizer and onto `AppState.chat_owner` — the same instance, so the
/// channel gate and the per-message gate agree.
async fn setup_app_state(pool: PgPool) -> (AppState, ChatIdentityStore, Arc<ChatObservationStore>) {
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

    let vault_sync_service = Arc::new(rustshare_core::services::VaultSyncService::new(
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

    let secret_key = rustshare_crypto::SecretEncryptionKey::from_bytes([0u8; 32]);

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
        Arc::new(rustshare_core::domain::ApplicationRegistry::first_party().unwrap()),
    ));
    let chat_identity_store = ChatIdentityStore::new(pool.clone());
    let chat_observation_store = Arc::new(ChatObservationStore::new(pool.clone()));
    let memory_catalog_store = Arc::new(MemoryCatalogStore::new(pool.clone()));
    let buzz_observation_service = Arc::new(
        rustshare_server::buzz_observation::BuzzObservationService::new(
            pool.clone(),
            chat_identity_store.clone(),
            (*chat_observation_store).clone(),
            outbox_store.clone(),
            rustshare_crypto::WebhookSigner::new("test-secret"),
            300,
            Arc::new(EventBroadcaster::new(64)),
        ),
    );

    // ONE Chat owner instance, registered in the source authorizer AND exposed
    // on AppState — the channel gate and the per-message gate must agree.
    let chat_owner = Arc::new(ChatResourceOwner::new(
        chat_identity_store.clone(),
        (*chat_observation_store).clone(),
    ));
    let mut owners = rustshare_resource_auth::ResourceOwnerRegistry::new();
    let chat_owner_registered: Arc<dyn rustshare_resource_auth::ResourceOwner> = chat_owner.clone();
    owners
        .register(
            chat_owner_registered,
            &rustshare_core::domain::ApplicationRegistry::first_party().unwrap(),
        )
        .expect("the io.elembra.chat owner registers against the canonical registry");
    let source_authorizer = Arc::new(rustshare_resource_auth::SourceAuthorizer::new(owners));

    let unified_search_service = Arc::new(
        rustshare_server::services::unified_search::UnifiedSearchService::new(
            source_authorizer.clone(),
            metadata_store.clone(),
            None,
            memory_catalog_store.clone(),
        ),
    );

    let ask_workspace_service = Arc::new(
        rustshare_server::services::ask_workspace::AskWorkspaceService::new(
            unified_search_service.clone(),
            None,
        ),
    );

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
        rate_limit_config: Arc::new(rustshare_server::middleware::RateLimitConfig::new()),
        secret_key,
        oidc_runtime_cache: rustshare_server::oidc_runtime::OidcRuntimeCache::new(),
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
        buzz_gateway: None,
        outbox_status: Arc::new(rustshare_server::outbox_dispatcher::OutboxStatus::default()),
        outbox_worker_enabled: false,
        outbox_readiness_staleness_secs: 60,
        shutdown_tx: tokio::sync::broadcast::channel(1).0,
        prometheus_handle: rustshare_server::metrics::init_metrics(),
    };

    (state, chat_identity_store, chat_observation_store)
}

// ---------------------------------------------------------------------------
// 1. Unconfigured tenant: the status endpoint returns the empty shape
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn status_unconfigured_returns_empty_shape() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let (state, _, _) = setup_app_state(pool.clone()).await;

    // A fresh tenant with no enablement, mapping, binding or admission.
    let tenant = TenantId::from(Uuid::new_v4());
    let principal = PrincipalId::from(Uuid::new_v4());

    let response = chat_status(State(state), auth(principal, tenant))
        .await
        .expect("status must succeed for an unconfigured tenant");
    let body = response.0;
    assert!(
        !body.chat_enabled,
        "an unconfigured tenant must report chat disabled"
    );
    assert!(body.mapping.is_none(), "no mapping must be reported");
    assert!(body.binding.is_none(), "no binding must be reported");
    assert!(
        !body.admission_active,
        "no admission must be reported as active"
    );

    cleanup(&pool, tenant, principal).await;
}

// ---------------------------------------------------------------------------
// 2. The channel list is gated channel-by-channel: workspace visible, dm
//    denied (local authority mode)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn channels_requires_mapping_and_gate() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let (state, _, _) = setup_app_state(pool.clone()).await;
    let env = setup_env(&pool).await;

    let base = Utc::now() - Duration::seconds(60);
    insert_observation(
        &pool,
        env.tenant,
        &env.community_id,
        "channel-ws",
        &unique_hex64(),
        &unique_hex64(),
        "workspace",
        "created",
        true,
        Some("workspace message"),
        base,
    )
    .await;
    insert_observation(
        &pool,
        env.tenant,
        &env.community_id,
        "channel-dm",
        &unique_hex64(),
        &unique_hex64(),
        "dm",
        "created",
        true,
        Some("dm message"),
        base + Duration::seconds(30),
    )
    .await;

    let response = list_channels(State(state), auth(env.principal, env.tenant))
        .await
        .expect("channels must succeed for a configured tenant");
    let channels = response.0;
    let visible: Vec<String> = channels.iter().map(|c| c.channel_id.clone()).collect();
    assert_eq!(
        visible,
        vec!["channel-ws".to_string()],
        "local mode: the workspace channel is listed, the dm channel is not"
    );
    assert_eq!(channels[0].channel_kind, "workspace");

    cleanup(&pool, env.tenant, env.principal).await;
}

// ---------------------------------------------------------------------------
// 3. Timeline fold is tenant-scoped: another tenant's observation rows for
//    the same community are never visible (proof #4)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn timeline_returns_authorized_messages_folded() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let (state, _, _) = setup_app_state(pool.clone()).await;
    let env = setup_env(&pool).await;

    let base = Utc::now() - Duration::seconds(60);
    let message_id = unique_hex64();
    let edit_event_id = unique_hex64();
    insert_observation(
        &pool,
        env.tenant,
        &env.community_id,
        "channel-1",
        &message_id,
        &message_id,
        "workspace",
        "created",
        true,
        Some("hello buzz"),
        base,
    )
    .await;
    insert_observation(
        &pool,
        env.tenant,
        &env.community_id,
        "channel-1",
        &message_id,
        &edit_event_id,
        "workspace",
        "edited",
        true,
        Some("hello buzz (edited)"),
        base + Duration::seconds(30),
    )
    .await;

    // Tenant B seeds an observation row into the SAME community/channel with
    // a foreign tenant id (a leaked/adjacent relay would do exactly this).
    // The timeline query is tenant-scoped and must never surface it.
    let tenant_b = TenantId::from(Uuid::new_v4());
    let principal_b = PrincipalId::from(Uuid::new_v4());
    let foreign_message_id = unique_hex64();
    insert_observation(
        &pool,
        tenant_b,
        &env.community_id,
        "channel-1",
        &foreign_message_id,
        &foreign_message_id,
        "workspace",
        "created",
        true,
        Some("ATTACKER SECRET"),
        base + Duration::seconds(45),
    )
    .await;

    let response = list_messages(
        State(state),
        auth(env.principal, env.tenant),
        Query(ListMessagesQuery {
            channel_id: "channel-1".to_string(),
            before: None,
            limit: None,
        }),
    )
    .await
    .expect("timeline must succeed");
    let messages = response.0.messages;
    assert_eq!(
        messages.len(),
        1,
        "the fold returns exactly the caller's message, never the foreign tenant's"
    );
    assert_eq!(messages[0].message_id, message_id);
    assert_eq!(
        messages[0].event_id, edit_event_id,
        "the fold exposes the latest event of the message"
    );
    assert_eq!(messages[0].body.as_deref(), Some("hello buzz (edited)"));
    assert_eq!(messages[0].channel_kind, "workspace");

    cleanup(&pool, env.tenant, env.principal).await;
    cleanup(&pool, tenant_b, principal_b).await;
}

// ---------------------------------------------------------------------------
// 4. The timeline hides deleted and inactive messages
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn timeline_hides_deleted_and_inactive() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let (state, _, _) = setup_app_state(pool.clone()).await;
    let env = setup_env(&pool).await;

    let base = Utc::now() - Duration::seconds(120);
    // A message with a later `deleted` event.
    let deleted_id = unique_hex64();
    insert_observation(
        &pool,
        env.tenant,
        &env.community_id,
        "channel-1",
        &deleted_id,
        &deleted_id,
        "workspace",
        "created",
        true,
        Some("soon deleted"),
        base,
    )
    .await;
    insert_observation(
        &pool,
        env.tenant,
        &env.community_id,
        "channel-1",
        &deleted_id,
        &unique_hex64(),
        "workspace",
        "deleted",
        false,
        None,
        base + Duration::seconds(30),
    )
    .await;
    // An inactive row.
    let inactive_id = unique_hex64();
    insert_observation(
        &pool,
        env.tenant,
        &env.community_id,
        "channel-1",
        &inactive_id,
        &inactive_id,
        "workspace",
        "created",
        false,
        Some("inactive"),
        base + Duration::seconds(60),
    )
    .await;
    // The one visible message.
    let visible_id = unique_hex64();
    insert_observation(
        &pool,
        env.tenant,
        &env.community_id,
        "channel-1",
        &visible_id,
        &visible_id,
        "workspace",
        "created",
        true,
        Some("visible"),
        base + Duration::seconds(90),
    )
    .await;

    let response = list_messages(
        State(state),
        auth(env.principal, env.tenant),
        Query(ListMessagesQuery {
            channel_id: "channel-1".to_string(),
            before: None,
            limit: None,
        }),
    )
    .await
    .expect("timeline must succeed");
    let ids: Vec<String> = response
        .0
        .messages
        .iter()
        .map(|m| m.message_id.clone())
        .collect();
    assert_eq!(
        ids,
        vec![visible_id.clone()],
        "deleted and inactive messages must be hidden; only the visible one remains"
    );

    cleanup(&pool, env.tenant, env.principal).await;
}

// ---------------------------------------------------------------------------
// 5. get_message is existence-hiding: deleted -> 404, cross-tenant -> 404
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn get_message_hides_denied_and_deleted() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let (state, _, _) = setup_app_state(pool.clone()).await;
    let env = setup_env(&pool).await;

    let base = Utc::now() - Duration::seconds(60);
    // A message tombstoned by a later `deleted` event.
    let deleted_id = unique_hex64();
    insert_observation(
        &pool,
        env.tenant,
        &env.community_id,
        "channel-1",
        &deleted_id,
        &deleted_id,
        "workspace",
        "created",
        true,
        Some("soon deleted"),
        base,
    )
    .await;
    insert_observation(
        &pool,
        env.tenant,
        &env.community_id,
        "channel-1",
        &deleted_id,
        &unique_hex64(),
        "workspace",
        "deleted",
        false,
        None,
        base + Duration::seconds(30),
    )
    .await;

    let deleted_result = get_message(
        State(state.clone()),
        auth(env.principal, env.tenant),
        Path(deleted_id),
    )
    .await;
    assert!(
        matches!(deleted_result, Err(AppError::NotFound(_))),
        "a deleted message must look absent"
    );

    // A message id that only exists in another tenant's observation index.
    let tenant_b = TenantId::from(Uuid::new_v4());
    let principal_b = PrincipalId::from(Uuid::new_v4());
    let foreign_id = unique_hex64();
    insert_observation(
        &pool,
        tenant_b,
        &env.community_id,
        "channel-1",
        &foreign_id,
        &foreign_id,
        "workspace",
        "created",
        true,
        Some("foreign secret"),
        base,
    )
    .await;

    let cross_result = get_message(
        State(state),
        auth(env.principal, env.tenant),
        Path(foreign_id),
    )
    .await;
    assert!(
        matches!(cross_result, Err(AppError::NotFound(_))),
        "a cross-tenant message id must look absent"
    );

    cleanup(&pool, env.tenant, env.principal).await;
    cleanup(&pool, tenant_b, principal_b).await;
}

// ---------------------------------------------------------------------------
// 6. Revoking the principal's binding/admission empties reads immediately
//    (proof #6 read half)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn revoked_binding_denies_reads() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let (state, chat_identity, _) = setup_app_state(pool.clone()).await;
    let env = setup_env(&pool).await;

    let base = Utc::now() - Duration::seconds(60);
    let message_id = unique_hex64();
    insert_observation(
        &pool,
        env.tenant,
        &env.community_id,
        "channel-1",
        &message_id,
        &message_id,
        "workspace",
        "created",
        true,
        Some("visible before revocation"),
        base,
    )
    .await;

    let before = list_messages(
        State(state.clone()),
        auth(env.principal, env.tenant),
        Query(ListMessagesQuery {
            channel_id: "channel-1".to_string(),
            before: None,
            limit: None,
        }),
    )
    .await
    .expect("timeline must succeed before revocation");
    assert_eq!(
        before.0.messages.len(),
        1,
        "the active member reads the message before revocation"
    );
    assert!(
        get_message(
            State(state.clone()),
            auth(env.principal, env.tenant),
            Path(message_id.clone())
        )
        .await
        .is_ok(),
        "the active member reads the single message before revocation"
    );

    let revoked = chat_identity
        .revoke_principal(env.tenant, env.principal)
        .await
        .expect("revocation must succeed");
    assert!(revoked > 0, "the principal had a live binding to revoke");

    let after = list_messages(
        State(state.clone()),
        auth(env.principal, env.tenant),
        Query(ListMessagesQuery {
            channel_id: "channel-1".to_string(),
            before: None,
            limit: None,
        }),
    )
    .await
    .expect("timeline must succeed after revocation");
    assert!(
        after.0.messages.is_empty(),
        "a revoked principal must see an empty timeline"
    );
    assert!(
        matches!(
            get_message(
                State(state),
                auth(env.principal, env.tenant),
                Path(message_id)
            )
            .await,
            Err(AppError::NotFound(_))
        ),
        "a revoked principal must get 404 for the previously visible message"
    );

    cleanup(&pool, env.tenant, env.principal).await;
}
