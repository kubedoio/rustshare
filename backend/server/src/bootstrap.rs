use crate::authz;
use crate::buzz_gateway::{BuzzGatewayAuthority, BuzzGatewayClient};
use crate::buzz_observation::BuzzObservationService;
use crate::config::{AppConfig, ChatProvisioningMode, OutboxWorkerConfig};
use crate::handlers::collab::CollabRooms;
use crate::handlers::ensure_optional_seed_user;
use crate::object_gc::{spawn_object_gc_worker, ObjectGcConfig};
use crate::oidc_runtime::{seed_oidc_config_from_env, OidcRuntimeCache};
use crate::outbox_dispatcher::OutboxDispatcher;
use crate::replication::{spawn_replication_worker, ReplicationWorkerConfig};
use crate::retention::{spawn_retention_cleanup_worker, RetentionConfig};
use crate::services::chat_bootstrap::ChatBootstrapService;
use crate::state::{AppAiService, AppState, AppUploadService, AppUserShareService};
use crate::trash_cleanup::{spawn_trash_cleanup_worker, TrashCleanupConfig};
use anyhow::Result;
use rand::RngExt;
use rustshare_auth::{JwtManager, PasswordHasher};
#[allow(deprecated)]
use rustshare_core::{
    domain::{ApplicationRegistry, User},
    events::EventBroadcaster,
    services::{
        AiService, ChatIntegrationService, ContentIndexer, FileService, FolderService,
        HttpWebhookDispatcher, IntegrationEventPublisher, NotificationService, PermissionResolver,
        ShareService, SimpleEmbeddingGenerator, ThumbnailService, UserShareService,
        UserShareServiceDeps, VaultSyncService,
    },
};
use rustshare_crypto::{SecretEncryptionKey, WebhookSigner};
use rustshare_infrastructure::{
    repositories::{
        FileRepository, FolderRepository, NotificationRepository, PermissionResolverRepository,
        ShareRepository, UserRepository,
    },
    PgVectorStore,
};
use rustshare_resource_auth::{BuzzAuthority, LocalFallbackAuthority};
use rustshare_storage::{
    repos::ShareNotificationRepoImpl, ChatIdentityStore, ChatObservationStore, EventStore,
    MemoryCatalogStore, MetadataStore, ObjectStore, OutboxStore,
};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::info;

struct Services {
    file_service:
        Arc<FileService<EventStore, MetadataStore, ObjectStore, PermissionResolverRepository>>,
    folder_service: Arc<FolderService<EventStore, MetadataStore, PermissionResolverRepository>>,
    share_service:
        Arc<ShareService<EventStore, MetadataStore, JwtManager, ShareNotificationRepoImpl>>,
    note_service: Arc<crate::services::note_service::NoteService>,
    decision_service: Arc<crate::services::decision_service::DecisionService>,
    meeting_service: Arc<crate::services::meeting_service::MeetingService>,
    standup_service: Arc<crate::services::standup_service::StandupService>,
    application_service: Arc<crate::services::application_service::ApplicationService>,
    template_service: Arc<crate::services::template_service::TemplateService>,
    kanban_service: Arc<crate::services::kanban_service::KanbanService>,
    brainstorming_service: Arc<crate::services::brainstorming_service::BrainstormingService>,
    thumbnail_service: Arc<ThumbnailService<ObjectStore>>,
    notification_service: Arc<NotificationService<NotificationRepository>>,
    user_share_service: Arc<AppUserShareService>,
    ai_service: Option<Arc<AppAiService>>,
    upload_service: Arc<AppUploadService>,
    user_repository: Arc<UserRepository>,
    vault_sync_service: Arc<VaultSyncService<MetadataStore, ObjectStore>>,
    chat_integration_service: Arc<crate::state::AppChatIntegrationService>,
    mail_service: Arc<crate::services::mail_service::MailService>,
    secret_key: Arc<SecretEncryptionKey>,
    application_registry: Arc<ApplicationRegistry>,
    outbox_store: Arc<OutboxStore>,
}

/// Buzz-mode chat runtime built in `init_app`: the shared gateway client, its
/// source-authority wrapper, and the zero-config bootstrap service (ADR-0036).
/// Local mode keeps the tuple's `Option`s `None`.
type ChatBuzzRuntime = (
    Option<Arc<BuzzGatewayClient>>,
    Box<dyn BuzzAuthority>,
    Option<Arc<ChatBootstrapService>>,
);

fn init_tracing(log_format: &str) {
    let env_filter =
        std::env::var("RUST_LOG").unwrap_or_else(|_| "info,rustshare=debug".to_string());

    match log_format {
        "json" => {
            tracing_subscriber::fmt()
                .json()
                .flatten_event(true)
                .with_current_span(true)
                .with_span_list(false)
                .with_env_filter(env_filter)
                .init();
        }
        _ => {
            tracing_subscriber::fmt()
                .pretty()
                .with_env_filter(env_filter)
                .init();
        }
    }
}

