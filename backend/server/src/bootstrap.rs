use crate::handlers::ensure_optional_seed_user;
use crate::oidc_runtime::{seed_oidc_config_from_env, OidcRuntimeCache};
use crate::replication::{spawn_replication_worker, ReplicationWorkerConfig};
use crate::state::{AppAiService, AppState, AppUploadService, AppUserShareService};
use crate::trash_cleanup::{spawn_trash_cleanup_worker, TrashCleanupConfig};
use anyhow::Result;
use rand::Rng;
use rustshare_auth::{JwtManager, PasswordHasher};
#[allow(deprecated)]
use rustshare_core::{
    domain::User,
    events::EventBroadcaster,
    services::{
        AiService, ContentIndexer, FileService, FolderService, NotificationService,
        PermissionResolver, ShareService, SimpleEmbeddingGenerator, ThumbnailService,
        UserShareService, UserShareServiceDeps,
    },
};
use rustshare_crypto::SecretEncryptionKey;
use rustshare_infrastructure::repositories::{
    FileRepository, FolderRepository, NotificationRepository, PermissionResolverRepository,
    ShareRepository, UserRepository,
};
use rustshare_storage::{repos::ShareNotificationRepoImpl, EventStore, MetadataStore, ObjectStore};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
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
    module_service: Arc<crate::services::module_service::ModuleService>,
    template_service: Arc<crate::services::template_service::TemplateService>,
    kanban_service: Arc<crate::services::kanban_service::KanbanService>,
    brainstorming_service: Arc<crate::services::brainstorming_service::BrainstormingService>,
    thumbnail_service: Arc<ThumbnailService<ObjectStore>>,
    notification_service: Arc<NotificationService<NotificationRepository>>,
    user_share_service: Arc<AppUserShareService>,
    ai_service: Option<Arc<AppAiService>>,
    upload_service: Arc<AppUploadService>,
    user_repository: Arc<UserRepository>,
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,rustshare=debug".to_string()),
        )
        .init();
}

async fn init_database() -> Result<PgPool> {
    let database_url = std::env::var("DATABASE_URL")?;
    let db_pool = PgPoolOptions::new()
        .before_acquire(|conn, _meta| {
            Box::pin(async move {
                // Set a default restrictive user context.
                // This will be overridden per-request by middleware.
                sqlx::query(
                    "SET LOCAL app.current_user_id = '00000000-0000-0000-0000-000000000000'"
                )
                .execute(&mut *conn)
                .await?;
                Ok(true)
            })
        })
        .connect(&database_url)
        .await?;
    info!("Connected to database");
    sqlx::migrate!("../migrations").run(&db_pool).await?;
    info!("Database migrations applied");
    Ok(db_pool)
}

async fn init_stores(
    db_pool: PgPool,
) -> Result<(Arc<MetadataStore>, Arc<EventStore>, Arc<ObjectStore>)> {
    let rustfs_endpoint = std::env::var("RUSTFS_ENDPOINT")?;
    let rustfs_region = std::env::var("RUSTFS_REGION")?;
    let rustfs_bucket = std::env::var("RUSTFS_BUCKET")?;

    let (metadata_store, event_store, object_store) = tokio::join!(
        async { Arc::new(MetadataStore::new(db_pool.clone())) },
        async { Arc::new(EventStore::new(db_pool.clone())) },
        async {
            ObjectStore::new(rustfs_endpoint, rustfs_region, rustfs_bucket)
                .await
                .map(Arc::new)
        }
    );

    info!("Object store initialized");
    Ok((metadata_store, event_store, object_store?))
}

async fn init_jwt_manager() -> Result<Arc<JwtManager>> {
    let jwt_secret = std::env::var("JWT_SECRET")?;
    if jwt_secret.len() < 32 {
        return Err(anyhow::anyhow!(
            "JWT_SECRET must be at least 32 characters long. Generate one with: openssl rand -base64 32"
        ));
    }
    if jwt_secret == "dev-secret-change-in-production"
        || jwt_secret == "dev-secret-key-change-in-production-12345"
        || jwt_secret == "ci-pilot-secret"
    {
        return Err(anyhow::anyhow!(
            "JWT_SECRET is using a known weak default value. Generate a strong secret with: openssl rand -base64 32"
        ));
    }
    Ok(Arc::new(JwtManager::new(jwt_secret)))
}

