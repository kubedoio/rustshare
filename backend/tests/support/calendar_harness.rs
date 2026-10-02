//! Shared DB/app harness for the Calendar Application integration suites.
//!
//! Extracted from the four calendar suites (api, import, google sync, outlook
//! sync) so a new DB-backed test costs ~10 lines instead of ~350.
//!
//! Safety guard: every entry point asserts that `DATABASE_URL`'s host is
//! `localhost`, `127.0.0.1`, `::1`, or the docker service name `postgres`,
//! refusing to run otherwise. This blocks accidental fixture writes into a
//! deployment database (leaked `@test.local` rows were previously found in the
//! dev deployment DB). Set `RUSTSHARE_TEST_ALLOW_REMOTE_DB=1` to override, and
//! only when you are certain the target is a throwaway database.
//!
//! DB-backed tests remain `#[ignore]`d and must run with `--test-threads=1`;
//! each binary's `SERIAL` guard serializes individual tests within the binary.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use sqlx::PgPool;
use tokio::sync::Mutex;
use tower::ServiceExt;
use uuid::Uuid;

use rustshare_server::state::AppState;

pub use super::mock_provider::{TEST_ACCESS_TOKEN, TEST_REFRESH_TOKEN};

/// Serializes the tests within one test binary (same convention as the
/// chat-bootstrap suite). Each calendar suite compiles its own instance.
pub static SERIAL: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// The database URL the tests will use, defaulting to a local dev database.
pub fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string())
}

/// The host of a Postgres URL, tolerating a missing userinfo section.
///
/// `postgres://deploy.example.com/db` must yield `deploy.example.com`, not
/// `postgres`: the scheme is stripped before the authority is split on the
/// last `@`, so a credential-less remote host cannot masquerade as the local
/// docker service.
fn database_host(url: &str) -> String {
    let after_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_scheme);
    // A password may contain '@', so the separator is the *last* one.
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    if let Some(rest) = authority.strip_prefix('[') {
        rest.split(']').next().unwrap_or(rest).to_string()
    } else {
        authority.split(':').next().unwrap_or(authority).to_string()
    }
}

fn is_local_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1" | "postgres")
}

/// Refuse to run DB-backed tests against a non-local database unless the
/// operator explicitly opts in with `RUSTSHARE_TEST_ALLOW_REMOTE_DB=1`.
pub fn assert_local_database() {
    let override_enabled = std::env::var("RUSTSHARE_TEST_ALLOW_REMOTE_DB").as_deref() == Ok("1");
    assert_local_database_with(&database_url(), override_enabled);
}

fn assert_local_database_with(url: &str, override_enabled: bool) {
    if override_enabled {
        return;
    }
    let host = database_host(url);
    if !is_local_host(&host) {
        panic!(
            "refusing to run calendar DB tests against DATABASE_URL host `{host}`: \
             these tests write fixture rows (tenants, users, calendar sources). \
             Point DATABASE_URL at a local/throwaway database, or set \
             RUSTSHARE_TEST_ALLOW_REMOTE_DB=1 to override."
        );
    }
}

/// Connect to the test database, enforcing the local-host guard.
pub async fn connect_pool() -> PgPool {
    assert_local_database();
    PgPool::connect(&database_url())
        .await
        .expect("Failed to connect to database")
}

/// Provider wiring for `setup_test_env_with_providers`.
///
/// Provider clients must be attached before the `CalendarService` is
/// `Arc`-wrapped, which is why this is a setup-time input rather than a
/// post-hoc mutation on `AppState`. `public_url` mirrors
/// `RUSTSHARE_PUBLIC_URL` and is the origin the redirect URIs derive from.
#[derive(Default)]
pub struct TestProviders {
    pub public_url: String,
    pub google: Option<rustshare_server::services::google_calendar::GoogleCalendarClient>,
    pub outlook: Option<rustshare_server::services::outlook_calendar::OutlookCalendarClient>,
}

/// Full `AppState` wired with the calendar service's outbox publishing enabled.
pub async fn setup_test_env() -> AppState {
    setup_test_env_inner(true, TestProviders::default()).await
}

/// Full `AppState` with the calendar service's outbox publishing left off
/// (the import suite's historical wiring).
pub async fn setup_test_env_without_calendar_outbox() -> AppState {
    setup_test_env_inner(false, TestProviders::default()).await
}