async fn init_database(config: &AppConfig) -> Result<PgPool> {
    let max_connections = config.db_pool_max_connections;
    let min_connections = config.db_pool_min_connections;
    let acquire_timeout_secs = config.db_pool_acquire_timeout_secs;
    let idle_timeout_secs = config.db_pool_idle_timeout_secs;
    let max_lifetime_secs = config.db_pool_max_lifetime_secs;

    let db_pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .min_connections(min_connections)
        .acquire_timeout(Duration::from_secs(acquire_timeout_secs))
        .idle_timeout(Some(Duration::from_secs(idle_timeout_secs)))
        .max_lifetime(Some(Duration::from_secs(max_lifetime_secs)))
        .before_acquire(|conn, _meta| {
            Box::pin(async move {
                // Reset RLS context variables to restrictive nil-UUID defaults on
                // every pool checkout. The per-request tenant context middleware
                // was removed; repository-level tenant filtering is the active
                // isolation mechanism.
                sqlx::query("SET app.current_tenant_id = '00000000-0000-0000-0000-000000000000'")
                    .execute(&mut *conn)
                    .await?;
                sqlx::query("SET app.current_user_id = '00000000-0000-0000-0000-000000000000'")
                    .execute(&mut *conn)
                    .await?;
                Ok(true)
            })
        })
        .connect(&config.database_url)
        .await?;
    info!(
        max_connections,
        min_connections,
        acquire_timeout_secs,
        idle_timeout_secs,
        max_lifetime_secs,
        "Database connection pool configured"
    );
    info!("Connected to database");
    sqlx::migrate!("../migrations").run(&db_pool).await?;
    info!("Database migrations applied");
    Ok(db_pool)
}

async fn init_blob_lock_pool(config: &AppConfig) -> Result<PgPool> {
    // Separate, small pool for PostgreSQL advisory locks on content-addressed
    // blobs. Isolating it from the main application pool prevents saturation
    // deadlocks where a writer holds a main-pool connection as a lock guard
    // while waiting for a second main-pool connection to persist metadata.
    // Capacity and wait policy are configurable so concurrent content-addressed
    // uploads queue instead of failing when every lock connection is busy.
    let max_connections = env_u32("RUSTSHARE_BLOB_LOCK_POOL_MAX_CONNECTIONS", 16);
    let acquire_timeout_secs = env_u64("RUSTSHARE_BLOB_LOCK_POOL_ACQUIRE_TIMEOUT_SECONDS", 30);

    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .min_connections(0)
        .acquire_timeout(Duration::from_secs(acquire_timeout_secs))
        .max_lifetime(Some(Duration::from_secs(3600)))
        .connect(&config.database_url)
        .await?;
    info!(
        max_connections,
        acquire_timeout_secs, "Blob lock pool configured"
    );
    Ok(pool)
}

