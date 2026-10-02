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
use axum::routing::get;
use chrono::{DateTime, Utc};
use rustshare_server::services::calendar_service::CalendarError;
use rustshare_server::services::google_calendar::{
    CalendarSyncConfig, GoogleCalendarClient, SyncOutcome,
};
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

/// Exactly the payload the Calendar UI sends for a create: the required
/// fields plus explicit nulls for the optional ones, with no `all_day` and no
/// `rrule` keys (serde defaults `all_day`, treats `rrule` as absent).
fn minimal_ui_event_body() -> Value {
    json!({
        "title": "Board meeting",
        "starts_at": "2026-10-14T09:00:00Z",
        "ends_at": "2026-10-14T10:00:00Z",
        "timezone": "Europe/Berlin",
        "description": null,
        "location": null
    })
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn create_accepts_the_minimal_ui_payload() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_minimal_ui", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/calendar/events")
                .method("POST")
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .body(Body::from(minimal_ui_event_body().to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = response_json(response).await;
    assert_eq!(status, StatusCode::CREATED, "body: {body}");
    assert_eq!(body["title"], "Board meeting");
    assert_eq!(body["timezone"], "Europe/Berlin");
    assert_eq!(body["starts_at"], "2026-10-14T09:00:00Z");
    assert!(body["description"].is_null());
    assert!(body["location"].is_null());

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn create_with_null_timezone_is_rejected_400() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_null_timezone", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());

    // The stale-bundle payload that produced the user's "JSON error".
    let mut payload = minimal_ui_event_body();
    payload["timezone"] = Value::Null;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/calendar/events")
                .method("POST")
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = response_json(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Invalid JSON payload");

    cleanup_tenant(&state.db_pool, tenant_id).await;
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

// ---------------------------------------------------------------------------
// B1: override suppression for recurring external events
// ---------------------------------------------------------------------------

async fn insert_google_source(pool: &PgPool, tenant_id: Uuid, owner_id: Uuid) -> Uuid {
    insert_google_source_with_account(pool, tenant_id, owner_id, "u@example.com").await
}

async fn insert_google_source_with_account(
    pool: &PgPool,
    tenant_id: Uuid,
    owner_id: Uuid,
    account: &str,
) -> Uuid {
    let source_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO calendar_sources
            (id, tenant_id, owner_id, kind, display_name, external_account, external_calendar_id, status)
         VALUES ($1, $2, $3, 'google', 'Test Google', $4, 'primary', 'healthy')",
    )
    .bind(source_id)
    .bind(tenant_id)
    .bind(owner_id)
    .bind(account)
    .execute(pool)
    .await
    .expect("insert google source");
    source_id
}

#[allow(clippy::too_many_arguments)]
async fn insert_mirrored_event(
    pool: &PgPool,
    tenant_id: Uuid,
    owner_id: Uuid,
    source_id: Uuid,
    external_uid: &str,
    recurrence_id: Option<&str>,
    title: &str,
    starts_at: &str,
    ends_at: &str,
    status: &str,
    rrule: Option<&str>,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO calendar_events
            (id, tenant_id, owner_id, source_id, external_uid, recurrence_id, title,
             starts_at, ends_at, timezone, rrule, status, read_only)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'UTC', $10, $11, true)",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(owner_id)
    .bind(source_id)
    .bind(external_uid)
    .bind(recurrence_id)
    .bind(title)
    .bind(starts_at.parse::<DateTime<Utc>>().unwrap())
    .bind(ends_at.parse::<DateTime<Utc>>().unwrap())
    .bind(rrule)
    .bind(status)
    .execute(pool)
    .await
    .expect("insert mirrored event");
    id
}

/// A cancelled override of a recurring master must not be regenerated as a
/// phantom occurrence: with `include_cancelled=false` the cancelled slot is
/// absent, and with `include_cancelled=true` it renders as a cancelled row.
#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn cancelled_recurring_override_suppresses_phantom_occurrence() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_override_cancelled", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;

    let source_id = insert_google_source(&state.db_pool, tenant_id, user.id).await;
    insert_mirrored_event(
        &state.db_pool,
        tenant_id,
        user.id,
        source_id,
        "series-1",
        None,
        "Weekly series",
        "2026-10-05T14:00:00Z",
        "2026-10-05T15:00:00Z",
        "confirmed",
        Some("FREQ=WEEKLY;COUNT=3"),
    )
    .await;
    // The cancelled occurrence is stored with its ORIGINAL slot as
    // recurrence_id (the master's start), but may be excluded by status.
    insert_mirrored_event(
        &state.db_pool,
        tenant_id,
        user.id,
        source_id,
        "series-1",
        Some("2026-10-12T14:00:00Z"),
        "Weekly series",
        "2026-10-12T14:00:00Z",
        "2026-10-12T15:00:00Z",
        "cancelled",
        None,
    )
    .await;

    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());

    // include_cancelled=false: master expands to 10-05 and 10-19 only.
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
    let starts: Vec<&str> = events
        .iter()
        .map(|event| event["instance_start"].as_str().unwrap())
        .collect();
    assert_eq!(starts, vec!["2026-10-05T14:00:00Z", "2026-10-19T14:00:00Z"]);
    assert!(
        !events
            .iter()
            .any(|event| event["starts_at"] == "2026-10-12T14:00:00Z"),
        "cancelled occurrence must not reappear as a phantom"
    );

    // include_cancelled=true: the cancelled override renders as its own row.
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/calendar/events?from=2026-10-01T00:00:00Z&to=2026-10-31T00:00:00Z&include_cancelled=true")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = response_json(response).await;
    assert_eq!(status, StatusCode::OK);
    let events = body["events"].as_array().unwrap();
    let cancelled = events
        .iter()
        .filter(|event| event["status"] == "cancelled")
        .collect::<Vec<_>>();
    assert_eq!(cancelled.len(), 1, "cancelled override must be listed");
    assert_eq!(cancelled[0]["starts_at"], "2026-10-12T14:00:00Z");
    // The master still expands to the two non-overridden occurrences.
    let instance_starts: Vec<&str> = events
        .iter()
        .filter_map(|event| event["instance_start"].as_str())
        .collect();
    assert_eq!(
        instance_starts,
        vec!["2026-10-05T14:00:00Z", "2026-10-19T14:00:00Z"]
    );

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