/// Full `AppState` with explicit calendar provider clients and public origin.
/// The Task 5 reviewer flagged the missing setup-time provider injection: the
/// connect suite needs clients configured before the service is wrapped in an
/// `Arc`, so no `Arc::get_mut` dance is possible afterwards.
pub async fn setup_test_env_with_providers(providers: TestProviders) -> AppState {
    setup_test_env_inner(true, providers).await
}

/// Persist a single-use OAuth state for the connect-flow callback tests.
pub async fn insert_oauth_state(
    state: &AppState,
    oauth_state: &str,
    tenant_id: Uuid,
    user_id: Uuid,
    kind: &str,
) {
    state
        .metadata_store
        .insert_calendar_oauth_state(
            oauth_state,
            tenant_id,
            user_id,
            kind,
            chrono::Utc::now() + chrono::Duration::minutes(10),
        )
        .await
        .expect("insert calendar oauth state");
}

async fn setup_test_env_inner(calendar_outbox: bool, providers: TestProviders) -> AppState {
    dotenvy::dotenv().ok();

    let pool = connect_pool().await;

    let metadata_store = Arc::new(rustshare_storage::MetadataStore::new(pool.clone()));
    let event_store = Arc::new(rustshare_storage::EventStore::new(pool.clone()));
    let broadcaster = Arc::new(rustshare_core::events::EventBroadcaster::new(100));

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
        rustshare_storage::ObjectStore::new_with_options(
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

    let permission_resolver =
        Arc::new(rustshare_core::services::PermissionResolver::new(Arc::new(
            rustshare_infrastructure::repositories::PermissionResolverRepository::new(pool.clone()),
        )));

    let file_service = Arc::new(rustshare_core::services::FileService::new(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        broadcaster.clone(),
        permission_resolver.clone(),
    ));

    let folder_service = Arc::new(rustshare_core::services::FolderService::new(
        event_store.clone(),
        metadata_store.clone(),
        broadcaster.clone(),
        permission_resolver.clone(),
    ));

    let share_notification_repo = Arc::new(
        rustshare_storage::repos::ShareNotificationRepoImpl::new(pool.clone()),
    );

    let share_service = Arc::new(rustshare_core::services::ShareService::new(
        event_store.clone(),
        metadata_store.clone(),
        broadcaster.clone(),
        jwt_manager.clone(),
        share_notification_repo.clone(),
    ));

    let thumbnail_service = Arc::new(rustshare_core::services::ThumbnailService::new(
        pool.clone(),
        object_store.clone(),
    ));
    let notification_service = Arc::new(rustshare_core::services::NotificationService::new(
        rustshare_infrastructure::repositories::NotificationRepository::new(pool.clone()),
    ));

    let user_repository = Arc::new(rustshare_infrastructure::repositories::UserRepository::new(
        pool.clone(),
    ));
    let file_repository = Arc::new(rustshare_infrastructure::repositories::FileRepository::new(
        pool.clone(),
    ));
    let folder_repository =
        Arc::new(rustshare_infrastructure::repositories::FolderRepository::new(pool.clone()));
    let share_repository =
        Arc::new(rustshare_infrastructure::repositories::ShareRepository::new(pool.clone()));

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

    let chat_integration_service = Arc::new(rustshare_core::services::ChatIntegrationService::new(
        metadata_store.clone(),
        event_store.clone(),
        broadcaster.clone(),
        "test-secret",
        Arc::new(rustshare_core::services::HttpWebhookDispatcher::new()),
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

    let outbox_store = Arc::new(rustshare_storage::OutboxStore::new(
        pool.clone(),
        Arc::new(rustshare_core::domain::ApplicationRegistry::first_party().unwrap()),
    ));
    let calendar_service = Arc::new({
        let mut service = rustshare_server::services::calendar_service::CalendarService::new(
            metadata_store.clone(),
            Arc::new(secret_key.clone()),
        );
        if calendar_outbox {
            service.configure_outbox(outbox_store.clone());
        }
        service.configure_public_url(providers.public_url.clone());
        service.configure_google(providers.google);
        service.configure_outlook(providers.outlook);
        service
    });
    let chat_observation_store =
        Arc::new(rustshare_storage::ChatObservationStore::new(pool.clone()));
    let memory_catalog_store = Arc::new(rustshare_storage::MemoryCatalogStore::new(pool.clone()));
    let buzz_observation_service = Arc::new(
        rustshare_server::buzz_observation::BuzzObservationService::new(
            pool.clone(),
            rustshare_storage::ChatIdentityStore::new(pool.clone()),
            (*chat_observation_store).clone(),
            outbox_store.clone(),
            rustshare_crypto::WebhookSigner::new("test-secret"),
            300,
            Arc::new(rustshare_core::events::EventBroadcaster::new(64)),
        ),
    );

    let unified_search_service = Arc::new(
        rustshare_server::services::unified_search::UnifiedSearchService::new(
            Arc::new(rustshare_resource_auth::SourceAuthorizer::empty()),
            metadata_store.clone(),
            None,
            memory_catalog_store.clone(),
        ),
    );

    let chat_owner = Arc::new(rustshare_server::authz::ChatResourceOwner::new(
        rustshare_storage::ChatIdentityStore::new(pool.clone()),
        (*chat_observation_store).clone(),
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
        source_authorizer: Arc::new(rustshare_resource_auth::SourceAuthorizer::empty()),
        notification_service,
        user_share_service,
        ai_service: None,
        upload_service: None,
        rate_limit_config: Arc::new(rustshare_server::middleware::RateLimitConfig::new()),
        secret_key,
        oidc_runtime_cache: rustshare_server::oidc_runtime::OidcRuntimeCache::new(),
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
        vault_sync_service,
        chat_integration_service,
        mail_service,
        calendar_service,
        outbox_store,
        chat_observation_store,
        memory_catalog_store,
        unified_search_service: unified_search_service.clone(),
        ask_workspace_service: Arc::new(
            rustshare_server::services::ask_workspace::AskWorkspaceService::new(
                unified_search_service.clone(),
                None,
            ),
        ),
        buzz_observation_service,
        chat_owner,
        buzz_gateway: None,
        chat_bootstrap: None,
        chat_provisioning: rustshare_server::config::ChatProvisioningMode::Manual,
        user_repository,
        public_base_url: "http://localhost:8080".to_string(),
        collab_rooms: Arc::new(rustshare_server::handlers::collab::CollabRooms::new()),
        outbox_status: Arc::new(rustshare_server::outbox_dispatcher::OutboxStatus::default()),
        outbox_worker_enabled: false,
        outbox_readiness_staleness_secs: 60,
        shutdown_tx: tokio::sync::broadcast::channel(1).0,
        prometheus_handle: rustshare_server::metrics::init_metrics(),
    }
}

pub async fn create_test_tenant(pool: &PgPool) -> Uuid {
    let tenant_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tenants (id, name, created_at, updated_at) VALUES ($1, $2, NOW(), NOW()) ON CONFLICT (id) DO NOTHING",
    )
    .bind(tenant_id)
    .bind(format!("Calendar Test Tenant {tenant_id}"))
    .execute(pool)
    .await
    .expect("Failed to create test tenant");
    tenant_id
}

pub async fn create_test_user(
    state: &AppState,
    username: &str,
    tenant_id: Uuid,
) -> rustshare_core::domain::User {
    let unique_username = format!("{}-{}", username, Uuid::new_v4());
    let user = rustshare_core::domain::User::new(
        unique_username.clone(),
        format!("{} Display", unique_username),
        "test_password_hash".to_string(),
        format!("{}@test.local", unique_username),
        false,
        10_737_418_240,
        tenant_id,
    );
    state
        .metadata_store
        .create_user(&user)
        .await
        .expect("Failed to create test user");
    user
}

pub fn create_auth_token(state: &AppState, user_id: Uuid, tenant_id: Uuid) -> String {
    state
        .jwt_manager
        .generate(user_id, "test@example.com", tenant_id)
        .unwrap()
}

/// Seed the first-party applications for the tenant without enabling
/// Calendar; `enable` toggles it on.
pub async fn configure_calendar(state: &AppState, tenant_id: Uuid, user_id: Uuid, enable: bool) {
    state
        .application_service
        .ensure_default_applications(tenant_id)
        .await
        .expect("ensure_default_applications should succeed");
    if enable {
        state
            .application_service
            .enable_application("io.elembra.calendar", user_id, tenant_id)
            .await
            .expect("enable calendar module should succeed");
    }
}

pub fn build_app(state: AppState) -> axum::Router<()> {
    rustshare_server::routes::calendar_routes()
        .with_state(state)
        .layer(axum::middleware::from_fn(
            rustshare_server::middleware::security_headers_middleware,
        ))
}

pub async fn cleanup_tenant(pool: &PgPool, tenant_id: Uuid) {
    // Calendar rows cascade from users; delete explicit tables first so a
    // broken cascade cannot mask other errors.
    sqlx::query(
        "DELETE FROM calendar_sync_states WHERE source_id IN
            (SELECT id FROM calendar_sources WHERE tenant_id = $1)",
    )
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("failed to clean up calendar_sync_states");
    sqlx::query("DELETE FROM calendar_oauth_states WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(pool)
        .await
        .expect("failed to clean up calendar_oauth_states");
    for table in [
        "calendar_import_jobs",
        "calendar_events",
        "calendar_sources",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE tenant_id = $1"))
            .bind(tenant_id)
            .execute(pool)
            .await
            .unwrap_or_else(|e| panic!("failed to clean up {table}: {e}"));
    }
    sqlx::query("DELETE FROM application_enablements WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(pool)
        .await
        .expect("failed to clean up application enablements");
    sqlx::query("DELETE FROM users WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(pool)
        .await
        .expect("failed to clean up users");
    sqlx::query("DELETE FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .execute(pool)
        .await
        .expect("failed to clean up tenant");
}

/// Remove the tenant's integration outbox/delivery rows.
pub async fn cleanup_outbox(pool: &PgPool, tenant_id: Uuid) {
    sqlx::query("DELETE FROM integration_deliveries WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(pool)
        .await
        .expect("clean up integration_deliveries");
    sqlx::query("DELETE FROM integration_outbox WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(pool)
        .await
        .expect("clean up integration_outbox");
}

pub async fn response_json(response: axum::response::Response) -> (StatusCode, Value) {
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("failed to read response body");
    let value = serde_json::from_slice(&body).expect("response body should be JSON");
    (status, value)
}

pub const IMPORT_BOUNDARY: &str = "calendar-import-boundary";

pub fn multipart_file_body(boundary: &str, filename: &str, content: &str) -> Vec<u8> {
    multipart_file_body_with_type(boundary, filename, "text/calendar", content)
}

pub fn multipart_file_body_with_type(
    boundary: &str,
    filename: &str,
    content_type: &str,
    content: &str,
) -> Vec<u8> {
    format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n\
         Content-Type: {content_type}\r\n\
         \r\n\
         {content}\r\n\
         --{boundary}--\r\n"
    )
    .into_bytes()
}

/// POST an arbitrary multipart body to an endpoint.
pub async fn upload_multipart(
    app: &axum::Router<()>,
    uri: &str,
    token: &str,
    boundary: &str,
    body: Vec<u8>,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .method("POST")
                .header("Authorization", format!("Bearer {token}"))
                .header(
                    "Content-Type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    response_json(response).await
}

/// The lightweight harness used by the sync suites: a pool, metadata store,
/// outbox, and one tenant with the Calendar application enabled.
pub struct SyncHarness {
    pub pool: PgPool,
    pub store: Arc<rustshare_storage::MetadataStore>,
    pub secret_key: Arc<rustshare_crypto::SecretEncryptionKey>,
    pub outbox: Arc<rustshare_storage::OutboxStore>,
    pub tenant_id: Uuid,
}

impl SyncHarness {
    pub async fn new() -> Self {
        dotenvy::dotenv().ok();
        let pool = connect_pool().await;
        let store = Arc::new(rustshare_storage::MetadataStore::new(pool.clone()));
        let outbox = Arc::new(rustshare_storage::OutboxStore::new(
            pool.clone(),
            Arc::new(rustshare_core::domain::ApplicationRegistry::first_party().unwrap()),
        ));
        let tenant_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO tenants (id, name, created_at, updated_at) VALUES ($1, $2, NOW(), NOW()) ON CONFLICT (id) DO NOTHING",
        )
        .bind(tenant_id)
        .bind(format!("Calendar Sync Test Tenant {tenant_id}"))
        .execute(&pool)
        .await
        .expect("Failed to create test tenant");
        sqlx::query(
            "INSERT INTO application_enablements (tenant_id, workspace_id, application_id, enabled)
             VALUES ($1, $1, 'io.elembra.calendar', true)",
        )
        .bind(tenant_id)
        .execute(&pool)
        .await
        .expect("Failed to enable calendar application");
        Self {
            pool,
            store,
            secret_key: Arc::new(rustshare_crypto::SecretEncryptionKey::from_bytes([7u8; 32])),
            outbox,
            tenant_id,
        }
    }

    pub async fn create_user(&self, label: &str) -> rustshare_core::domain::User {
        let username = format!("{}-{}", label, Uuid::new_v4());
        let user = rustshare_core::domain::User::new(
            username.clone(),
            format!("{username} Display"),
            "test_password_hash".to_string(),
            format!("{username}@test.local"),
            false,
            10_737_418_240,
            self.tenant_id,
        );
        self.store
            .create_user(&user)
            .await
            .expect("Failed to create test user");
        user
    }

    /// Create an OAuth source with valid unexpired tokens against a mock.
    pub async fn create_oauth_source(
        &self,
        user_id: Uuid,
        kind: &str,
        display_name: &str,
        external_account: &str,
        scope: &str,
    ) -> rustshare_core::domain::CalendarSource {
        self.create_oauth_source_with_access_expiry(
            user_id,
            kind,
            display_name,
            external_account,
            scope,
            3600,
        )
        .await
    }

    pub async fn create_oauth_source_with_access_expiry(
        &self,
        user_id: Uuid,
        kind: &str,
        display_name: &str,
        external_account: &str,
        scope: &str,
        access_expires_in_secs: i64,
    ) -> rustshare_core::domain::CalendarSource {
        let refresh_enc = rustshare_crypto::encrypt_secret(TEST_REFRESH_TOKEN, &self.secret_key)
            .expect("encrypt refresh token");
        let access_enc = rustshare_crypto::encrypt_secret(TEST_ACCESS_TOKEN, &self.secret_key)
            .expect("encrypt access token");
        self.store
            .create_oauth_calendar_source(
                self.tenant_id,
                user_id,
                kind,
                display_name,
                external_account,
                "primary",
                &refresh_enc,
                &access_enc,
                chrono::Utc::now() + chrono::Duration::seconds(access_expires_in_secs),
                scope,
            )
            .await
            .expect("create oauth calendar source")
    }

    pub async fn reload_source(&self, source_id: Uuid) -> rustshare_core::domain::CalendarSource {
        let row = sqlx::query_as!(
            rustshare_core::domain::CalendarSource,
            r#"SELECT id, tenant_id, owner_id, kind, display_name, external_account,
                external_calendar_id, refresh_token_enc, access_token_enc,
                access_token_expires_at, scopes, is_enabled, last_synced_at,
                last_error, status, deleted_at, created_at, updated_at
            FROM calendar_sources WHERE id = $1"#,
            source_id,
        )
        .fetch_one(&self.pool)
        .await
        .expect("reload source");
        row
    }

    pub async fn list_source_events(
        &self,
        source_id: Uuid,
    ) -> Vec<rustshare_core::domain::CalendarEvent> {
        sqlx::query_as!(
            rustshare_core::domain::CalendarEvent,
            r#"SELECT id, tenant_id, owner_id, source_id, external_uid, external_etag,
                recurrence_id, title, description, location, starts_at, ends_at,
                all_day, original_date, timezone, rrule, status, read_only, raw,
                deleted_at, created_at, updated_at
            FROM calendar_events WHERE source_id = $1 AND deleted_at IS NULL
            ORDER BY external_uid, COALESCE(recurrence_id, '')"#,
            source_id,
        )
        .fetch_all(&self.pool)
        .await
        .expect("list source events")
    }

    pub async fn cleanup(&self) {
        let tenant_id = self.tenant_id;
        sqlx::query(
            "DELETE FROM calendar_sync_states WHERE source_id IN
                (SELECT id FROM calendar_sources WHERE tenant_id = $1)",
        )
        .bind(tenant_id)
        .execute(&self.pool)
        .await
        .expect("cleanup calendar_sync_states");
        sqlx::query("DELETE FROM calendar_oauth_states WHERE tenant_id = $1")
            .bind(tenant_id)
            .execute(&self.pool)
            .await
            .expect("cleanup calendar_oauth_states");
        sqlx::query("DELETE FROM integration_deliveries WHERE tenant_id = $1")
            .bind(tenant_id)
            .execute(&self.pool)
            .await
            .expect("cleanup integration_deliveries");
        sqlx::query("DELETE FROM integration_outbox WHERE tenant_id = $1")
            .bind(tenant_id)
            .execute(&self.pool)
            .await
            .expect("cleanup integration_outbox");
        for table in [
            "calendar_import_jobs",
            "calendar_events",
            "calendar_sources",
        ] {
            sqlx::query(&format!("DELETE FROM {table} WHERE tenant_id = $1"))
                .bind(tenant_id)
                .execute(&self.pool)
                .await
                .unwrap_or_else(|e| panic!("cleanup {table}: {e}"));
        }
        sqlx::query("DELETE FROM users WHERE tenant_id = $1")
            .bind(tenant_id)
            .execute(&self.pool)
            .await
            .expect("cleanup users");
        sqlx::query("DELETE FROM application_enablements WHERE tenant_id = $1")
            .bind(tenant_id)
            .execute(&self.pool)
            .await
            .expect("cleanup enablements");
        sqlx::query("DELETE FROM tenants WHERE id = $1")
            .bind(tenant_id)
            .execute(&self.pool)
            .await
            .expect("cleanup tenant");
    }
}

#[cfg(test)]
mod guard_tests {
    use super::{assert_local_database, assert_local_database_with, database_host, is_local_host};

    #[test]
    fn host_parses_local_urls_with_and_without_credentials_or_port() {
        for url in [
            "postgres://localhost/db",
            "postgres://localhost:5432/db",
            "postgres://u:p@localhost:5432/db",
            "postgres://127.0.0.1/db",
            "postgres://u:p@127.0.0.1:5432/db",
            "postgres://[::1]/db",
            "postgres://[::1]:5432/db",
            "postgres://postgres/db",
            "postgres://u:p@postgres:5432/db",
        ] {
            let host = database_host(url);
            assert!(is_local_host(&host), "{url} parsed to host `{host}`");
        }
    }

    #[test]
    fn host_parses_remote_urls_with_and_without_credentials() {
        assert_eq!(
            database_host("postgres://deploy.example.com/db"),
            "deploy.example.com"
        );
        assert_eq!(
            database_host("postgres://deploy.example.com:5432/db"),
            "deploy.example.com"
        );
        assert_eq!(
            database_host("postgres://u:p@deploy.example.com/db"),
            "deploy.example.com"
        );
    }

    #[test]
    fn guard_refuses_remote_hosts_with_and_without_credentials() {
        for url in [
            "postgres://deploy.example.com/db",
            "postgres://deploy.example.com:5432/db",
            "postgres://u:p@deploy.example.com/db",
            "postgres://u:p@deploy.example.com:5432/db",
        ] {
            let result = std::panic::catch_unwind(|| assert_local_database_with(url, false));
            assert!(result.is_err(), "{url} must be refused");
        }
    }

    #[test]
    fn guard_allows_local_hosts_and_remote_override() {
        for url in [
            "postgres://localhost/db",
            "postgres://127.0.0.1:5432/db",
            "postgres://[::1]/db",
            "postgres://postgres:5432/db",
        ] {
            assert_local_database_with(url, false);
        }
        // The documented opt-out lets a remote URL through.
        assert_local_database_with("postgres://deploy.example.com/db", true);
    }

    #[test]
    fn guard_env_override_is_honoured() {
        std::env::set_var("DATABASE_URL", "postgres://deploy.example.com/db");
        std::env::set_var("RUSTSHARE_TEST_ALLOW_REMOTE_DB", "1");
        assert_local_database();
        std::env::remove_var("RUSTSHARE_TEST_ALLOW_REMOTE_DB");
        std::env::remove_var("DATABASE_URL");
    }
}
