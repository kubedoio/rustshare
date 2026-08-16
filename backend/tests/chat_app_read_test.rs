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
use axum::Json;
use chrono::{DateTime, Duration, Utc};
use nostr::Keys;
use rustshare_core::domain::{ApplicationId, PrincipalId, TenantId};
use rustshare_core::events::EventBroadcaster;
use rustshare_core::services::{
    ChatIntegrationService, FileService, FolderService, HttpWebhookDispatcher, NotificationService,
    PermissionResolver, ShareService, ThumbnailService,
};
use rustshare_infrastructure::repositories::{
    FileRepository, FolderRepository, NotificationRepository, PermissionResolverRepository,
    ShareRepository, UserRepository,
};
use rustshare_resource_auth::ResourceRef;
use rustshare_server::authz::ChatResourceOwner;
use rustshare_server::config::ChatProvisioningMode;
use rustshare_server::handlers::chat_app::{
    chat_status, get_message, list_channels, list_messages, ListMessagesQuery,
};
use rustshare_server::handlers::{
    open_attachment, prepare_attachment, preview_attachment, AppError, AuthenticatedUser,
};
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
    // Files uploaded by the attachment tests: `files.owner_id` cascades from
    // `users`, but `file_versions.created_by` is a plain FK — drop versions
    // first so the user deletion below cannot violate it.
    sqlx::query(
        "DELETE FROM file_versions WHERE created_by IN (SELECT id FROM users WHERE tenant_id = $1)",
    )
    .bind(tenant_id.0)
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

/// Insert a mapping row for `community_id` under `tenant` (workspace ==
/// tenant, per the platform invariant) with the given `active` flag. Returns
/// the mapping id.
async fn insert_mapping(pool: &PgPool, tenant: TenantId, community_id: &str, active: bool) -> Uuid {
    let mapping_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_workspace_communities
            (mapping_id, tenant_id, workspace_id, community_id, relay_url, active)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(mapping_id)
    .bind(tenant.0)
    .bind(tenant.0)
    .bind(community_id)
    .bind("wss://relay.example.test")
    .bind(active)
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
    insert_observation_with_refs(
        pool,
        tenant,
        community_id,
        channel_id,
        message_id,
        event_id,
        channel_kind,
        event_type,
        active,
        body,
        created_at,
        &[],
    )
    .await;
}