/// A moved override whose new time falls outside the requested window must
/// still suppress the master's original slot (which is inside the window).
#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn moved_recurring_override_outside_window_suppresses_original_slot() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_override_moved", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;

    let source_id = insert_google_source(&state.db_pool, tenant_id, user.id).await;
    insert_mirrored_event(
        &state.db_pool,
        tenant_id,
        user.id,
        source_id,
        "series-2",
        None,
        "Moved series",
        "2026-10-05T14:00:00Z",
        "2026-10-05T15:00:00Z",
        "confirmed",
        Some("FREQ=WEEKLY;COUNT=2"),
    )
    .await;
    // The 10-12 instance was moved to 11-20, outside the October window.
    insert_mirrored_event(
        &state.db_pool,
        tenant_id,
        user.id,
        source_id,
        "series-2",
        Some("2026-10-12T14:00:00Z"),
        "Moved series",
        "2026-11-20T09:00:00Z",
        "2026-11-20T10:00:00Z",
        "confirmed",
        None,
    )
    .await;

    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());
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
    assert_eq!(events.len(), 1, "only the 10-05 occurrence should render");
    assert_eq!(events[0]["instance_start"], "2026-10-05T14:00:00Z");
    assert!(
        !events
            .iter()
            .any(|event| event["starts_at"] == "2026-10-12T14:00:00Z"
                || event["instance_start"] == "2026-10-12T14:00:00Z"),
        "the moved slot must not appear at its original time"
    );

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