async fn init_broadcaster() -> Result<Arc<EventBroadcaster>> {
    let capacity = std::env::var("BROADCAST_CAPACITY")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);
    let broadcaster = Arc::new(EventBroadcaster::new(capacity));
    info!("EventBroadcaster initialized with capacity {}", capacity);
    Ok(broadcaster)
}

async fn init_repositories(
    db_pool: PgPool,
) -> Result<(
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
    let permission_resolver = Arc::new(PermissionResolver::new(Arc::new(
        PermissionResolverRepository::new(db_pool.clone()),
    )));
    Ok((
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
) -> Result<Services> {
    let (
        file_service,
        folder_service,
        share_notification_repo,
        thumbnail_service,
        notification_service,
    ) = tokio::join!(
        async {
            Arc::new(FileService::new(
                Arc::clone(&event_store),
                Arc::clone(&metadata_store),
                Arc::clone(&object_store),
                Arc::clone(&broadcaster),
                Arc::clone(&permission_resolver),
            ))
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

    let (
        share_service,
        note_service,
        decision_service,
        meeting_service,
        standup_service,
        module_service,
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
            Arc::new(crate::services::note_service::NoteService::new(
                Arc::clone(&file_service),
                Arc::clone(&folder_service),
                Arc::clone(&metadata_store),
                Arc::clone(&object_store),
            ))
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
            Arc::new(crate::services::module_service::ModuleService::new(
                Arc::clone(&folder_service),
                Arc::clone(&metadata_store),
            ))
        },
        async {
            Arc::new(crate::services::template_service::TemplateService::new(
                Arc::clone(&file_service),
                Arc::clone(&folder_service),
                Arc::clone(&metadata_store),
            ))
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

    let ai_service_enabled = std::env::var("RUSTSHARE_AI_ENABLED")
        .ok()
        .and_then(|s| s.parse::<bool>().ok())
        .unwrap_or(true);

    let ai_service: Option<Arc<AppAiService>> = if ai_service_enabled {
        let embedding_generator = Arc::new(SimpleEmbeddingGenerator::new());
        let content_indexer = Arc::new(ContentIndexer::new(embedding_generator));
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

    let upload_backend_config = rustshare_storage::metadata_v2::MetadataBackendConfig {
        base_prefix: "apps/rustshare".to_string(),
        namespace: "uploads".to_string(),
        enable_optimistic_concurrency: true,
        fallback_to_leases: true,
    };

    let upload_doc_store: Arc<dyn rustshare_storage::metadata_v2::MetadataDocumentStore> = Arc::new(
        rustshare_storage::metadata_v2::stores::LocalFsDocumentStore::new(
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
        Arc::new(crate::adapters::UploadObjectStoreAdapter::new(Arc::clone(
            &object_store,
        ))),
        Arc::new(crate::adapters::UploadMetadataStoreAdapter::new(
            Arc::clone(&metadata_store),
        )),
        Arc::clone(&event_store),
        Arc::clone(&broadcaster),
    ));

    info!(
        "Upload service initialized with store path: {}",
        upload_doc_store_path
    );

    Ok(Services {
        file_service,
        folder_service,
        share_service,
        note_service,
        decision_service,
        meeting_service,
        standup_service,
        module_service,
        template_service,
        kanban_service,
        brainstorming_service,
        thumbnail_service,
        notification_service,
        user_share_service,
        ai_service,
        upload_service,
        user_repository,
    })
}

pub async fn init_app() -> Result<AppState> {
    dotenvy::dotenv().ok();

    init_tracing();

    info!("Starting RustShare server");

    let db_pool = init_database().await?;

    let (metadata_store, event_store, object_store) = init_stores(db_pool.clone()).await?;

    let jwt_manager = init_jwt_manager().await?;

    let broadcaster = init_broadcaster().await?;

    let (
        notification_repository,
        share_repository,
        user_repository,
        file_repository,
        folder_repository,
        permission_resolver,
    ) = init_repositories(db_pool.clone()).await?;

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
    )
    .await?;

    let rate_limit_config = Arc::new(crate::middleware::RateLimitConfig::new());
    info!("Rate limiting initialized");

    let default_tenant_id = std::env::var("RUSTSHARE_DEFAULT_TENANT_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(uuid::Uuid::nil);

    let replication_worker_config = ReplicationWorkerConfig::from_env();
    spawn_replication_worker(
        Arc::clone(&metadata_store),
        Arc::clone(&object_store),
        Arc::clone(&event_store),
        Arc::clone(&broadcaster),
        replication_worker_config,
    );

    let trash_cleanup_config = TrashCleanupConfig::from_env();
    spawn_trash_cleanup_worker(Arc::clone(&metadata_store), trash_cleanup_config);

    if !metadata_store.has_users().await? {
        let admin_username = std::env::var("RUSTSHARE_ADMIN_USERNAME")?;
        let admin_email = std::env::var("RUSTSHARE_ADMIN_EMAIL")?;

        let admin_password = match std::env::var("RUSTSHARE_ADMIN_PASSWORD") {
            Ok(pwd) => pwd,
            Err(_) => {
                const CHARSET: &[u8] =
                    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
                let mut rng = rand::thread_rng();
                let password: String = (0..32)
                    .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
                    .collect();
                password
            }
        };

        let password_hash = PasswordHasher::hash(&admin_password)?;
        let admin_user = User::new(
            admin_username.clone(),
            "Administrator".to_string(),
            password_hash,
            admin_email.clone(),
            true,
            default_storage_quota_bytes(),
            default_tenant_id,
        );

        metadata_store.create_user(&admin_user).await?;

        let pref_repo = rustshare_infrastructure::repositories::UserModulePreferenceRepository::new(
            db_pool.clone(),
        );
        if let Err(e) = pref_repo.seed_defaults(admin_user.id).await {
            tracing::warn!(
                "Failed to seed default module preferences for admin: {:?}",
                e
            );
        }

        info!("╔══════════════════════════════════════════════════════════════════╗");
        info!("║  BOOTSTRAP ADMIN PASSWORD                                        ║");
        info!("╠══════════════════════════════════════════════════════════════════╣");
        info!("║  Email:    {:<53} ║", admin_email);
        info!("║  Password: {:<53} ║", admin_password);
        info!("╚══════════════════════════════════════════════════════════════════╝");
        info!("Log in and change this password immediately. It will not be shown again.");
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
        .module_service
        .ensure_default_modules(default_tenant_id)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to seed default modules: {}", e))?;
    services
        .template_service
        .ensure_default_templates(default_tenant_id)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to seed default templates: {}", e))?;
    info!("Default modules and templates seeded");

    let encryption_key = std::env::var("RUSTSHARE_SECRET_ENCRYPTION_KEY").unwrap_or_default();
    if encryption_key == "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=" {
        return Err(anyhow::anyhow!(
            "RUSTSHARE_SECRET_ENCRYPTION_KEY is using a known weak default value. Generate a strong key with: openssl rand -base64 32"
        ));
    }
    let secret_key = SecretEncryptionKey::from_env()
        .map_err(|e| anyhow::anyhow!("Secret encryption key error: {}", e))?;

    if seed_oidc_config_from_env(&db_pool, &secret_key).await? {
        info!("Seeded initial OIDC config from environment bootstrap values");
    }

    let public_base_url = std::env::var("RUSTSHARE_PUBLIC_URL")
        .unwrap_or_else(|_| "http://localhost:5173".to_string());

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
        notification_service: services.notification_service,
        user_share_service: services.user_share_service,
        ai_service: services.ai_service,
        upload_service: Some(services.upload_service),
        rate_limit_config,
        secret_key,
        oidc_runtime_cache: OidcRuntimeCache::new(),
        poll_rate_limiter: Arc::new(Mutex::new(HashMap::new())),
        default_tenant_id,
        note_service: services.note_service,
        decision_service: services.decision_service,
        meeting_service: services.meeting_service,
        standup_service: services.standup_service,
        module_service: services.module_service,
        template_service: services.template_service,
        kanban_service: services.kanban_service,
        brainstorming_service: services.brainstorming_service,
        user_repository: services.user_repository,
        public_base_url,
    };

    Ok(state)
}

pub fn default_storage_quota_bytes() -> i64 {
    std::env::var("RUSTSHARE_DEFAULT_STORAGE_QUOTA_BYTES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(10_737_418_240)
}
