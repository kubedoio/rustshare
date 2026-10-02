//! DB-backed integration suite for the Calendar Application .ics import
//! (issue #315, Task 2 of the calendar implementation plan).
//!
//! Covers: multipart upload → `202` with a pending job, background worker
//! processing to `completed`, range queries returning the imported events
//! (TZID converted to UTC, all-day spans), and idempotent re-import: a
//! second upload of the identical file creates no duplicate rows.
//!
//! DB-backed and `#[ignore]`d; run against the dev database (migrations
//! applied) with `--test-threads=1`:
//!
//!   set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true \
//!     cargo test -p rustshare-server --test calendar_import_test -- \
//!       --ignored --test-threads=1
//!
//! Every test takes the shared `SERIAL` guard and cleans up exactly the rows
//! it created under fresh tenants.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rustshare_server::state::AppState;
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

/// Serializes the tests within this binary (same convention as the
/// chat-bootstrap suite).
static SERIAL: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

async fn setup_test_env() -> AppState {
    dotenvy::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());

    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database");

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

    let calendar_service = Arc::new(
        rustshare_server::services::calendar_service::CalendarService::new(
            metadata_store.clone(),
            Arc::new(secret_key.clone()),
        ),
    );

    let outbox_store = Arc::new(rustshare_storage::OutboxStore::new(
        pool.clone(),
        Arc::new(rustshare_core::domain::ApplicationRegistry::first_party().unwrap()),
    ));
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