/// Override suppression is keyed by `(source_id, external_uid)`: two sources
/// exposing the same external UID must not let one source's override suppress
/// the other source's legitimate occurrence.
#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn override_in_one_source_does_not_suppress_other_source_with_same_uid() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_override_cross_source", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;

    let source_a = insert_google_source(&state.db_pool, tenant_id, user.id).await;
    let source_b =
        insert_google_source_with_account(&state.db_pool, tenant_id, user.id, "other@example.com")
            .await;

    // Both sources expose a recurring series under the SAME external UID.
    for source_id in [source_a, source_b] {
        insert_mirrored_event(
            &state.db_pool,
            tenant_id,
            user.id,
            source_id,
            "shared-uid",
            None,
            "Shared series",
            "2026-10-05T14:00:00Z",
            "2026-10-05T15:00:00Z",
            "confirmed",
            Some("FREQ=WEEKLY;COUNT=2"),
        )
        .await;
    }
    // Source A cancels its 10-12 instance.
    insert_mirrored_event(
        &state.db_pool,
        tenant_id,
        user.id,
        source_a,
        "shared-uid",
        Some("2026-10-12T14:00:00Z"),
        "Shared series",
        "2026-10-12T14:00:00Z",
        "2026-10-12T15:00:00Z",
        "cancelled",
        None,
    )
    .await;

    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());
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

    let source_b_starts: Vec<&str> = events
        .iter()
        .filter(|event| event["source_id"] == source_b.to_string())
        .map(|event| event["instance_start"].as_str().unwrap())
        .collect();
    assert_eq!(
        source_b_starts,
        vec!["2026-10-05T14:00:00Z", "2026-10-12T14:00:00Z"],
        "source B's 10-12 occurrence must survive source A's override"
    );
    let source_a_starts: Vec<&str> = events
        .iter()
        .filter(|event| event["source_id"] == source_a.to_string())
        .map(|event| event["instance_start"].as_str().unwrap())
        .collect();
    assert_eq!(source_a_starts, vec!["2026-10-05T14:00:00Z"]);

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