fn env_u32(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

/// Replay-window (seconds) for incoming Buzz/Chat webhook signatures. Mirrors
/// `ChatIntegrationService`'s parsing of `RUSTSHARE_WEBHOOK_MAX_AGE_SECONDS`:
/// a positive integer, defaulting to 300; any other value is ignored.
fn chat_webhook_max_age_seconds() -> u64 {
    std::env::var("RUSTSHARE_WEBHOOK_MAX_AGE_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|&value| value >= 1)
        .unwrap_or(300)
}

async fn init_stores(
    config: &AppConfig,
    db_pool: PgPool,
    blob_lock_pool: PgPool,
) -> Result<(Arc<MetadataStore>, Arc<EventStore>, Arc<ObjectStore>)> {
    let rustfs_endpoint = config.rustfs_endpoint.clone();
    let rustfs_region = config.rustfs_region.clone();
    let rustfs_bucket = config.rustfs_bucket.clone();
    let object_store_options = rustshare_storage::ObjectStoreOptions {
        auto_create_bucket: config.object_store_auto_create_bucket,
    };

    let (metadata_store, event_store, object_store) = tokio::join!(
        async { Arc::new(MetadataStore::new(db_pool.clone())) },
        async { Arc::new(EventStore::new(db_pool.clone())) },
        async {
            ObjectStore::new_with_options(
                rustfs_endpoint,
                rustfs_region,
                rustfs_bucket,
                object_store_options,
            )
            .await
            .map(|store| store.with_blob_lock_pool(blob_lock_pool))
            .map(Arc::new)
        }
    );

    info!("Object store initialized");
    Ok((metadata_store, event_store, object_store?))
}

async fn init_jwt_manager(config: &AppConfig) -> Result<Arc<JwtManager>> {
    Ok(Arc::new(JwtManager::new(
        config.jwt_secret.clone(),
        config.jwt_issuer.clone(),
        config.jwt_audience.clone(),
        config.jwt_expiry_hours,
    )))
}

async fn init_broadcaster(config: &AppConfig) -> Result<Arc<EventBroadcaster>> {
    let capacity = config.broadcast_capacity;
    let broadcaster = Arc::new(EventBroadcaster::new(capacity));
    info!("EventBroadcaster initialized with capacity {}", capacity);
    Ok(broadcaster)
}

async fn init_repositories(
    db_pool: PgPool,
) -> Result<(
    Arc<PermissionResolverRepository>,
    NotificationRepository,
    Arc<ShareRepository>,
    Arc<UserRepository>,
    Arc<FileRepository>,
    Arc<FolderRepository>,
    Arc<PermissionResolver<PermissionResolverRepository>>,
)> {
    let notification_repository = NotificationRepository::new(db_pool.clone());
    let share_repository = Arc::new(ShareRepository::new(db_pool.clone()));
    let user_repository = Arc::new(UserRepository::new(db_pool.clone()));
    let file_repository = Arc::new(FileRepository::new(db_pool.clone()));
    let folder_repository = Arc::new(FolderRepository::new(db_pool.clone()));
    let permission_resolver_repository =
        Arc::new(PermissionResolverRepository::new(db_pool.clone()));
    let permission_resolver = Arc::new(PermissionResolver::new(Arc::clone(
        &permission_resolver_repository,
    )));
    Ok((
        permission_resolver_repository,
        notification_repository,
        share_repository,
        user_repository,
        file_repository,
        folder_repository,
        permission_resolver,
    ))
}

#[allow(clippy::too_many_arguments)]
async fn init_services(
    db_pool: PgPool,
    metadata_store: Arc<MetadataStore>,
    event_store: Arc<EventStore>,
    object_store: Arc<ObjectStore>,
    jwt_manager: Arc<JwtManager>,
    broadcaster: Arc<EventBroadcaster>,
    notification_repository: NotificationRepository,
    share_repository: Arc<ShareRepository>,
    user_repository: Arc<UserRepository>,
    file_repository: Arc<FileRepository>,
    folder_repository: Arc<FolderRepository>,
    permission_resolver: Arc<PermissionResolver<PermissionResolverRepository>>,
    secret_key: Arc<SecretEncryptionKey>,
    config: &AppConfig,
) -> Result<Services> {
    let application_registry =
        Arc::new(ApplicationRegistry::first_party().map_err(|error| {
            anyhow::anyhow!("invalid first-party Application manifest: {error}")
        })?);
    // Durable integration outbox (ADR-0031). The publisher is attached to
    // the FileService below so file mutations publish atomically with their
    // metadata transaction; the dispatcher loop (spawned later, gated on
    // RUSTSHARE_OUTBOX_WORKER_ENABLED) drains it asynchronously.
    let outbox_store = Arc::new(OutboxStore::new(
        db_pool.clone(),
        Arc::clone(&application_registry),
    ));
    let vault_sync_service = Arc::new(VaultSyncService::new(
        Arc::clone(&metadata_store),
        Arc::clone(&object_store),
    ));

    let (
        file_service,
        folder_service,
        share_notification_repo,
        thumbnail_service,
        notification_service,
    ) = tokio::join!(
        async {
            let publisher: Arc<
                dyn IntegrationEventPublisher<sqlx::Transaction<'static, sqlx::Postgres>>,
            > = outbox_store.clone();
            Arc::new(
                FileService::new(
                    Arc::clone(&event_store),
                    Arc::clone(&metadata_store),
                    Arc::clone(&object_store),
                    Arc::clone(&broadcaster),
                    Arc::clone(&permission_resolver),
                )
                .with_integration_publisher(publisher),
            )
        },
        async {
            Arc::new(FolderService::new(
                Arc::clone(&event_store),
                Arc::clone(&metadata_store),
                Arc::clone(&broadcaster),
                Arc::clone(&permission_resolver),
            ))
        },
        async { Arc::new(ShareNotificationRepoImpl::new(db_pool.clone())) },
        async {
            Arc::new(ThumbnailService::new(
                db_pool.clone(),
                Arc::clone(&object_store),
            ))
        },
        async { Arc::new(NotificationService::new(notification_repository)) },
    );

    let mail_service = Arc::new(crate::services::mail_service::MailService::new(
        Arc::clone(&metadata_store),
        Arc::clone(&object_store),
        Arc::clone(&file_service),
        Arc::clone(&folder_service),
        Arc::clone(&permission_resolver),
        Arc::clone(&event_store),
        Arc::clone(&broadcaster),
        Arc::clone(&secret_key),
    ));

    // Shared content indexer used both by the AI service and by the note
    // service's indexing callback sink. Kept outside the tokio::join! so both
    // services can be wired to the same in-memory index.
    let ai_service_enabled = config.ai_enabled;
    let shared_content_indexer: Option<Arc<ContentIndexer<SimpleEmbeddingGenerator>>> =
        if ai_service_enabled {
            let embedding_generator = Arc::new(SimpleEmbeddingGenerator::new());
            let store = Arc::new(PgVectorStore::new(db_pool.clone()));
            Some(Arc::new(ContentIndexer::new(embedding_generator, store)))
        } else {
            None
        };

    let note_index_sink: Option<Arc<dyn crate::services::note_index_sink::NoteIndexSink>> =
        shared_content_indexer.as_ref().map(|indexer| {
            Arc::new(
                crate::services::note_index_sink::ContentIndexerNoteSink::new(Arc::clone(indexer)),
            ) as Arc<dyn crate::services::note_index_sink::NoteIndexSink>
        });

    let (
        share_service,
        note_service,
        decision_service,
        meeting_service,
        standup_service,
        application_service,
        template_service,
        kanban_service,
        brainstorming_service,
    ) = tokio::join!(
        async {
            Arc::new(ShareService::new(
                Arc::clone(&event_store),
                Arc::clone(&metadata_store),
                Arc::clone(&broadcaster),
                Arc::clone(&jwt_manager),
                Arc::clone(&share_notification_repo),
            ))
        },
        async {
            Arc::new(
                crate::services::note_service::NoteService::new(
                    Arc::clone(&file_service),
                    Arc::clone(&folder_service),
                    Arc::clone(&metadata_store),
                    Arc::clone(&object_store),
                    Arc::clone(&permission_resolver),
                    db_pool.clone(),
                )
                .with_index_sink(note_index_sink),
            )
        },
        async {
            Arc::new(crate::services::decision_service::DecisionService::new(
                Arc::clone(&file_service),
                Arc::clone(&folder_service),
                Arc::clone(&metadata_store),
                Arc::clone(&object_store),
            ))
        },
        async {
            Arc::new(crate::services::meeting_service::MeetingService::new(
                Arc::clone(&file_service),
                Arc::clone(&folder_service),
                Arc::clone(&metadata_store),
                Arc::clone(&object_store),
            ))
        },
        async {
            Arc::new(crate::services::standup_service::StandupService::new(
                Arc::clone(&file_service),
                Arc::clone(&folder_service),
                Arc::clone(&metadata_store),
                Arc::clone(&object_store),
            ))
        },
        async {
            Arc::new(
                crate::services::application_service::ApplicationService::with_registry(
                    Arc::clone(&folder_service),
                    Arc::clone(&metadata_store),
                    Arc::clone(&application_registry),
                ),
            )
        },
        async {
            Arc::new(
                crate::services::template_service::TemplateService::with_registry(
                    Arc::clone(&file_service),
                    Arc::clone(&folder_service),
                    Arc::clone(&metadata_store),
                    Arc::clone(&application_registry),
                ),
            )
        },
        async {
            Arc::new(crate::services::kanban_service::KanbanService::new(
                Arc::clone(&file_service),
                Arc::clone(&folder_service),
                Arc::clone(&metadata_store),
                Arc::clone(&object_store),
                Arc::clone(&user_repository),
            ))
        },
        async {
            Arc::new(
                crate::services::brainstorming_service::BrainstormingService::new(
                    Arc::clone(&file_service),
                    Arc::clone(&folder_service),
                    Arc::clone(&metadata_store),
                    Arc::clone(&object_store),
                ),
            )
        },
    );

    #[allow(deprecated)]
    let user_share_service = Arc::new(UserShareService::new(UserShareServiceDeps {
        share_repo: Arc::clone(&share_repository),
        user_repo: Arc::clone(&user_repository),
        file_repo: Arc::clone(&file_repository),
        folder_repo: Arc::clone(&folder_repository),
        permission_resolver: Arc::clone(&permission_resolver),
        notification_service: Arc::clone(&notification_service),
        event_store: Arc::clone(&event_store),
        broadcaster: Arc::clone(&broadcaster),
    }));

    let ai_service: Option<Arc<AppAiService>> =
        if let Some(content_indexer) = shared_content_indexer {
            Some(Arc::new(AiService::new(
                content_indexer,
                Arc::clone(&permission_resolver),
            )))
        } else {
            None
        };

    if ai_service.is_some() {
        info!("AI service initialized");
    } else {
        info!("AI service disabled");
    }

    let upload_doc_store_path = std::env::var("RUSTSHARE_UPLOAD_STORE_PATH")
        .unwrap_or_else(|_| "/tmp/rustshare-uploads".to_string());

    let upload_backend_config = rustshare_storage::upload_doc_store::MetadataBackendConfig {
        base_prefix: "apps/rustshare".to_string(),
        namespace: "uploads".to_string(),
    };

    let upload_doc_store: Arc<rustshare_storage::upload_doc_store::LocalFsDocumentStore> = Arc::new(
        rustshare_storage::upload_doc_store::LocalFsDocumentStore::new(
            std::path::PathBuf::from(&upload_doc_store_path),
            upload_backend_config,
        ),
    );

    let upload_session_repo = rustshare_storage::repos::RustFsUploadSessionRepository::new(
        upload_doc_store,
        "apps/rustshare".to_string(),
        "uploads".to_string(),
    );

    let upload_service = Arc::new(AppUploadService::new(
        Arc::new(upload_session_repo),
        Arc::clone(&object_store),
        Arc::clone(&metadata_store),
        Arc::clone(&event_store),
        Arc::clone(&broadcaster),
    ));

    info!(
        "Upload service initialized with store path: {}",
        upload_doc_store_path
    );

    let chat_webhook_secret = config.rustshare_chat_webhook_secret.clone();
    let chat_integration_service = Arc::new(ChatIntegrationService::new(
        Arc::clone(&metadata_store),
        Arc::clone(&event_store),
        Arc::clone(&broadcaster),
        chat_webhook_secret,
        Arc::new(HttpWebhookDispatcher::new()),
    ));
    info!("Chat integration service initialized");

    Ok(Services {
        file_service,
        folder_service,
        share_service,
        note_service,
        decision_service,
        meeting_service,
        standup_service,
        application_service,
        template_service,
        kanban_service,
        brainstorming_service,
        thumbnail_service,
        notification_service,
        user_share_service,
        ai_service,
        upload_service,
        user_repository,
        vault_sync_service,
        chat_integration_service,
        mail_service,
        secret_key,
        application_registry,
        outbox_store,
    })
}

pub async fn init_app() -> Result<AppState> {
    dotenvy::dotenv().ok();

    let config = match AppConfig::from_env() {
        Ok(c) => c,
        Err(errors) => {
            eprintln!("\n❌ Configuration errors — server cannot start:\n");
            for error in &errors {
                eprintln!("  ✗ {}", error);
            }
            eprintln!();
            return Err(anyhow::anyhow!("Configuration invalid"));
        }
    };

    init_tracing(&config.log_format);

    info!("Starting RustShare server");

    let db_pool = init_database(&config).await?;
    let blob_lock_pool = init_blob_lock_pool(&config).await?;

    let (metadata_store, event_store, object_store) =
        init_stores(&config, db_pool.clone(), blob_lock_pool).await?;

    let jwt_manager = init_jwt_manager(&config).await?;

    let broadcaster = init_broadcaster(&config).await?;

    let (
        permission_resolver_repository,
        notification_repository,
        share_repository,
        user_repository,
        file_repository,
        folder_repository,
        permission_resolver,
    ) = init_repositories(db_pool.clone()).await?;

    let encryption_key = std::env::var("RUSTSHARE_SECRET_ENCRYPTION_KEY").unwrap_or_default();
    if encryption_key == "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=" {
        return Err(anyhow::anyhow!(
            "RUSTSHARE_SECRET_ENCRYPTION_KEY is using a known weak default value. Generate a strong key with: openssl rand -base64 32"
        ));
    }
    let secret_key = Arc::new(
        SecretEncryptionKey::from_env()
            .map_err(|e| anyhow::anyhow!("Secret encryption key error: {}", e))?,
    );

    let services = init_services(
        db_pool.clone(),
        Arc::clone(&metadata_store),
        Arc::clone(&event_store),
        Arc::clone(&object_store),
        Arc::clone(&jwt_manager),
        Arc::clone(&broadcaster),
        notification_repository,
        Arc::clone(&share_repository),
        Arc::clone(&user_repository),
        Arc::clone(&file_repository),
        Arc::clone(&folder_repository),
        Arc::clone(&permission_resolver),
        Arc::clone(&secret_key),
        &config,
    )
    .await?;

    let mail_service = Arc::clone(&services.mail_service);

    // Buzz → Elembra Memory projection (ADR-0033/ADR-0034), observation half:
    // the authenticated ingestion path for signed Buzz chat events. The
    // stores are shared with the later Memory consumer via AppState.
    let chat_identity_store = Arc::new(ChatIdentityStore::new(db_pool.clone()));
    let chat_observation_store = Arc::new(ChatObservationStore::new(db_pool.clone()));
    // The catalog is wired with the observation index so the projection
    // consumer can enforce the tombstone-before-create delivery guard in
    // `upsert_from_event_in_tx` (a store without it fails closed).
    let memory_catalog_store = Arc::new(MemoryCatalogStore::with_observation_store(
        db_pool.clone(),
        (*chat_observation_store).clone(),
    ));
    let buzz_observation_service = Arc::new(BuzzObservationService::new(
        db_pool.clone(),
        (*chat_identity_store).clone(),
        (*chat_observation_store).clone(),
        Arc::clone(&services.outbox_store),
        WebhookSigner::new(config.rustshare_chat_webhook_secret.clone()),
        chat_webhook_max_age_seconds(),
        Arc::clone(&broadcaster),
    ));

    let rate_limit_config = Arc::new(crate::middleware::RateLimitConfig::new());
    info!("Rate limiting initialized");

    let default_tenant_id = std::env::var("RUSTSHARE_DEFAULT_TENANT_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(uuid::Uuid::nil);

    let prometheus_handle = crate::metrics::init_metrics();
    info!("Prometheus metrics recorder installed");

    let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);

    let object_gc_config = ObjectGcConfig::from_env()?;
    spawn_object_gc_worker(
        Arc::clone(&metadata_store),
        Arc::clone(&object_store),
        object_gc_config,
        shutdown_tx.subscribe(),
    );

    let replication_worker_config = ReplicationWorkerConfig::from_env();
    spawn_replication_worker(
        Arc::clone(&metadata_store),
        Arc::clone(&object_store),
        Arc::clone(&event_store),
        Arc::clone(&broadcaster),
        replication_worker_config,
        shutdown_tx.subscribe(),
    );

    let trash_cleanup_config = TrashCleanupConfig::from_env();
    spawn_trash_cleanup_worker(
        Arc::clone(&metadata_store),
        services.ai_service.clone(),
        trash_cleanup_config,
        shutdown_tx.subscribe(),
    );

    let retention_config = RetentionConfig::from_env();
    spawn_retention_cleanup_worker(
        Arc::clone(&metadata_store),
        Arc::clone(&object_store),
        retention_config,
        shutdown_tx.subscribe(),
    );

    // Durable integration-event outbox dispatcher (ADR-0031). Publishing
    // into the outbox is always active (attached to FileService above); only
    // the drain loop is gated on RUSTSHARE_OUTBOX_WORKER_ENABLED — a
    // disabled worker just accumulates events until re-enabled.
    //
    // Consumers register themselves (durable consumer registration,
    // `integration_consumers` / `integration_consumer_subscriptions`) at the
    // start of every dispatcher tick; registration is future-only, so a
    // consumer must be present in the Vec before any event it should consume
    // is published. The Memory chat projection consumer is always registered
    // (it consumes the Chat observation events this process publishes); the
    // Buzz admission bridge is optional (disabled unless a service key is
    // configured).
    let memory_projection_consumer =
        Arc::new(crate::memory_projection::MemoryChatProjectionConsumer::new(
            db_pool.clone(),
            (*chat_identity_store).clone(),
            (*chat_observation_store).clone(),
            (*memory_catalog_store).clone(),
        ));
    let outbox_consumers: Vec<Arc<dyn rustshare_integration_events::OutboxConsumer>> =
        crate::buzz_bridge::BuzzAdmissionBridge::from_env()
            .map(|consumer| {
                Arc::new(consumer) as Arc<dyn rustshare_integration_events::OutboxConsumer>
            })
            .into_iter()
            .chain(std::iter::once(
                memory_projection_consumer as Arc<dyn rustshare_integration_events::OutboxConsumer>,
            ))
            .collect();
    let outbox_worker_config = OutboxWorkerConfig::from_env();
    let outbox_dispatcher = Arc::new(OutboxDispatcher::new(
        Arc::clone(&services.outbox_store),
        outbox_consumers,
        outbox_worker_config.clone(),
        format!("outbox-{}", uuid::Uuid::new_v4()),
    ));
    let outbox_status = outbox_dispatcher.status().clone();
    if outbox_worker_config.enabled {
        outbox_dispatcher.spawn(shutdown_tx.subscribe());
    } else {
        info!("Outbox worker disabled; integration events still publish into the outbox");
    }

    if config.mail_import_worker_enabled {
        crate::mail_import_worker::spawn_mail_import_worker(
            Arc::clone(&mail_service),
            Arc::clone(&metadata_store),
            shutdown_tx.subscribe(),
            crate::mail_import_worker::MailImportWorkerConfig::from_config(&config),
        );
    } else {
        info!("Mail import worker disabled");
    }

    if !metadata_store.has_users().await? {
        let admin_username = std::env::var("RUSTSHARE_ADMIN_USERNAME")?;
        let admin_email = std::env::var("RUSTSHARE_ADMIN_EMAIL")?;

        let (admin_password, is_user_provided_password) =
            resolve_admin_password(std::env::var("RUSTSHARE_ADMIN_PASSWORD").ok());

        let password_hash = PasswordHasher::hash(&admin_password)?;
        let admin_user = User::new(
            admin_username.clone(),
            "Administrator".to_string(),
            password_hash,
            admin_email.clone(),
            true,
            config.default_storage_quota_bytes,
            default_tenant_id,
        );

        metadata_store.create_user(&admin_user).await?;

        let pref_repo =
            rustshare_infrastructure::repositories::ApplicationUserPreferenceRepository::new(
                db_pool.clone(),
            );
        if let Err(e) = pref_repo.seed_defaults(admin_user.id).await {
            tracing::warn!(
                "Failed to seed default Application preferences for admin: {:?}",
                e
            );
        }

        if is_user_provided_password {
            info!("Bootstrap admin user created with user-provided password.");
        } else {
            let password_file = config.bootstrap_password_file.clone();
            write_bootstrap_password_file(std::path::Path::new(&password_file), &admin_password)?;
            info!(path = %password_file, "Bootstrap admin password written to secure file. Change immediately.");
        }
    }

    ensure_optional_seed_user(
        &metadata_store,
        "RUSTSHARE_DEMO_VIEWER_USERNAME",
        "RUSTSHARE_DEMO_VIEWER_EMAIL",
        "RUSTSHARE_DEMO_VIEWER_PASSWORD",
        std::env::var("RUSTSHARE_DEMO_VIEWER_DISPLAY_NAME")
            .unwrap_or_else(|_| "Viewer User".to_string()),
        false,
        default_tenant_id,
    )
    .await?;

    services
        .application_service
        .ensure_default_applications(default_tenant_id)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to seed default Applications: {}", e))?;
    services
        .template_service
        .ensure_default_templates(default_tenant_id)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to seed default templates: {}", e))?;
    info!("Default Applications and templates seeded");

    if seed_oidc_config_from_env(&db_pool, &services.secret_key).await? {
        info!("Seeded initial OIDC config from environment bootstrap values");
    }

    let public_base_url = config.public_url.clone();

    let pool_clone = db_pool.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
        loop {
            interval.tick().await;
            let idle = pool_clone.num_idle() as f64;
            let total = pool_clone.size() as f64;
            metrics::gauge!("db_pool_idle_connections").set(idle);
            metrics::gauge!("db_pool_active_connections").set(total - idle);
        }
    });

    // Buzz source-authority gateway (config-driven, fail closed): when
    // `rustshare_chat_authority` is `buzz`, the Chat owner's FINAL
    // channel/message decisions go to the community's authoritative relay
    // through this client. Config validation guarantees the bridge secret
    // key exists and parses in buzz mode; any failure here is a startup
    // error — never a silent fallback to local.
    let (buzz_gateway, chat_buzz_authority, chat_bootstrap): ChatBuzzRuntime = if config
        .rustshare_chat_authority
        == "buzz"
    {
        let key = config
            .rustshare_chat_bridge_secret_key
            .as_deref()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "RUSTSHARE_CHAT_AUTHORITY is 'buzz' but RUSTSHARE_CHAT_BRIDGE_SECRET_KEY is not set"
                )
            })?;
        let keys = nostr::Keys::parse(key).map_err(|error| {
            anyhow::anyhow!("invalid RUSTSHARE_CHAT_BRIDGE_SECRET_KEY: {error}")
        })?;
        // One client instance is shared: the `BuzzGatewayAuthority` wrapper
        // presents the same stateless client (service key + HTTP client)
        // stored in AppState to the source authorizer.
        let gateway = Arc::new(
            BuzzGatewayClient::new(keys, reqwest::ClientBuilder::new())
                .map_err(|error| anyhow::anyhow!("cannot build Buzz gateway client: {error}"))?,
        );
        let authority: Box<dyn BuzzAuthority> =
            Box::new(BuzzGatewayAuthority(Arc::clone(&gateway)));
        // Zero-config bootstrap service (ADR-0036) — only in buzz mode.
        let chat_bootstrap = Some(Arc::new(ChatBootstrapService::new(
            gateway.clone(),
            chat_identity_store.clone(),
            config
                .rustshare_chat_bootstrap_relay_url
                .clone()
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL required for chat provisioning"
                    )
                })?,
        )));
        info!("Chat authority mode: buzz (relay-backed gateway)");
        (Some(gateway), authority, chat_bootstrap)
    } else {
        info!("Chat authority mode: local (coarse workspace-only gate)");
        (None, Box::new(LocalFallbackAuthority), None)
    };

    let (source_authorizer, chat_owner) = authz::build_source_authorizer(
        Arc::clone(&services.application_registry),
        Arc::clone(&permission_resolver),
        Arc::clone(&permission_resolver_repository),
        Arc::clone(&metadata_store),
        Arc::clone(&object_store),
        (*chat_identity_store).clone(),
        (*chat_observation_store).clone(),
        chat_buzz_authority,
    )
    .map_err(|error| anyhow::anyhow!("source owner registration failed: {error}"))?;
    let source_authorizer = Arc::new(source_authorizer);

    // Permission-aware unified search: candidates come from Files metadata,
    // the note index and the Memory catalog; final inclusion is gated by the
    // source authorizer (Files → PermissionResolver; Chat → BuzzAuthority).
    let unified_search_service =
        Arc::new(crate::services::unified_search::UnifiedSearchService::new(
            Arc::clone(&source_authorizer),
            Arc::clone(&metadata_store),
            services.ai_service.clone(),
            Arc::clone(&memory_catalog_store),
        ));
    let llm_provider = crate::services::ask_workspace::OpenAiCompatibleProvider::from_env()
        .map_err(|error| anyhow::anyhow!("LLM provider configuration failed: {error}"))?;
    let ask_workspace_service = Arc::new(crate::services::ask_workspace::AskWorkspaceService::new(
        Arc::clone(&unified_search_service),
        llm_provider,
    ));

    let state = AppState {
        db_pool,
        metadata_store,
        event_store,
        object_store,
        jwt_manager,
        broadcaster,
        file_service: services.file_service,
        folder_service: services.folder_service,
        share_service: services.share_service,
        thumbnail_service: services.thumbnail_service,
        permission_resolver,
        source_authorizer,
        notification_service: services.notification_service,
        user_share_service: services.user_share_service,
        ai_service: services.ai_service,
        upload_service: Some(services.upload_service),
        rate_limit_config,
        secret_key: (*services.secret_key).clone(),
        oidc_runtime_cache: OidcRuntimeCache::new(),
        poll_rate_limiter: Arc::new(Mutex::new(HashMap::new())),
        default_tenant_id,
        note_service: services.note_service,
        decision_service: services.decision_service,
        meeting_service: services.meeting_service,
        standup_service: services.standup_service,
        application_service: services.application_service,
        template_service: services.template_service,
        kanban_service: services.kanban_service,
        brainstorming_service: services.brainstorming_service,
        user_repository: services.user_repository,
        public_base_url,
        collab_rooms: Arc::new(CollabRooms::new()),
        vault_sync_service: services.vault_sync_service,
        chat_integration_service: services.chat_integration_service,
        mail_service: services.mail_service,
        outbox_store: services.outbox_store,
        chat_observation_store,
        memory_catalog_store,
        unified_search_service,
        ask_workspace_service,
        buzz_observation_service,
        chat_owner,
        buzz_gateway,
        chat_bootstrap,
        chat_provisioning: ChatProvisioningMode::parse(&config.rustshare_chat_provisioning)
            .map_err(|message| anyhow::anyhow!(message))?,
        outbox_status,
        outbox_worker_enabled: outbox_worker_config.enabled,
        outbox_readiness_staleness_secs: outbox_worker_config.readiness_staleness_secs,
        shutdown_tx,
        prometheus_handle,
    };

    Ok(state)
}