/// [`insert_observation`] plus the row's retained identifier-only
/// `elembra-ref` attachment references (canonical `ResourceRef` JSON).
#[allow(clippy::too_many_arguments)]
async fn insert_observation_with_refs(
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
    attachment_refs: &[ResourceRef],
) {
    sqlx::query(
        "INSERT INTO chat_observed_events
            (tenant_id, workspace_id, event_id, message_id, event_type,
             community_id, channel_id, channel_kind, author_pubkey,
             event_created_at, observed_at, checksum, signature,
             signature_verified, body, attachment_refs, active)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, now(),
                 $11, $12, true, $13, $14, $15)",
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
    .bind(serde_json::to_value(attachment_refs).unwrap())
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

    let mapping_id = insert_mapping(pool, tenant, &community_id, true).await;
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

    let permission_repo = Arc::new(PermissionResolverRepository::new(pool.clone()));
    let permission_resolver = Arc::new(PermissionResolver::new(permission_repo.clone()));

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
    // The Files owner, so attachment open/preview reauthorize through the real
    // Files permission semantics (the endpoint mapping is what the chat read
    // surface proves; authorizer-level denial lives in source_authorization_test).
    owners
        .register(
            Arc::new(rustshare_server::authz::FilesResourceOwner::new(
                permission_resolver.clone(),
                permission_repo,
                metadata_store.clone(),
                object_store.clone(),
            )),
            &rustshare_core::domain::ApplicationRegistry::first_party().unwrap(),
        )
        .expect("the io.elembra.files owner registers against the canonical registry");
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
        chat_bootstrap: None,
        chat_provisioning: ChatProvisioningMode::Manual,
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
// 1b. Inactive mapping: status must not disclose community/relay details
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn status_hides_inactive_mapping_details() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let (state, _, _) = setup_app_state(pool.clone()).await;

    let tenant = TenantId::from(Uuid::new_v4());
    let keys = Keys::generate();
    let principal = PrincipalId::from(Uuid::new_v4());
    let community_id = format!("community-{}", Uuid::new_v4());

    // Full setup EXCEPT the mapping is inactive: the caller has a live
    // binding and an admission row, but the community mapping is off.
    let mapping_id = insert_mapping(&pool, tenant, &community_id, false).await;
    let binding_id = insert_binding(&pool, tenant, principal, &keys.public_key().to_hex()).await;
    insert_admission(
        &pool,
        tenant,
        mapping_id,
        binding_id,
        &keys.public_key().to_hex(),
    )
    .await;
    set_chat_enablement(&pool, tenant, true).await;
    insert_user(&pool, tenant, principal).await;

    let response = chat_status(State(state), auth(principal, tenant))
        .await
        .expect("status must succeed with an inactive mapping");
    let body = response.0;
    assert!(
        body.chat_enabled,
        "the tenant-level chat enablement is still the caller's own state"
    );
    assert!(
        body.mapping.is_none(),
        "an inactive mapping must not disclose community/relay configuration"
    );
    assert!(
        body.binding.is_some(),
        "the caller's own binding remains the caller's own data"
    );
    assert!(
        !body.admission_active,
        "an inactive mapping cannot yield an active admission"
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

// ---------------------------------------------------------------------------
// 7. revoke_principal: exact revocation semantics (regression guard for the
//    ambiguous `revoked_at` qualification fix — the admission's own timestamp
//    must be preserved, and revocation must be exactly tenant/principal
//    scoped and idempotent)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn revoke_principal_scope_and_timestamp_semantics() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let chat_identity = ChatIdentityStore::new(pool.clone());

    // Tenant A: the revoked principal plus an unrelated principal in the same
    // tenant who must stay untouched.
    let env_a = setup_env(&pool).await;
    let (mapping_a,): (Uuid,) =
        sqlx::query_as("SELECT mapping_id FROM chat_workspace_communities WHERE tenant_id = $1")
            .bind(env_a.tenant.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    let other_keys = Keys::generate();
    let other = PrincipalId::from(Uuid::new_v4());
    let other_binding = insert_binding(
        &pool,
        env_a.tenant,
        other,
        &other_keys.public_key().to_hex(),
    )
    .await;
    insert_admission(
        &pool,
        env_a.tenant,
        mapping_a,
        other_binding,
        &other_keys.public_key().to_hex(),
    )
    .await;

    // Tenant B: a fully configured foreign tenant that must stay untouched.
    let env_b = setup_env(&pool).await;

    let revoked = chat_identity
        .revoke_principal(env_a.tenant, env_a.principal)
        .await
        .expect("first revocation must succeed");
    assert_eq!(revoked, 1, "exactly one live binding is revoked");

    // The principal's own binding is revoked and timestamped.
    let (binding_status, binding_revoked_at): (String, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT status, revoked_at FROM chat_identity_bindings
         WHERE tenant_id = $1 AND principal_id = $2",
    )
    .bind(env_a.tenant.0)
    .bind(env_a.principal.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(binding_status, "revoked");
    let first_binding_revoked_at =
        binding_revoked_at.expect("revocation must stamp the binding's revoked_at");

    // The matching admission is inactive and timestamped.
    let (pubkey,): (String,) = sqlx::query_as(
        "SELECT buzz_pubkey FROM chat_identity_bindings
         WHERE tenant_id = $1 AND principal_id = $2",
    )
    .bind(env_a.tenant.0)
    .bind(env_a.principal.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    let (admission_active, admission_revoked_at): (bool, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT a.active, a.revoked_at FROM chat_buzz_admissions a
         JOIN chat_identity_bindings b
           ON b.tenant_id = a.tenant_id AND b.binding_id = a.binding_id
         WHERE a.tenant_id = $1 AND b.principal_id = $2",
    )
    .bind(env_a.tenant.0)
    .bind(env_a.principal.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!admission_active, "the admission must be deactivated");
    let first_admission_revoked_at =
        admission_revoked_at.expect("revocation must stamp the admission's revoked_at");
    assert!(
        !chat_identity
            .active_admission(env_a.tenant, &env_a.community_id, &pubkey)
            .await
            .unwrap(),
        "active_admission must report false after revocation"
    );

    // Idempotent: a second revocation touches nothing and preserves the
    // original timestamps (the COALESCE reads the admission's own column).
    let revoked_again = chat_identity
        .revoke_principal(env_a.tenant, env_a.principal)
        .await
        .expect("second revocation must succeed");
    assert_eq!(revoked_again, 0, "a second revocation is a no-op");
    let (binding_revoked_at_again, admission_revoked_at_again): (
        Option<DateTime<Utc>>,
        Option<DateTime<Utc>>,
    ) = sqlx::query_as(
        "SELECT b.revoked_at, a.revoked_at FROM chat_identity_bindings b
         JOIN chat_buzz_admissions a
           ON a.tenant_id = b.tenant_id AND a.binding_id = b.binding_id
         WHERE b.tenant_id = $1 AND b.principal_id = $2",
    )
    .bind(env_a.tenant.0)
    .bind(env_a.principal.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        binding_revoked_at_again,
        Some(first_binding_revoked_at),
        "the binding's original revoked_at is preserved"
    );
    assert_eq!(
        admission_revoked_at_again,
        Some(first_admission_revoked_at),
        "the admission's original revoked_at is preserved"
    );

    // The unrelated principal in the same tenant is untouched.
    let (other_status, other_revoked_at): (String, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT status, revoked_at FROM chat_identity_bindings
         WHERE tenant_id = $1 AND principal_id = $2",
    )
    .bind(env_a.tenant.0)
    .bind(other.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(other_status, "active");
    assert!(other_revoked_at.is_none());
    let (other_admission_active, other_admission_revoked_at): (bool, Option<DateTime<Utc>>) =
        sqlx::query_as(
            "SELECT a.active, a.revoked_at FROM chat_buzz_admissions a
             JOIN chat_identity_bindings b
               ON b.tenant_id = a.tenant_id AND b.binding_id = a.binding_id
             WHERE a.tenant_id = $1 AND b.principal_id = $2",
        )
        .bind(env_a.tenant.0)
        .bind(other.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(other_admission_active);
    assert!(other_admission_revoked_at.is_none());

    // The foreign tenant is untouched.
    let (tenant_b_status, tenant_b_admission_active): (String, bool) = sqlx::query_as(
        "SELECT b.status, a.active FROM chat_identity_bindings b
         JOIN chat_buzz_admissions a
           ON a.tenant_id = b.tenant_id AND a.binding_id = b.binding_id
         WHERE b.tenant_id = $1 AND b.principal_id = $2",
    )
    .bind(env_b.tenant.0)
    .bind(env_b.principal.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(tenant_b_status, "active");
    assert!(tenant_b_admission_active);

    cleanup(&pool, env_a.tenant, env_a.principal).await;
    cleanup(&pool, env_b.tenant, env_b.principal).await;
}

// ---------------------------------------------------------------------------
// 8. Pagination advances past a fully filtered page: when a whole page is
//    tombstoned/denied, `next_before` comes from the last *fetched* row so
//    older authorized messages stay reachable
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn pagination_advances_past_a_fully_tombstoned_page() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let (state, _, _) = setup_app_state(pool.clone()).await;
    let env = setup_env(&pool).await;

    let base = Utc::now() - Duration::seconds(120);
    // A tombstoned message whose latest (deleted) event is the newest row.
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
    // An older authorized message that the tombstone page must not hide.
    let older_id = unique_hex64();
    insert_observation(
        &pool,
        env.tenant,
        &env.community_id,
        "channel-1",
        &older_id,
        &older_id,
        "workspace",
        "created",
        true,
        Some("still readable"),
        base - Duration::seconds(10),
    )
    .await;

    // Page 1 (limit 1): the newest folded row is the tombstone — filtered out,
    // but the cursor must still advance.
    let page1 = list_messages(
        State(state.clone()),
        auth(env.principal, env.tenant),
        Query(ListMessagesQuery {
            channel_id: "channel-1".to_string(),
            before: None,
            limit: Some(1),
        }),
    )
    .await
    .expect("first page must succeed");
    assert!(
        page1.0.messages.is_empty(),
        "the tombstone page must be hidden from the timeline"
    );
    let cursor = page1
        .0
        .next_before
        .expect("pagination must advance past a fully filtered page");

    // Page 2: folding the full history never re-emits the tombstoned message's
    // pre-delete row, so the older authorized message is reachable immediately.
    let page2 = list_messages(
        State(state),
        auth(env.principal, env.tenant),
        Query(ListMessagesQuery {
            channel_id: "channel-1".to_string(),
            before: Some(cursor),
            limit: Some(1),
        }),
    )
    .await
    .expect("second page must succeed");
    assert_eq!(
        page2.0.messages.len(),
        1,
        "the older authorized message must be reachable past the tombstone"
    );
    assert_eq!(
        page2.0.messages[0].message_id, older_id,
        "the reachable message is the older one"
    );

    cleanup(&pool, env.tenant, env.principal).await;
}

// ---------------------------------------------------------------------------
// 9. Attachments fail closed at the endpoint: an inaccessible Files ref is an
//    existence-hiding 404 across prepare/preview/open. (Authorizer-level
//    denial for existing-but-denied files is covered by
//    source_authorization_test.rs; this proves the endpoint mapping.)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn attachment_endpoints_deny_inaccessible_files() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let (state, _, _) = setup_app_state(pool.clone()).await;
    let env = setup_env(&pool).await;

    // A ref to a file that does not exist (and, being random, cannot belong to
    // this principal either): every attachment endpoint must 404 without
    // revealing existence or ownership.
    let request = || {
        Json(rustshare_server::handlers::chat_resource::ResourceRequest {
            resource: ResourceRef::new(
                ApplicationId::new("io.elembra.files"),
                "file",
                Uuid::new_v4().to_string(),
            ),
        })
    };

    let prepare = prepare_attachment(
        State(state.clone()),
        auth(env.principal, env.tenant),
        request(),
    )
    .await;
    assert!(
        matches!(prepare, Err(AppError::NotFound(_))),
        "prepare must hide inaccessible files, got {prepare:?}"
    );

    let preview = preview_attachment(
        State(state.clone()),
        auth(env.principal, env.tenant),
        request(),
    )
    .await;
    assert!(
        matches!(preview, Err(AppError::NotFound(_))),
        "preview must hide inaccessible files, got {preview:?}"
    );

    let open = open_attachment(State(state), auth(env.principal, env.tenant), request()).await;
    assert!(
        matches!(open, Err(AppError::NotFound(_))),
        "open must hide inaccessible files, got {open:?}"
    );

    cleanup(&pool, env.tenant, env.principal).await;
}

// ---------------------------------------------------------------------------
// 10. Attachment refs surface on the timeline DTO (issue #242): the fold
//     surfaces the latest event's refs (an edit REPLACES them), tombstones
//     never expose refs, and opening reauthorizes through the Files owner
//     with existence-hiding errors for missing or cross-tenant files.
// ---------------------------------------------------------------------------

/// A canonical Files ref for a test file id.
fn file_ref(id: &str) -> ResourceRef {
    ResourceRef::new(ApplicationId::new("io.elembra.files"), "file", id)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn timeline_surfaces_attachment_refs_and_edit_replaces_them() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let (state, _, _) = setup_app_state(pool.clone()).await;
    let env = setup_env(&pool).await;
    let base = Utc::now() - Duration::seconds(120);

    // A message created with two attachments, then edited with one.
    let attached_id = unique_hex64();
    insert_observation_with_refs(
        &pool,
        env.tenant,
        &env.community_id,
        "channel-1",
        &attached_id,
        &attached_id,
        "workspace",
        "created",
        true,
        Some("with attachments"),
        base,
        &[file_ref("f-1"), file_ref("f-2")],
    )
    .await;
    insert_observation_with_refs(
        &pool,
        env.tenant,
        &env.community_id,
        "channel-1",
        &attached_id,
        &unique_hex64(),
        "workspace",
        "edited",
        true,
        Some("edited, one attachment"),
        base + Duration::seconds(10),
        &[file_ref("f-3")],
    )
    .await;
    // A message with no attachments at all.
    let plain_id = unique_hex64();
    insert_observation(
        &pool,
        env.tenant,
        &env.community_id,
        "channel-1",
        &plain_id,
        &plain_id,
        "workspace",
        "created",
        true,
        Some("no attachments"),
        base,
    )
    .await;

    let timeline = list_messages(
        State(state.clone()),
        auth(env.principal, env.tenant),
        Query(ListMessagesQuery {
            channel_id: "channel-1".to_string(),
            before: None,
            limit: None,
        }),
    )
    .await
    .expect("timeline must succeed");
    let messages = timeline.0.messages;
    assert_eq!(messages.len(), 2);

    let attached = messages
        .iter()
        .find(|m| m.message_id == attached_id)
        .expect("the attached message must be on the timeline");
    assert_eq!(
        attached.attachments.len(),
        1,
        "the edit's refs replace the created refs wholesale"
    );
    assert_eq!(attached.attachments[0].application, "io.elembra.files");
    assert_eq!(attached.attachments[0].resource_type, "file");
    assert_eq!(attached.attachments[0].resource_id, "f-3");
    assert_eq!(attached.attachments[0].version, None);

    let plain = messages
        .iter()
        .find(|m| m.message_id == plain_id)
        .expect("the plain message must be on the timeline");
    assert!(plain.attachments.is_empty(), "no refs means no affordance");

    // The single-message endpoint surfaces the same attachments.
    let single = get_message(
        State(state),
        auth(env.principal, env.tenant),
        Path(attached_id.clone()),
    )
    .await
    .expect("get_message must succeed");
    assert_eq!(single.0.attachments.len(), 1);
    assert_eq!(single.0.attachments[0].resource_id, "f-3");

    cleanup(&pool, env.tenant, env.principal).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn timeline_hides_attachments_of_tombstoned_messages() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let (state, _, _) = setup_app_state(pool.clone()).await;
    let env = setup_env(&pool).await;
    let base = Utc::now() - Duration::seconds(120);

    // Even a deleted event carrying refs must never surface them: the fold
    // picks the tombstone row and the read surface drops the message.
    let deleted_id = unique_hex64();
    insert_observation_with_refs(
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
        &[file_ref("f-1")],
    )
    .await;
    insert_observation_with_refs(
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
        &[file_ref("f-9")],
    )
    .await;

    let timeline = list_messages(
        State(state.clone()),
        auth(env.principal, env.tenant),
        Query(ListMessagesQuery {
            channel_id: "channel-1".to_string(),
            before: None,
            limit: None,
        }),
    )
    .await
    .expect("timeline must succeed");
    assert!(
        timeline.0.messages.is_empty(),
        "a tombstoned message must never surface, with or without refs"
    );

    let single = get_message(
        State(state),
        auth(env.principal, env.tenant),
        Path(deleted_id),
    )
    .await;
    assert!(
        matches!(single, Err(AppError::NotFound(_))),
        "get_message must stay existence-hiding for a tombstoned message, got {single:?}"
    );

    cleanup(&pool, env.tenant, env.principal).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn attachment_open_preview_authorize_through_files_owner() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let (state, _, _) = setup_app_state(pool.clone()).await;
    let env = setup_env(&pool).await;
    let foreign = setup_env(&pool).await;

    // A real file owned by env's principal in env's tenant.
    let file = state
        .file_service
        .upload_file(
            env.principal.0,
            "plan.txt".to_string(),
            None,
            bytes::Bytes::from_static(b"attachment bytes"),
            "text/plain".to_string(),
            env.tenant.0,
        )
        .await
        .expect("upload the test file");
    let reference = file_ref(&file.id.to_string());
    let request = || {
        Json(rustshare_server::handlers::chat_resource::ResourceRequest {
            resource: reference.clone(),
        })
    };

    // Authorized preview: safe metadata only.
    let preview = preview_attachment(
        State(state.clone()),
        auth(env.principal, env.tenant),
        request(),
    )
    .await
    .expect("the owner must preview their own file");
    assert_eq!(preview.0.display_name, "plan.txt");
    assert_eq!(preview.0.resource, reference);

    // Authorized open: the exact bytes stream through, served as a forced
    // download so a surprising attachment can never execute as same-origin
    // script in the recipient's browser (Content-Disposition + nosniff).
    let open = open_attachment(
        State(state.clone()),
        auth(env.principal, env.tenant),
        request(),
    )
    .await
    .expect("the owner must open their own file");
    {
        let headers = open.headers();
        assert_eq!(
            headers
                .get("content-disposition")
                .and_then(|v| v.to_str().ok()),
            Some("attachment"),
            "open must force a download disposition"
        );
        assert_eq!(
            headers
                .get("x-content-type-options")
                .and_then(|v| v.to_str().ok()),
            Some("nosniff"),
            "open must refuse content-type sniffing"
        );
    }
    let body = axum::body::to_bytes(open.into_body(), 1024)
        .await
        .expect("read the streamed body");
    assert_eq!(&body[..], b"attachment bytes");

    // The same ref from ANOTHER tenant: the file exists, but not for this
    // principal — an existence-hiding 404, never an "it exists" signal.
    let foreign_open = open_attachment(
        State(state.clone()),
        auth(foreign.principal, foreign.tenant),
        request(),
    )
    .await;
    assert!(
        matches!(foreign_open, Err(AppError::NotFound(_))),
        "cross-tenant open must be an existence-hiding 404, got {foreign_open:?}"
    );
    let foreign_preview = preview_attachment(
        State(state),
        auth(foreign.principal, foreign.tenant),
        request(),
    )
    .await;
    assert!(
        matches!(foreign_preview, Err(AppError::NotFound(_))),
        "cross-tenant preview must be an existence-hiding 404, got {foreign_preview:?}"
    );

    cleanup(&pool, env.tenant, env.principal).await;
    cleanup(&pool, foreign.tenant, foreign.principal).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn attachment_open_denies_same_tenant_unshared_file() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let (state, _, _) = setup_app_state(pool.clone()).await;
    let env = setup_env(&pool).await;

    // A second principal in the SAME tenant with no share on the file: the
    // file exists and the tenant matches, but Files permission (View) decides
    // at read time — the endpoint answers an existence-hiding 404.
    let stranger = PrincipalId::from(Uuid::new_v4());
    insert_user(&pool, env.tenant, stranger).await;
    let file = state
        .file_service
        .upload_file(
            env.principal.0,
            "private.txt".to_string(),
            None,
            bytes::Bytes::from_static(b"private bytes"),
            "text/plain".to_string(),
            env.tenant.0,
        )
        .await
        .expect("upload the test file");
    let request = || {
        Json(rustshare_server::handlers::chat_resource::ResourceRequest {
            resource: file_ref(&file.id.to_string()),
        })
    };

    let open = open_attachment(State(state.clone()), auth(stranger, env.tenant), request()).await;
    assert!(
        matches!(open, Err(AppError::NotFound(_))),
        "same-tenant open without a share must be an existence-hiding 404, got {open:?}"
    );
    let preview = preview_attachment(State(state), auth(stranger, env.tenant), request()).await;
    assert!(
        matches!(preview, Err(AppError::NotFound(_))),
        "same-tenant preview without a share must be an existence-hiding 404, got {preview:?}"
    );

    cleanup(&pool, env.tenant, env.principal).await;
}