// ---------------------------------------------------------------------------
// B3: empty string clears optional PATCH fields
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn patch_empty_string_clears_optional_fields() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_clear_fields", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());

    // Create treats an empty string as "no value".
    let mut create_body = create_event_body();
    create_body["description"] = json!("");
    create_body["location"] = json!("");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/calendar/events")
                .method("POST")
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .body(Body::from(create_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = response_json(response).await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(body["description"].is_null());
    assert!(body["location"].is_null());
    let event_id = body["id"].as_str().unwrap().to_string();

    // Set recurrence + fields, then clear them via empty strings.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/calendar/events/{event_id}"))
                .method("PATCH")
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({"description": "note", "location": "Room 7", "rrule": "FREQ=WEEKLY;COUNT=2"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = response_json(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["description"], "note");
    assert_eq!(body["rrule"], "FREQ=WEEKLY;COUNT=2");

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/calendar/events/{event_id}"))
                .method("PATCH")
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({"description": "", "location": "", "rrule": ""}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = response_json(response).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["description"].is_null(), "empty string must clear");
    assert!(body["location"].is_null(), "empty string must clear");
    assert!(body["rrule"].is_null(), "empty string must stop recurrence");

    // A null value leaves the field unchanged.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/calendar/events/{event_id}"))
                .method("PATCH")
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({"description": null, "title": "Renamed"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = response_json(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["title"], "Renamed");
    assert!(body["description"].is_null());

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

// ---------------------------------------------------------------------------
// B4: concurrent soft-delete during update must not publish updated.v1
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn update_on_soft_deleted_event_returns_404_and_publishes_nothing() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_update_race", tenant_id).await;

    let created = state
        .calendar_service
        .create_event(
            tenant_id,
            user.id,
            rustshare_server::services::calendar_service::NewCalendarEvent {
                title: "Race".to_string(),
                description: None,
                location: None,
                starts_at: "2026-10-05T14:00:00Z".parse().unwrap(),
                ends_at: "2026-10-05T15:00:00Z".parse().unwrap(),
                all_day: false,
                timezone: "UTC".to_string(),
                rrule: None,
            },
        )
        .await
        .expect("create event");

    // Simulate the row being soft-deleted between the read and the write.
    sqlx::query("UPDATE calendar_events SET deleted_at = NOW() WHERE id = $1")
        .bind(created.id)
        .execute(&state.db_pool)
        .await
        .expect("soft delete row");

    let err = state
        .calendar_service
        .update_event(
            tenant_id,
            user.id,
            created.id,
            rustshare_server::services::calendar_service::CalendarEventPatch {
                title: Some("Renamed".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect_err("update of a soft-deleted row must fail");
    assert!(matches!(err, CalendarError::NotFound(_)), "got {err:?}");

    let updated_envelopes: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM integration_outbox \
         WHERE tenant_id = $1 AND event_type = 'io.elembra.calendar.event.updated.v1'",
    )
    .bind(tenant_id)
    .fetch_one(&state.db_pool)
    .await
    .expect("count updated envelopes");
    assert_eq!(
        updated_envelopes, 0,
        "no updated.v1 envelope may be published for a mutation that did not land"
    );

    cleanup_outbox(&state.db_pool, tenant_id).await;
    cleanup_tenant(&state.db_pool, tenant_id).await;
}

// B4b: the non-outbox update path (the fix that returns NotFound when
// `update_calendar_event_in_tx` reports false). The existing test above
// soft-deletes before the read, so it exits at the read; a true
// read-then-delete race cannot be forced deterministically here. This test
// therefore covers both halves it can: the storage helper reports `false` for a
// soft-deleted row, and the service's read path returns NotFound (not a stale
// 200) when built WITHOUT `configure_outbox`.
#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn update_on_soft_deleted_event_returns_not_found_without_outbox() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_update_nooutbox", tenant_id).await;

    // Mirror the harness's service construction but deliberately skip
    // `configure_outbox`, exercising the non-outbox branch.
    let service = rustshare_server::services::calendar_service::CalendarService::new(
        state.metadata_store.clone(),
        Arc::new(state.secret_key.clone()),
    );

    let created = service
        .create_event(
            tenant_id,
            user.id,
            rustshare_server::services::calendar_service::NewCalendarEvent {
                title: "No outbox".to_string(),
                description: None,
                location: None,
                starts_at: "2026-10-05T14:00:00Z".parse().unwrap(),
                ends_at: "2026-10-05T15:00:00Z".parse().unwrap(),
                all_day: false,
                timezone: "UTC".to_string(),
                rrule: None,
            },
        )
        .await
        .expect("create event");

    sqlx::query("UPDATE calendar_events SET deleted_at = NOW() WHERE id = $1")
        .bind(created.id)
        .execute(&state.db_pool)
        .await
        .expect("soft delete row");

    // Storage helper: a soft-deleted row yields Ok(false), the signal the
    // service branch relies on.
    let mut tx = state.db_pool.begin().await.expect("begin tx");
    let updated = state
        .metadata_store
        .update_calendar_event_in_tx(&mut tx, &created)
        .await
        .expect("update in tx");
    assert!(
        !updated,
        "update_calendar_event_in_tx must report a soft-deleted row as not updated"
    );
    tx.rollback().await.expect("rollback");

    // Service without an outbox must surface NotFound, not a stale 200.
    let err = service
        .update_event(
            tenant_id,
            user.id,
            created.id,
            rustshare_server::services::calendar_service::CalendarEventPatch {
                title: Some("Renamed".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect_err("update of a soft-deleted row must be NotFound");
    assert!(matches!(err, CalendarError::NotFound(_)), "got {err:?}");

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

// ---------------------------------------------------------------------------
// B5: disconnect lease coordination
// ---------------------------------------------------------------------------

async fn insert_oauth_source_with_tokens(state: &AppState, tenant_id: Uuid, user_id: Uuid) -> Uuid {
    let refresh_enc =
        rustshare_crypto::encrypt_secret("refresh-token-value", &state.secret_key).unwrap();
    let access_enc =
        rustshare_crypto::encrypt_secret("access-token-value", &state.secret_key).unwrap();
    state
        .metadata_store
        .create_oauth_calendar_source(
            tenant_id,
            user_id,
            "google",
            "Google (disconnect@test.local)",
            "disconnect@test.local",
            "primary",
            &refresh_enc,
            &access_enc,
            Utc::now() + chrono::Duration::hours(1),
            "scope",
        )
        .await
        .expect("create oauth source")
        .id
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn disconnect_without_lease_wipes_tokens_and_parks_source() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_disconnect_ok", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;
    let source_id = insert_oauth_source_with_tokens(&state, tenant_id, user.id).await;

    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/calendar/sources/{source_id}/disconnect"))
                .method("POST")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, _) = response_json(response).await;
    assert_eq!(status, StatusCode::OK);

    let reloaded = state
        .metadata_store
        .get_calendar_source(tenant_id, user.id, source_id)
        .await
        .expect("load source")
        .expect("source exists");
    assert_eq!(reloaded.status, "auth_required");
    assert!(reloaded.refresh_token_enc.is_none(), "tokens must be wiped");
    assert!(reloaded.access_token_enc.is_none(), "tokens must be wiped");

    // A parked source makes subsequent sync runs no-ops: `sync_source` returns
    // `Parked` before any provider call or token use.
    let mut client = GoogleCalendarClient::new(
        "id".to_string(),
        "secret".to_string(),
        "http://elembra.test",
    );
    client.api_base = "http://127.0.0.1:1/calendar/v3".to_string();
    let outcome = rustshare_server::services::google_calendar::sync_source(
        &state.metadata_store,
        &client,
        &state.secret_key,
        &reloaded,
        &CalendarSyncConfig {
            past_days: 90,
            future_days: 365,
        },
        "disconnect-test",
    )
    .await;
    assert!(
        matches!(outcome, SyncOutcome::Parked),
        "a parked source must no-op, got {outcome:?}"
    );

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn disconnect_while_lease_held_returns_409_and_keeps_tokens() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_disconnect_busy", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;
    let source_id = insert_oauth_source_with_tokens(&state, tenant_id, user.id).await;

    state
        .metadata_store
        .ensure_calendar_sync_state(source_id)
        .await
        .expect("sync state");
    sqlx::query(
        "UPDATE calendar_sync_states SET locked_at = NOW(), locked_by = 'disconnect-test' \
         WHERE source_id = $1",
    )
    .bind(source_id)
    .execute(&state.db_pool)
    .await
    .expect("acquire lease");

    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/calendar/sources/{source_id}/disconnect"))
                .method("POST")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, _) = response_json(response).await;
    assert_eq!(status, StatusCode::CONFLICT);

    let reloaded = state
        .metadata_store
        .get_calendar_source(tenant_id, user.id, source_id)
        .await
        .expect("load source")
        .expect("source exists");
    assert_eq!(
        reloaded.status, "healthy",
        "a rejected disconnect must not wipe"
    );
    assert!(reloaded.refresh_token_enc.is_some());
    assert!(reloaded.access_token_enc.is_some());

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

/// The lease guard lives inside the wipe itself, so a worker that claims the
/// source after the caller's pre-check cannot be overwritten: `false` is
/// returned and the tokens are left intact.
#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn wipe_tokens_is_lease_guarded() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_wipe_guard", tenant_id).await;
    let source_id = insert_oauth_source_with_tokens(&state, tenant_id, user.id).await;

    state
        .metadata_store
        .ensure_calendar_sync_state(source_id)
        .await
        .expect("sync state");
    sqlx::query(
        "UPDATE calendar_sync_states SET locked_at = NOW(), locked_by = 'wipe-guard' \
         WHERE source_id = $1",
    )
    .bind(source_id)
    .execute(&state.db_pool)
    .await
    .expect("acquire lease");

    let wiped = state
        .metadata_store
        .wipe_calendar_source_tokens(source_id, std::time::Duration::from_secs(300))
        .await
        .expect("lease-guarded wipe");
    assert!(!wiped, "a live lease must block the token wipe");

    let reloaded = state
        .metadata_store
        .get_calendar_source(tenant_id, user.id, source_id)
        .await
        .expect("load source")
        .expect("source exists");
    assert_eq!(reloaded.status, "healthy", "the wipe must not have run");
    assert!(reloaded.refresh_token_enc.is_some());
    assert!(reloaded.access_token_enc.is_some());

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

// ---------------------------------------------------------------------------
// B6 + T1: cross-tenant IDs are indistinguishable from unknown ones
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn cross_tenant_ids_are_not_visible() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_a = create_test_tenant(&state.db_pool).await;
    let tenant_b = create_test_tenant(&state.db_pool).await;
    let user_a = create_test_user(&state, "calendar_xtenant_a", tenant_a).await;
    let user_b = create_test_user(&state, "calendar_xtenant_b", tenant_b).await;
    configure_calendar(&state, tenant_a, user_a.id, true).await;
    configure_calendar(&state, tenant_b, user_b.id, true).await;

    let event_a = state
        .calendar_service
        .create_event(
            tenant_a,
            user_a.id,
            rustshare_server::services::calendar_service::NewCalendarEvent {
                title: "Tenant A private".to_string(),
                description: None,
                location: None,
                starts_at: "2026-10-05T14:00:00Z".parse().unwrap(),
                ends_at: "2026-10-05T15:00:00Z".parse().unwrap(),
                all_day: false,
                timezone: "UTC".to_string(),
                rrule: None,
            },
        )
        .await
        .expect("create event");
    let source_a = state
        .calendar_service
        .create_source(
            tenant_a,
            user_a.id,
            rustshare_core::domain::CalendarSourceKind::IcalImport,
            "Tenant A import".to_string(),
        )
        .await
        .expect("create source");
    let now = Utc::now();
    let job_a = rustshare_core::domain::CalendarImportJob {
        id: Uuid::new_v4(),
        tenant_id: tenant_a,
        owner_id: user_a.id,
        source_id: source_a.id,
        status: "pending".to_string(),
        filename: "tenant-a.ics".to_string(),
        size_bytes: 16,
        total_events: 0,
        processed_events: 0,
        failed_events: 0,
        last_error: None,
        started_at: None,
        completed_at: None,
        deleted_at: None,
        created_at: now,
        updated_at: now,
    };
    state
        .metadata_store
        .create_calendar_import_job(&job_a, b"BEGIN:VCALENDAR")
        .await
        .expect("create import job");

    let token_b = create_auth_token(&state, user_b.id, tenant_b);
    let app = build_app(state.clone());

    // Tenant-B user cannot read tenant-A's event, source, or import job.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/calendar/events/{}", event_a.id))
                .header("Authorization", format!("Bearer {token_b}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, _) = response_json(response).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "cross-tenant event must be 404"
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/calendar/sources/{}", source_a.id))
                .method("PATCH")
                .header("Authorization", format!("Bearer {token_b}"))
                .header("Content-Type", "application/json")
                .body(Body::from(json!({"display_name": "Hijacked"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, _) = response_json(response).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "cross-tenant source must be 404"
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/calendar/import-jobs/{}", job_a.id))
                .header("Authorization", format!("Bearer {token_b}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, _) = response_json(response).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "cross-tenant job must be 404"
    );

    // B6: DELETE of a foreign-tenant event is indistinguishable from a random
    // ID — an idempotent success, never a 404 that would confirm existence in
    // another tenant.
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/calendar/events/{}", event_a.id))
                .method("DELETE")
                .header("Authorization", format!("Bearer {token_b}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, _) = response_json(response).await;
    assert_eq!(status, StatusCode::OK);

    cleanup_tenant(&state.db_pool, tenant_a).await;
    cleanup_tenant(&state.db_pool, tenant_b).await;
}

// ---------------------------------------------------------------------------
// B7: oversized multipart text field is bounded
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn oversized_source_id_multipart_field_is_rejected() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_big_field", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());

    let boundary = "----rustshareTestBoundary";
    let oversized = "a".repeat(1024);
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"source_id\"\r\n\r\n\
         {oversized}\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"x.ics\"\r\n\
         Content-Type: text/calendar\r\n\r\nBEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n\
         --{boundary}--\r\n"
    );
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/calendar/import")
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
    assert_eq!(
        response.status(),
        StatusCode::PAYLOAD_TOO_LARGE,
        "an oversized source_id field must be rejected before parsing"
    );

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

// ---------------------------------------------------------------------------
// T2: malformed provider payload fails the run safely
// ---------------------------------------------------------------------------

async fn spawn_non_json_google() -> String {
    let app = axum::Router::new().route(
        "/calendar/v3/calendars/primary/events",
        get(|| async { (StatusCode::OK, "definitely not json") }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock server");
    let addr = listener.local_addr().expect("mock addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn malformed_provider_payload_fails_run_without_corruption() {
    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "calendar_bad_payload", tenant_id).await;
    configure_calendar(&state, tenant_id, user.id, true).await;

    let base = spawn_non_json_google().await;
    let secret_key = rustshare_crypto::SecretEncryptionKey::from_bytes([0u8; 32]);
    let refresh_enc = rustshare_crypto::encrypt_secret("refresh", &secret_key).unwrap();
    let access_enc = rustshare_crypto::encrypt_secret("access", &secret_key).unwrap();
    let source = state
        .metadata_store
        .create_oauth_calendar_source(
            tenant_id,
            user.id,
            "google",
            "Google (bad-payload@test.local)",
            "bad-payload@test.local",
            "primary",
            &refresh_enc,
            &access_enc,
            Utc::now() + chrono::Duration::hours(1),
            "scope",
        )
        .await
        .expect("create source");

    state
        .metadata_store
        .ensure_calendar_sync_state(source.id)
        .await
        .expect("sync state");
    sqlx::query(
        "UPDATE calendar_sync_states SET locked_at = NOW(), locked_by = 'bad-payload' \
         WHERE source_id = $1",
    )
    .bind(source.id)
    .execute(&state.db_pool)
    .await
    .expect("acquire lease");

    let mut client = GoogleCalendarClient::new(
        "id".to_string(),
        "secret".to_string(),
        "http://elembra.test",
    );
    client.api_base = format!("{base}/calendar/v3");
    client.token_url = format!("{base}/token");

    let outcome = rustshare_server::services::google_calendar::sync_source(
        &state.metadata_store,
        &client,
        &secret_key,
        &source,
        &CalendarSyncConfig {
            past_days: 90,
            future_days: 365,
        },
        "bad-payload",
    )
    .await;
    assert!(
        matches!(outcome, SyncOutcome::Failed(_)),
        "non-JSON provider body must fail the run, got {outcome:?}"
    );

    let event_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM calendar_events WHERE source_id = $1")
            .bind(source.id)
            .fetch_one(&state.db_pool)
            .await
            .expect("count events");
    assert_eq!(event_count, 0, "no events may be materialized");

    let reloaded = state
        .metadata_store
        .get_calendar_source(tenant_id, user.id, source.id)
        .await
        .expect("load source")
        .expect("source exists");
    assert_eq!(reloaded.status, "healthy");
    assert!(reloaded.refresh_token_enc.is_some());
    assert!(reloaded.access_token_enc.is_some());

    cleanup_tenant(&state.db_pool, tenant_id).await;
}
