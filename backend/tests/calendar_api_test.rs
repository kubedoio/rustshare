//! DB-backed integration suite for the Calendar Application internal-events
//! API (issue #315, Task 1 of the calendar implementation plan).
//!
//! Covers: 403 when the Application is disabled for the tenant, the
//! create/list/update/delete round-trip (including lazy internal-source
//! creation and idempotent delete), 404 for foreign-owned event IDs, the
//! 400-day range-window rejection, and the 409 read-only-mirror rejection
//! for events of non-internal sources.
//!
//! DB-backed and `#[ignore]`d; run against the dev database (migrations
//! applied) with `--test-threads=1`:
//!
//!   set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true \
//!     cargo test -p rustshare-server --test calendar_api_test -- \
//!       --ignored --test-threads=1
//!
//! Every test takes the shared `SERIAL` guard and cleans up exactly the rows
//! it created under fresh tenants.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rustshare_server::state::AppState;
use serde_json::{json, Value};
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

    let outbox_store = Arc::new(rustshare_storage::OutboxStore::new(
        pool.clone(),
        Arc::new(rustshare_core::domain::ApplicationRegistry::first_party().unwrap()),
    ));
    let calendar_service = Arc::new({
        let mut service = rustshare_server::services::calendar_service::CalendarService::new(
            metadata_store.clone(),
            Arc::new(secret_key.clone()),
        );
        service.configure_outbox(outbox_store.clone());
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

fn create_event_body() -> Value {
    json!({
        "title": "Sprint review",
        "description": "Weekly sync",
        "location": "Room 1",
        "starts_at": "2026-10-05T14:00:00Z",
        "ends_at": "2026-10-05T15:00:00Z",
        "all_day": false,
        "timezone": "Europe/Berlin",
        "rrule": null
    })
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn calendar_disabled_returns_403() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_disabled", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, false).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/calendar/sources")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = response_json(response).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "Calendar module is disabled");

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn event_crud_round_trip() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_crud", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());

    // Create.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/calendar/events")
                .method("POST")
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .body(Body::from(create_event_body().to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = response_json(response).await;
    assert_eq!(status, StatusCode::CREATED);
    let event_id = Uuid::parse_str(body["id"].as_str().unwrap()).unwrap();
    assert_eq!(body["title"], "Sprint review");
    assert_eq!(body["source_kind"], "internal");
    assert_eq!(body["read_only"], false);

    // The internal source was created lazily.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/calendar/sources")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = response_json(response).await;
    assert_eq!(status, StatusCode::OK);
    let sources = body["sources"].as_array().unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0]["kind"], "internal");

    // Range list contains the event.
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
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["id"], event_id.to_string());

    // Single get.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/calendar/events/{event_id}"))
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = response_json(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["description"], "Weekly sync");

    // Partial update.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/calendar/events/{event_id}"))
                .method("PATCH")
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .body(Body::from(json!({"title": "Renamed review"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = response_json(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["title"], "Renamed review");

    // Delete (idempotent).
    for _ in 0..2 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/calendar/events/{event_id}"))
                    .method("DELETE")
                    .header("Authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let (status, body) = response_json(response).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["ok"], true);
    }

    // The event is gone from range lists.
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
    assert_eq!(body["events"].as_array().unwrap().len(), 0);

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn foreign_event_id_returns_404() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user_a = create_test_user(&state, "calendar_owner_a", tenant_id).await;
    let user_b = create_test_user(&state, "calendar_owner_b", tenant_id).await;
    configure_calendar(&state, tenant_id, user_a.id, true).await;
    configure_calendar(&state, tenant_id, user_b.id, true).await;

    let event = state
        .calendar_service
        .create_event(
            tenant_id,
            user_a.id,
            rustshare_server::services::calendar_service::NewCalendarEvent {
                title: "Private".to_string(),
                description: None,
                location: None,
                starts_at: "2026-10-05T14:00:00Z"
                    .parse::<chrono::DateTime<chrono::Utc>>()
                    .unwrap(),
                ends_at: "2026-10-05T15:00:00Z"
                    .parse::<chrono::DateTime<chrono::Utc>>()
                    .unwrap(),
                all_day: false,
                timezone: "UTC".to_string(),
                rrule: None,
            },
        )
        .await
        .expect("create event");

    let token_b = create_auth_token(&state, user_b.id, tenant_id);
    let app = build_app(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/calendar/events/{}", event.id))
                .header("Authorization", format!("Bearer {token_b}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, _) = response_json(response).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn range_window_over_366_days_returns_400() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_window", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/calendar/events?from=2026-01-01T00:00:00Z&to=2027-02-05T00:00:00Z")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, _) = response_json(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn invalid_rrule_returns_400() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_rrule", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());

    let mut body = create_event_body();
    body["rrule"] = json!("FREQ=NOT_A_FREQUENCY");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/calendar/events")
                .method("POST")
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, _) = response_json(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // A valid RRULE is accepted.
    let mut body = create_event_body();
    body["rrule"] = json!("FREQ=WEEKLY;BYDAY=MO");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/calendar/events")
                .method("POST")
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = response_json(response).await;
    assert_eq!(status, StatusCode::CREATED);

    // And it expands within a range window.
    let event_id = body["id"].as_str().unwrap();
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
    assert!(events.iter().all(|event| event["id"] == event_id));
    assert!(events
        .iter()
        .any(|event| event["instance_start"].is_string()));
    assert!(events.len() > 1, "weekly master should expand in-window");

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn repeated_source_id_query_params_parse() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_src_filter", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;

    let internal = state
        .calendar_service
        .ensure_internal_source(tenant_id, user.id)
        .await
        .expect("internal source");
    let imported = state
        .calendar_service
        .create_source(
            tenant_id,
            user.id,
            rustshare_core::domain::CalendarSourceKind::IcalImport,
            "Imported".to_string(),
        )
        .await
        .expect("ical source");

    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/calendar/events?from=2026-10-01T00:00:00Z&to=2026-10-31T00:00:00Z&source_id={}&source_id={}",
                    internal.id, imported.id
                ))
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, _) = response_json(response).await;
    assert_eq!(status, StatusCode::OK);

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn patch_on_read_only_mirror_event_returns_409() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_mirror", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;

    // Mirror fixtures: a Google source with one read-only mirrored event,
    // inserted the way the sync worker (Tasks 4/5) will.
    let source_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO calendar_sources
            (id, tenant_id, owner_id, kind, display_name, external_account, external_calendar_id, status)
         VALUES ($1, $2, $3, 'google', 'Work Google', 'user@example.com', 'primary', 'healthy')",
    )
    .bind(source_id)
    .bind(tenant_id)
    .bind(user.id)
    .execute(&state.db_pool)
    .await
    .expect("insert google source");
    let event_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO calendar_events
            (id, tenant_id, owner_id, source_id, external_uid, title, starts_at, ends_at,
             timezone, status, read_only)
         VALUES ($1, $2, $3, $4, 'ext-1', 'Mirrored standup', '2026-10-05T09:00:00Z',
                 '2026-10-05T09:30:00Z', 'UTC', 'confirmed', true)",
    )
    .bind(event_id)
    .bind(tenant_id)
    .bind(user.id)
    .bind(source_id)
    .execute(&state.db_pool)
    .await
    .expect("insert mirrored event");

    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/calendar/events/{event_id}"))
                .method("PATCH")
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .body(Body::from(json!({"title": "Nope"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = response_json(response).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        body["error"],
        "Event is synchronized read-only from an external source"
    );

    // DELETE on the mirror is likewise rejected.
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/calendar/events/{event_id}"))
                .method("DELETE")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, _) = response_json(response).await;
    assert_eq!(status, StatusCode::CONFLICT);

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

/// Latest outbox envelope of a calendar event type for the tenant.
async fn latest_calendar_outbox_event(
    pool: &PgPool,
    tenant_id: Uuid,
    event_type: &str,
) -> Option<Value> {
    sqlx::query_scalar::<_, Value>(
        "SELECT event_json FROM integration_outbox \
         WHERE tenant_id = $1 AND event_type = $2 \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .bind(event_type)
    .fetch_optional(pool)
    .await
    .expect("query integration_outbox")
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
async fn internal_event_mutations_publish_minimal_outbox_envelopes() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_outbox", tenant_id).await;

    let created = state
        .calendar_service
        .create_event(
            tenant_id,
            user.id,
            rustshare_server::services::calendar_service::NewCalendarEvent {
                title: "Secret standup".to_string(),
                description: Some("classified agenda".to_string()),
                location: Some("Room 42".to_string()),
                starts_at: "2026-10-05T14:00:00Z".parse().unwrap(),
                ends_at: "2026-10-05T15:00:00Z".parse().unwrap(),
                all_day: false,
                timezone: "Europe/Berlin".to_string(),
                rrule: None,
            },
        )
        .await
        .expect("create event");

    let envelope = latest_calendar_outbox_event(
        &state.db_pool,
        tenant_id,
        "io.elembra.calendar.event.created.v1",
    )
    .await
    .expect("created envelope published");
    assert_eq!(envelope["source"], "elembra://io.elembra.calendar");
    assert_eq!(envelope["elembraTenant"], tenant_id.to_string());
    assert_eq!(envelope["data"]["event_id"], created.id.to_string());
    assert_eq!(envelope["elembraResource"]["resourceType"], "event");
    // Minimum-safe-data: titles/descriptions/locations never leak into events.
    for forbidden in ["title", "description", "location"] {
        assert!(
            envelope["data"].get(forbidden).is_none(),
            "data must not carry `{forbidden}`"
        );
    }

    let updated = state
        .calendar_service
        .update_event(
            tenant_id,
            user.id,
            created.id,
            rustshare_server::services::calendar_service::CalendarEventPatch {
                title: Some("Renamed secret".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect("update event");
    let envelope = latest_calendar_outbox_event(
        &state.db_pool,
        tenant_id,
        "io.elembra.calendar.event.updated.v1",
    )
    .await
    .expect("updated envelope published");
    assert_eq!(envelope["data"]["event_id"], updated.id.to_string());
    assert!(envelope["data"].get("title").is_none());

    state
        .calendar_service
        .delete_event(tenant_id, user.id, created.id)
        .await
        .expect("delete event");
    let envelope = latest_calendar_outbox_event(
        &state.db_pool,
        tenant_id,
        "io.elembra.calendar.event.deleted.v1",
    )
    .await
    .expect("deleted envelope published");
    assert_eq!(envelope["data"]["event_id"], created.id.to_string());

    cleanup_outbox(&state.db_pool, tenant_id).await;
    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn outbox_publish_rolls_back_with_source_transaction() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_outbox_rollback", tenant_id).await;

    let source = state
        .calendar_service
        .ensure_internal_source(tenant_id, user.id)
        .await
        .expect("internal source");
    let now = chrono::Utc::now();
    let event = rustshare_core::domain::CalendarEvent {
        id: Uuid::new_v4(),
        tenant_id,
        owner_id: user.id,
        source_id: source.id,
        external_uid: None,
        external_etag: None,
        recurrence_id: None,
        title: "Rollback probe".to_string(),
        description: None,
        location: None,
        starts_at: now + chrono::Duration::days(1),
        ends_at: now + chrono::Duration::days(1) + chrono::Duration::hours(1),
        all_day: false,
        original_date: None,
        timezone: "UTC".to_string(),
        rrule: None,
        status: "confirmed".to_string(),
        read_only: false,
        raw: None,
        deleted_at: None,
        created_at: now,
        updated_at: now,
    };
    let envelope = rustshare_server::services::calendar_service::build_event_envelope(
        tenant_id,
        user.id,
        &event,
        "io.elembra.calendar.event.created.v1",
    )
    .expect("envelope builds");

    // The mutation and the outbox insert commit or vanish together.
    let mut tx = state.db_pool.begin().await.expect("begin tx");
    state
        .metadata_store
        .create_calendar_event_in_tx(&mut tx, &event)
        .await
        .expect("insert event in tx");
    state
        .outbox_store
        .insert_in_tx(&mut tx, &envelope)
        .await
        .expect("insert outbox in tx");
    tx.rollback().await.expect("rollback");

    let event_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM calendar_events WHERE id = $1")
        .bind(event.id)
        .fetch_one(&state.db_pool)
        .await
        .expect("count events");
    assert_eq!(
        event_rows, 0,
        "rolled-back mutation must leave no event row"
    );
    let outbox_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM integration_outbox WHERE event_id = $1")
            .bind(envelope.id)
            .fetch_one(&state.db_pool)
            .await
            .expect("count outbox rows");
    assert_eq!(
        outbox_rows, 0,
        "rolled-back mutation must leave no outbox row"
    );

    cleanup_tenant(&state.db_pool, tenant_id).await;
}
