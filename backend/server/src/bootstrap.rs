use crate::config::AppConfig;
use crate::handlers::collab::CollabRooms;
use crate::handlers::ensure_optional_seed_user;
use crate::oidc_runtime::{seed_oidc_config_from_env, OidcRuntimeCache};
use crate::replication::{spawn_replication_worker, ReplicationWorkerConfig};
use crate::retention::{spawn_retention_cleanup_worker, RetentionConfig};
use crate::state::{AppAiService, AppState, AppUploadService, AppUserShareService};
use crate::trash_cleanup::{spawn_trash_cleanup_worker, TrashCleanupConfig};
use anyhow::Result;
use rand::RngExt;
use rustshare_auth::{JwtManager, PasswordHasher};
#[allow(deprecated)]
use rustshare_core::{
    domain::User,
    events::EventBroadcaster,
    services::{
        AiService, ChatIntegrationService, ContentIndexer, FileService, FolderService,
        HttpWebhookDispatcher, NotificationService, PermissionResolver, ShareService,
        SimpleEmbeddingGenerator, ThumbnailService, UserShareService, UserShareServiceDeps,
        VaultSyncService,
    },
};
use rustshare_crypto::SecretEncryptionKey;
use rustshare_infrastructure::{
    repositories::{
        FileRepository, FolderRepository, NotificationRepository, PermissionResolverRepository,
        ShareRepository, UserRepository,
    },
    PgVectorStore,
};
use rustshare_storage::{repos::ShareNotificationRepoImpl, EventStore, MetadataStore, ObjectStore};
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
    vault_sync_service: Arc<VaultSyncService<MetadataStore, ObjectStore>>,
    chat_integration_service: Arc<crate::state::AppChatIntegrationService>,
    mail_service: Arc<crate::services::mail_service::MailService>,
    secret_key: Arc<SecretEncryptionKey>,
}

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

async fn init_stores(
    config: &AppConfig,
    db_pool: PgPool,
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
    secret_key: Arc<SecretEncryptionKey>,
    config: &AppConfig,
) -> Result<Services> {
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
        enable_optimistic_concurrency: true,
        fallback_to_leases: true,
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
        vault_sync_service,
        chat_integration_service,
        mail_service,
        secret_key,
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

    let (metadata_store, event_store, object_store) = init_stores(&config, db_pool.clone()).await?;

    let jwt_manager = init_jwt_manager(&config).await?;

    let broadcaster = init_broadcaster(&config).await?;

    let (
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

    let rate_limit_config = Arc::new(crate::middleware::RateLimitConfig::new());
    info!("Rate limiting initialized");

    let default_tenant_id = std::env::var("RUSTSHARE_DEFAULT_TENANT_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(uuid::Uuid::nil);

    let prometheus_handle = crate::metrics::init_metrics();
    info!("Prometheus metrics recorder installed");

    let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);

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
        trash_cleanup_config,
        shutdown_tx.subscribe(),
    );

    let retention_config = RetentionConfig::from_env();
    spawn_retention_cleanup_worker(
        Arc::clone(&metadata_store),
        retention_config,
        shutdown_tx.subscribe(),
    );

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
            match std::env::var("RUSTSHARE_ADMIN_PASSWORD") {
                Ok(pwd) => (pwd, true),
                Err(_) => {
                    const CHARSET: &[u8] =
                        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
                    let mut rng = rand::rng();
                    let password: String = (0..32)
                        .map(|_| CHARSET[rng.random_range(0..CHARSET.len())] as char)
                        .collect();
                    (password, false)
                }
            };

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

        let pref_repo = rustshare_infrastructure::repositories::UserModulePreferenceRepository::new(
            db_pool.clone(),
        );
        if let Err(e) = pref_repo.seed_defaults(admin_user.id).await {
            tracing::warn!(
                "Failed to seed default module preferences for admin: {:?}",
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
        secret_key: (*services.secret_key).clone(),
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
        collab_rooms: Arc::new(CollabRooms::new()),
        vault_sync_service: services.vault_sync_service,
        chat_integration_service: services.chat_integration_service,
        mail_service: services.mail_service,
        shutdown_tx,
        prometheus_handle,
    };

    Ok(state)
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