/// Resolve the bootstrap admin password from the environment value.
///
/// Returns the user-provided password and `true` when one is configured, or a
/// newly generated random password and `false` otherwise. Empty or
/// whitespace-only values are treated as unset because Docker Compose forwards
/// `${RUSTSHARE_ADMIN_PASSWORD}` references as set-but-empty when `.env`
/// leaves the variable blank.
fn resolve_admin_password(env_value: Option<String>) -> (String, bool) {
    match env_value.filter(|pwd| !pwd.trim().is_empty()) {
        Some(pwd) => (pwd, true),
        None => {
            const CHARSET: &[u8] =
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
            let mut rng = rand::rng();
            let password: String = (0..32)
                .map(|_| CHARSET[rng.random_range(0..CHARSET.len())] as char)
                .collect();
            (password, false)
        }
    }
}

/// Write the generated bootstrap admin password to a secure file.
///
/// The file is created with restrictive permissions (0600 on Unix) and the
/// password bytes are never logged to stdout/stderr by this helper.
pub fn write_bootstrap_password_file(path: &std::path::Path, password: &str) -> Result<()> {
    use std::io::Write;

    let mut file = std::fs::File::create(path)?;
    file.write_all(password.as_bytes())?;
    file.sync_all()?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(path)?.permissions();
        permissions.set_mode(0o600);
        std::fs::set_permissions(path, permissions)?;
    }

    Ok(())
}