async fn create_test_tenant(pool: &PgPool) -> Uuid {
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

async fn create_test_user(
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

fn create_auth_token(state: &AppState, user_id: Uuid, tenant_id: Uuid) -> String {
    state
        .jwt_manager
        .generate(user_id, "test@example.com", tenant_id)
        .unwrap()
}

/// Seed the first-party applications for the tenant without enabling
/// Calendar; `enable_calendar` toggles it on.
async fn configure_calendar(state: &AppState, tenant_id: Uuid, user_id: Uuid, enable: bool) {
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

fn build_app(state: AppState) -> axum::Router<()> {
    rustshare_server::routes::calendar_routes()
        .with_state(state)
        .layer(axum::middleware::from_fn(
            rustshare_server::middleware::security_headers_middleware,
        ))
}

async fn cleanup_tenant(pool: &PgPool, tenant_id: Uuid) {
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

async fn response_json(response: axum::response::Response) -> (StatusCode, Value) {
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("failed to read response body");
    let value = serde_json::from_slice(&body).expect("response body should be JSON");
    (status, value)
}

const ICS_FIXTURE: &str = "\
BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//RustShare//Test//EN
BEGIN:VEVENT
UID:import-minimal-1
DTSTART:20261005T140000Z
DTEND:20261005T150000Z
SUMMARY:Minimal meeting
END:VEVENT
BEGIN:VEVENT
UID:import-tzid-1
DTSTART;TZID=Europe/Berlin:20261006T140000
DTEND;TZID=Europe/Berlin:20261006T150000
SUMMARY:Berlin meeting
END:VEVENT
BEGIN:VEVENT
UID:import-allday-1
DTSTART;VALUE=DATE:20261007
DTEND;VALUE=DATE:20261008
SUMMARY:All day off
END:VEVENT
BEGIN:VEVENT
UID:import-recur-1
DTSTART:20261008T140000Z
DTEND:20261008T150000Z
RRULE:FREQ=WEEKLY;COUNT=3
SUMMARY:Weekly sync
END:VEVENT
BEGIN:VEVENT
UID:import-recur-1
RECURRENCE-ID:20261015T140000Z
DTSTART:20261015T160000Z
DTEND:20261015T170000Z
SUMMARY:Weekly sync moved
END:VEVENT
BEGIN:VTODO
UID:import-todo-1
SUMMARY:A task that must be skipped
END:VTODO
END:VCALENDAR
";

fn multipart_file_body(boundary: &str, filename: &str, content: &str) -> Vec<u8> {
    format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n\
         Content-Type: text/calendar\r\n\
         \r\n\
         {content}\r\n\
         --{boundary}--\r\n"
    )
    .into_bytes()
}

async fn upload_ics(
    app: &axum::Router<()>,
    token: &str,
    filename: &str,
    content: &str,
) -> (StatusCode, Value) {
    let boundary = "calendar-import-boundary";
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/calendar/import")
                .method("POST")
                .header("Authorization", format!("Bearer {token}"))
                .header(
                    "Content-Type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(multipart_file_body(boundary, filename, content)))
                .unwrap(),
        )
        .await
        .unwrap();
    response_json(response).await
}

async fn spawn_import_worker(state: &AppState) {
    rustshare_server::calendar_import_worker::spawn_calendar_import_worker(
        Arc::clone(&state.metadata_store),
        Arc::clone(&state.outbox_store),
        state.shutdown_tx.subscribe(),
        rustshare_server::calendar_import_worker::CalendarImportWorkerConfig {
            poll_interval: std::time::Duration::from_millis(250),
            max_concurrent_jobs: 2,
            stale_threshold: std::time::Duration::from_secs(300),
        },
    );
}

/// Poll `GET /api/v1/calendar/import-jobs/{id}` until the job reaches a
/// terminal state, asserting it completes successfully.
async fn wait_for_job(app: &axum::Router<()>, token: &str, job_id: Uuid) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/calendar/import-jobs/{job_id}"))
                    .header("Authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let (status, body) = response_json(response).await;
        assert_eq!(status, StatusCode::OK);
        let status = body["status"].as_str().unwrap();
        if matches!(status, "completed" | "failed" | "cancelled") {
            return body;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "import job {job_id} did not finish in time"
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

async fn count_imported_events(state: &AppState, tenant_id: Uuid) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM calendar_events WHERE tenant_id = $1 AND external_uid IS NOT NULL",
    )
    .bind(tenant_id)
    .fetch_one(&state.db_pool)
    .await
    .expect("count imported events")
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn ics_upload_import_and_reimport_is_idempotent() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_import", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());
    spawn_import_worker(&state).await;

    // First upload: accepted with a pending job.
    let (status, body) = upload_ics(&app, &token, "export.ics", ICS_FIXTURE).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let job_id = Uuid::parse_str(body["job_id"].as_str().unwrap()).unwrap();
    let source_id = Uuid::parse_str(body["source_id"].as_str().unwrap()).unwrap();
    assert_eq!(body["status"], "pending");

    // The worker picks it up and completes it.
    let job = wait_for_job(&app, &token, job_id).await;
    assert_eq!(job["status"], "completed");
    assert_eq!(job["failed_events"], 0);
    assert_eq!(job["total_events"], 5);
    assert_eq!(job["processed_events"], 5);

    // The completed run published one imported.v1 envelope with counts and
    // the source ResourceRef — identifiers/counts only, never titles.
    let envelope = sqlx::query_scalar::<_, Value>(
        "SELECT event_json FROM integration_outbox \
         WHERE tenant_id = $1 AND event_type = 'io.elembra.calendar.event.imported.v1' \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(&state.db_pool)
    .await
    .expect("query integration_outbox")
    .expect("imported envelope published");
    assert_eq!(envelope["data"]["processed_events"], 5);
    assert_eq!(envelope["data"]["total_events"], 5);
    assert_eq!(envelope["elembraResource"]["resourceType"], "source");
    assert_eq!(
        envelope["elembraResource"]["resourceId"],
        source_id.to_string()
    );
    assert!(
        envelope["data"].get("title").is_none(),
        "import events must not carry titles"
    );

    // Range query returns the imported events with converted times.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/calendar/events?from=2026-10-01T00:00:00Z&to=2026-10-31T00:00:00Z")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = response_json(response).await;
    assert_eq!(status, StatusCode::OK);
    let events = body["events"].as_array().unwrap();
    let titles: Vec<&str> = events
        .iter()
        .map(|event| event["title"].as_str().unwrap())
        .collect();
    assert!(titles.contains(&"Minimal meeting"));
    assert!(titles.contains(&"Berlin meeting"));
    assert!(titles.contains(&"All day off"));
    // The weekly master expands (2 in-window instances; the third is
    // overridden) and the override row is returned as stored.
    assert!(titles.contains(&"Weekly sync"));
    assert!(titles.contains(&"Weekly sync moved"));

    // TZID event is stored as the correct UTC instant (14:00 CEST = 12:00Z).
    let berlin = events
        .iter()
        .find(|event| event["title"] == "Berlin meeting")
        .unwrap();
    assert_eq!(berlin["starts_at"], "2026-10-06T12:00:00Z");
    assert_eq!(berlin["timezone"], "Europe/Berlin");
    // All-day event is a UTC-midnight span.
    let all_day = events
        .iter()
        .find(|event| event["title"] == "All day off")
        .unwrap();
    assert_eq!(all_day["starts_at"], "2026-10-07T00:00:00Z");
    assert_eq!(all_day["ends_at"], "2026-10-08T00:00:00Z");
    assert_eq!(all_day["all_day"], true);
    // Imported events are read-only mirrors of the file.
    assert!(events.iter().all(|event| event["read_only"] == true));

    // Row count after the first import.
    let first_count = count_imported_events(&state, tenant_id).await;
    assert_eq!(first_count, 5);

    // Re-upload the identical file (same filename reuses the source) and let
    // the worker process it: no duplicate rows appear.
    let (status, body) = upload_ics(&app, &token, "export.ics", ICS_FIXTURE).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(body["source_id"], source_id.to_string());
    let second_job = wait_for_job(
        &app,
        &token,
        Uuid::parse_str(body["job_id"].as_str().unwrap()).unwrap(),
    )
    .await;
    assert_eq!(second_job["status"], "completed");
    assert_eq!(second_job["total_events"], 5);
    assert_eq!(second_job["processed_events"], 5);

    let second_count = count_imported_events(&state, tenant_id).await;
    assert_eq!(
        first_count, second_count,
        "re-import must not duplicate rows"
    );

    // The job list shows both runs.
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/calendar/import-jobs")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = response_json(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["jobs"].as_array().unwrap().len(), 2);

    cleanup_outbox(&state.db_pool, tenant_id).await;
    cleanup_tenant(&state.db_pool, tenant_id).await;
}

async fn cleanup_outbox(pool: &PgPool, tenant_id: Uuid) {
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

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn structurally_invalid_ics_fails_the_job() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_import_bad", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());
    spawn_import_worker(&state).await;

    let (status, body) = upload_ics(&app, &token, "broken.ics", "not a calendar").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let job = wait_for_job(
        &app,
        &token,
        Uuid::parse_str(body["job_id"].as_str().unwrap()).unwrap(),
    )
    .await;
    assert_eq!(job["status"], "failed");
    assert!(job["last_error"].is_string());

    assert_eq!(count_imported_events(&state, tenant_id).await, 0);

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn single_event_upsert_failure_does_not_fail_the_job() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_import_onebad", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());
    spawn_import_worker(&state).await;

    // The first event violates the ends_at > starts_at table CHECK; the
    // second is well-formed. Per-component semantics: the job still
    // completes and the good event is imported.
    let ics = "\
BEGIN:VCALENDAR
BEGIN:VEVENT
UID:bad-window-1
DTSTART:20261005T150000Z
DTEND:20261005T140000Z
SUMMARY:Backwards window
END:VEVENT
BEGIN:VEVENT
UID:good-after-bad-1
DTSTART:20261006T140000Z
DTEND:20261006T150000Z
SUMMARY:Good event
END:VEVENT
END:VCALENDAR
";

    let (status, body) = upload_ics(&app, &token, "onebad.ics", ics).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let job = wait_for_job(
        &app,
        &token,
        Uuid::parse_str(body["job_id"].as_str().unwrap()).unwrap(),
    )
    .await;

    // The bad row counts as its own failure; the job itself completes.
    assert_eq!(job["status"], "completed");
    assert_eq!(job["total_events"], 2);
    assert_eq!(job["failed_events"], 1);
    assert_eq!(job["processed_events"], 1);
    assert!(job["last_error"].is_string());

    // Only the good event was persisted.
    assert_eq!(count_imported_events(&state, tenant_id).await, 1);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/calendar/events?from=2026-10-01T00:00:00Z&to=2026-10-31T00:00:00Z")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = response_json(response).await;
    assert_eq!(status, StatusCode::OK);
    let events = body["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["title"], "Good event");

    cleanup_outbox(&state.db_pool, tenant_id).await;
    cleanup_tenant(&state.db_pool, tenant_id).await;
}
