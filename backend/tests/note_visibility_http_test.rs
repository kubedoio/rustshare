//! HTTP-level authorization tests for note publication
//! (`POST /api/v1/notes/{id}/visibility`).
//!
//! Publication changes a note's exposure boundary, so it must require
//! ownership or the highest delegated share tier (Admin). View-only and Edit
//! share recipients, and users with no share at all, must be rejected with
//! 403 and the note's visibility / public index must remain unchanged.
//!
//! Run with: cargo test --test note_visibility_http_test -- --ignored
//!
//! All tests are ignored because they require a live database and S3.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rustshare_server::middleware;
use rustshare_server::routes;
use rustshare_server::state::AppState;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower::ServiceExt;
use uuid::Uuid;

mod contracts;
use contracts::common::share_file_to_user;
use rustshare_core::domain::SharePermissions;

async fn setup_test_env() -> AppState {
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
        poll_rate_limiter: Arc::new(Mutex::new(HashMap::new())),
        default_tenant_id: Uuid::nil(),
        default_tenant_config: rustshare_server::config::DefaultTenantConfig::Unset,
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

fn build_app(state: AppState) -> axum::Router {
    routes::note_routes()
        .merge(routes::note_public_routes())
        .route(
            "/api/v1/features",
            axum::routing::get(rustshare_server::handlers::get_features),
        )
        .with_state(state)
        .layer(axum::middleware::from_fn(
            middleware::security_headers_middleware,
        ))
}

#[tokio::test]
async fn features_endpoint_rejects_unauthenticated_requests() {
    let response = build_app(setup_test_env().await)
        .oneshot(
            Request::get("/api/v1/features")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

async fn setup_test_tenant(pool: &PgPool) -> Uuid {
    let tenant_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO tenants (id, name, created_at, updated_at)
        VALUES ($1, $2, NOW(), NOW())
        ON CONFLICT (id) DO NOTHING
        "#,
    )
    .bind(tenant_id)
    .bind(format!("Note Visibility Test Tenant {}", tenant_id))
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
    let unique_username = format!("{}_{}", username, Uuid::new_v4());
    let user = rustshare_core::domain::User::new(
        unique_username.clone(),
        format!("{} Display", username),
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

fn auth_token(state: &AppState, user_id: Uuid, tenant_id: Uuid) -> String {
    state
        .jwt_manager
        .generate(user_id, "test@example.com", tenant_id)
        .unwrap()
}

fn visibility_request(note_id: Uuid, token: &str) -> Request<Body> {
    Request::builder()
        .uri(format!("/api/v1/notes/{}/visibility", note_id))
        .method("POST")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap()
}

fn get_note_request(note_id: Uuid, token: &str) -> Request<Body> {
    Request::builder()
        .uri(format!("/api/v1/notes/{}", note_id))
        .method("GET")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap()
}

fn public_note_request(share_id: &str) -> Request<Body> {
    Request::builder()
        .uri(format!("/api/v1/public/notes/{}", share_id))
        .method("GET")
        .body(Body::empty())
        .unwrap()
}

async fn response_json(response: axum::http::Response<Body>) -> serde_json::Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn cleanup_tenant(pool: &PgPool, tenant_id: Uuid) {
    // Order matters for foreign key constraints.
    let queries = [
        "DELETE FROM file_thumbnails WHERE tenant_id = $1",
        "DELETE FROM file_versions WHERE tenant_id = $1",
        "DELETE FROM shares WHERE tenant_id = $1",
        "DELETE FROM files WHERE tenant_id = $1",
        "DELETE FROM folders WHERE tenant_id = $1",
        "DELETE FROM notifications WHERE tenant_id = $1",
        "DELETE FROM users WHERE tenant_id = $1",
        "DELETE FROM tenants WHERE id = $1",
    ];
    for sql in &queries {
        sqlx::query(sql).bind(tenant_id).execute(pool).await.ok();
    }
}

async fn seed_note(state: &AppState, owner_id: Uuid, tenant_id: Uuid, title: &str) -> Uuid {
    state
        .note_service
        .create_note(
            owner_id,
            tenant_id,
            Some(title.to_string()),
            None,
            Some("note body".to_string()),
        )
        .await
        .expect("owner creates note")
        .id
}

/// Regression: a View-only share recipient could publish (and unpublish) the
/// owner's note through the real HTTP route. Must be 403 with state intact.
#[tokio::test]
#[ignore = "Requires database and S3"]
async fn test_view_share_recipient_cannot_publish_note_via_http() {
    let state = setup_test_env().await;
    let tenant_id = setup_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "http_vis_owner_v", tenant_id).await;
    let viewer = create_test_user(&state, "http_vis_viewer_v", tenant_id).await;
    let note_id = seed_note(&state, owner.id, tenant_id, "HttpVisView").await;
    share_file_to_user(
        &state.metadata_store,
        note_id,
        owner.id,
        viewer.id,
        SharePermissions::View,
        tenant_id,
    )
    .await;

    let app = build_app(state.clone());
    let viewer_token = auth_token(&state, viewer.id, tenant_id);
    let owner_token = auth_token(&state, owner.id, tenant_id);

    // View-only recipient is denied publishing.
    let response = app
        .clone()
        .oneshot(visibility_request(note_id, &viewer_token))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "View-only share recipient must not be able to publish over HTTP"
    );

    // The note is still private and has no public share id.
    let response = app
        .clone()
        .oneshot(get_note_request(note_id, &owner_token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = response_json(response).await;
    assert_eq!(json["metadata"]["visibility"], "private");
    assert!(
        json["metadata"]["public_share_id"].is_null(),
        "denied publish must not allocate a public share id"
    );

    // Owner publishes; the View-only recipient is denied unpublishing too.
    let response = app
        .clone()
        .oneshot(visibility_request(note_id, &owner_token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = response_json(response).await;
    assert_eq!(json["visibility"], "public");
    let share_id = json["public_share_id"].as_str().unwrap().to_string();

    let response = app
        .clone()
        .oneshot(visibility_request(note_id, &viewer_token))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "View-only share recipient must not be able to unpublish over HTTP"
    );

    // Public access still works: the denied unpublish changed nothing.
    let response = app
        .clone()
        .oneshot(public_note_request(&share_id))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "denied unpublish must not remove the note from the public index"
    );

    // Owner unpublishes so no public index object is left behind in the
    // shared test bucket.
    let response = app
        .clone()
        .oneshot(visibility_request(note_id, &owner_token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = response_json(response).await;
    assert_eq!(json["visibility"], "private");

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn test_edit_share_recipient_cannot_toggle_visibility_via_http() {
    let state = setup_test_env().await;
    let tenant_id = setup_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "http_vis_owner_e", tenant_id).await;
    let editor = create_test_user(&state, "http_vis_editor_e", tenant_id).await;
    let note_id = seed_note(&state, owner.id, tenant_id, "HttpVisEdit").await;
    share_file_to_user(
        &state.metadata_store,
        note_id,
        owner.id,
        editor.id,
        SharePermissions::Edit,
        tenant_id,
    )
    .await;

    let app = build_app(state.clone());
    let editor_token = auth_token(&state, editor.id, tenant_id);
    let owner_token = auth_token(&state, owner.id, tenant_id);

    let response = app
        .clone()
        .oneshot(visibility_request(note_id, &editor_token))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "Edit share recipient must not be able to change public visibility over HTTP"
    );

    let response = app
        .clone()
        .oneshot(get_note_request(note_id, &owner_token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = response_json(response).await;
    assert_eq!(json["metadata"]["visibility"], "private");
    assert!(
        json["metadata"]["public_share_id"].is_null(),
        "denied toggle must not allocate a public share id"
    );

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn test_no_share_user_cannot_toggle_visibility_via_http() {
    let state = setup_test_env().await;
    let tenant_id = setup_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "http_vis_owner_n", tenant_id).await;
    let stranger = create_test_user(&state, "http_vis_stranger_n", tenant_id).await;
    let note_id = seed_note(&state, owner.id, tenant_id, "HttpVisNone").await;

    let app = build_app(state.clone());
    let stranger_token = auth_token(&state, stranger.id, tenant_id);

    let response = app
        .oneshot(visibility_request(note_id, &stranger_token))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "user without any share must not be able to change public visibility over HTTP"
    );

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn test_admin_share_recipient_can_publish_via_http() {
    let state = setup_test_env().await;
    let tenant_id = setup_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "http_vis_owner_a", tenant_id).await;
    let admin = create_test_user(&state, "http_vis_admin_a", tenant_id).await;
    let note_id = seed_note(&state, owner.id, tenant_id, "HttpVisAdmin").await;
    share_file_to_user(
        &state.metadata_store,
        note_id,
        owner.id,
        admin.id,
        SharePermissions::Admin,
        tenant_id,
    )
    .await;

    let app = build_app(state.clone());
    let admin_token = auth_token(&state, admin.id, tenant_id);
    let owner_token = auth_token(&state, owner.id, tenant_id);

    let response = app
        .clone()
        .oneshot(visibility_request(note_id, &admin_token))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Admin share recipient should be able to publish over HTTP"
    );
    let json = response_json(response).await;
    assert_eq!(json["visibility"], "public");
    let share_id = json["public_share_id"].as_str().unwrap().to_string();

    let response = app
        .clone()
        .oneshot(public_note_request(&share_id))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Owner unpublishes so no public index object is left behind in the
    // shared test bucket.
    let response = app
        .clone()
        .oneshot(visibility_request(note_id, &owner_token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = response_json(response).await;
    assert_eq!(json["visibility"], "private");

    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn test_owner_can_publish_and_unpublish_via_http() {
    let state = setup_test_env().await;
    let tenant_id = setup_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "http_vis_owner_p", tenant_id).await;
    let note_id = seed_note(&state, owner.id, tenant_id, "HttpVisOwner").await;

    let app = build_app(state.clone());
    let owner_token = auth_token(&state, owner.id, tenant_id);

    // Owner publishes.
    let response = app
        .clone()
        .oneshot(visibility_request(note_id, &owner_token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = response_json(response).await;
    assert_eq!(json["visibility"], "public");
    let share_id = json["public_share_id"].as_str().unwrap().to_string();

    // Public route serves the note while published.
    let response = app
        .clone()
        .oneshot(public_note_request(&share_id))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = response_json(response).await;
    assert_eq!(json["title"], "HttpVisOwner");

    // Owner unpublishes.
    let response = app
        .clone()
        .oneshot(visibility_request(note_id, &owner_token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = response_json(response).await;
    assert_eq!(json["visibility"], "private");

    // Public route no longer serves the note.
    let response = app
        .clone()
        .oneshot(public_note_request(&share_id))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "unpublished note must not be readable through the public route"
    );

    cleanup_tenant(&state.db_pool, tenant_id).await;
}