pub fn default_storage_quota_bytes() -> i64 {
    std::env::var("RUSTSHARE_DEFAULT_STORAGE_QUOTA_BYTES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(10_737_418_240)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_admin_password_uses_configured_password() {
        let (password, is_user_provided) =
            resolve_admin_password(Some("my-configured-password".to_string()));
        assert_eq!(password, "my-configured-password");
        assert!(is_user_provided);
    }

    #[test]
    fn resolve_admin_password_generates_when_unset() {
        let (password, is_user_provided) = resolve_admin_password(None);
        assert_eq!(password.len(), 32);
        assert!(!is_user_provided);
    }

    #[test]
    fn resolve_admin_password_generates_when_empty_or_whitespace() {
        // Docker Compose forwards `${RUSTSHARE_ADMIN_PASSWORD}` as set-but-empty
        // when `.env` leaves it blank; std::env::var then returns Ok("").
        for blank in ["", "   ", "\t\n"] {
            let (password, is_user_provided) = resolve_admin_password(Some(blank.to_string()));
            assert_eq!(password.len(), 32, "blank value {blank:?} must be unset");
            assert!(
                !is_user_provided,
                "blank value {blank:?} must trigger generation"
            );
        }
    }

    #[test]
    fn bootstrap_password_file_is_written_with_restrictive_permissions() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("bootstrap-password.txt");
        let password = "super-secret-generated-password-12345";

        write_bootstrap_password_file(&path, password).expect("write password file");

        let contents = std::fs::read_to_string(&path).expect("read password file");
        assert_eq!(
            contents, password,
            "password file must contain the generated password"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path)
                .expect("get password file metadata")
                .permissions()
                .mode();
            assert_eq!(
                mode & 0o777,
                0o600,
                "password file must be readable only by the owner"
            );
        }
    }

    #[test]
    fn bootstrap_password_file_overwrites_existing_file() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("bootstrap-password.txt");
        std::fs::write(&path, "old-password").expect("write old password");

        let new_password = "new-super-secret-password";
        write_bootstrap_password_file(&path, new_password).expect("write password file");

        let contents = std::fs::read_to_string(&path).expect("read password file");
        assert_eq!(contents, new_password);
    }
}
