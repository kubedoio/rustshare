mod handlers;

use anyhow::Result;
use axum::{
    routing::{delete, get, post, put},
    Json, Router,
};
use rustshare_auth::{JwtManager, PasswordHasher};
use rustshare_core::{
    domain::User,
    services::{FileService, FolderService},
};
use rustshare_storage::{EventStore, MetadataStore, ObjectStore};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing::info;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub db_pool: PgPool,
    pub metadata_store: Arc<MetadataStore>,
    pub event_store: Arc<EventStore>,
    pub object_store: Arc<ObjectStore>,
    pub jwt_manager: Arc<JwtManager>,
    pub file_service: Arc<FileService<EventStore, MetadataStore, ObjectStore>>,
    pub folder_service: Arc<FolderService<EventStore, MetadataStore>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables
    dotenv::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,rustshare=debug".to_string()),
        )
        .init();

    info!("Starting RustShare server");

    // Connect to database
    let database_url = std::env::var("DATABASE_URL")?;
    let db_pool = PgPool::connect(&database_url).await?;

    info!("Connected to database");

    // Run migrations (path relative to workspace root)
    sqlx::migrate!("../migrations")
        .run(&db_pool)
        .await?;

    info!("Database migrations applied");

    // Initialize stores
    let metadata_store = Arc::new(MetadataStore::new(db_pool.clone()));
    let event_store = Arc::new(EventStore::new(db_pool.clone()));

    // Initialize object store
    let rustfs_endpoint = std::env::var("RUSTFS_ENDPOINT")?;
    let rustfs_region = std::env::var("RUSTFS_REGION")?;
    let rustfs_bucket = std::env::var("RUSTFS_BUCKET")?;

    let object_store = Arc::new(
        ObjectStore::new(rustfs_endpoint, rustfs_region, rustfs_bucket).await?,
    );

    info!("Object store initialized");

    // Initialize JWT manager
    let jwt_secret = std::env::var("JWT_SECRET")?;
    let jwt_manager = Arc::new(JwtManager::new(jwt_secret));

    // Initialize services
    let file_service = Arc::new(FileService::new(
        Arc::clone(&event_store),
        Arc::clone(&metadata_store),
        Arc::clone(&object_store),
    ));
    let folder_service = Arc::new(FolderService::new(
        Arc::clone(&event_store),
        Arc::clone(&metadata_store),
    ));

    // Bootstrap admin user if no users exist
    if !metadata_store.has_users().await? {
        let admin_username = std::env::var("RUSTSHARE_ADMIN_USERNAME")?;
        let admin_email = std::env::var("RUSTSHARE_ADMIN_EMAIL")?;
        let admin_password = std::env::var("RUSTSHARE_ADMIN_PASSWORD")?;

        let password_hash = PasswordHasher::hash(&admin_password)?;
        let admin_user = User::new(
            admin_username.clone(),
            "Administrator".to_string(),
            password_hash,
            admin_email.clone(),
            true,
            10_737_418_240, // 10GB default quota
        );

        metadata_store.create_user(&admin_user).await?;

        info!("Admin user created: {} ({})", admin_username, admin_email);
    }

    // Build application state
    let state = AppState {
        db_pool,
        metadata_store,
        event_store,
        object_store,
        jwt_manager,
        file_service,
        folder_service,
    };

    // Build router
    let app = Router::new()
        // Health check
        .route("/health", get(health_check))
        // Auth
        .route("/api/auth/login", post(login))
        // File routes (Task 15-19)
        .route("/api/files/upload", post(handlers::upload_file))
        .route("/api/files/:id", get(handlers::get_file))
        .route("/api/files/:id", put(handlers::update_file))
        .route("/api/files/:id", delete(handlers::delete_file))
        .route("/api/files/:id/download", get(handlers::download_file))
        .route("/api/files/:id/versions", get(handlers::get_file_versions))
        .route("/api/files/:id/restore", post(handlers::restore_file_version))
        .route("/api/files/:id/move", post(handlers::move_file))
        .route("/api/files/:id/rename", post(handlers::rename_file))
        // Folder routes (Task 20-22)
        .route("/api/folders", post(handlers::create_folder))
        .route("/api/folders/:id", get(handlers::get_folder))
        .route("/api/folders/:id", delete(handlers::delete_folder))
        .route("/api/folders/:id/contents", get(handlers::get_folder_contents))
        .route("/api/folders/tree", get(handlers::get_folder_tree))
        .route("/api/folders/:id/move", post(handlers::move_folder))
        .route("/api/folders/:id/rename", post(handlers::rename_folder))
        // Tracing
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Start server
    let host = std::env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = std::env::var("SERVER_PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("{}:{}", host, port);

    info!("Server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Health check endpoint
async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

#[derive(Serialize)]
struct HealthResponse {
    status: String,
}

/// Login request
#[derive(Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

/// Login response
#[derive(Serialize)]
struct LoginResponse {
    token: String,
    user: UserResponse,
}

#[derive(Serialize)]
struct UserResponse {
    id: String,
    email: String,
    display_name: String,
    is_admin: bool,
}

/// Login handler
async fn login(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (axum::http::StatusCode, String)> {
    // Find user
    let user = state
        .metadata_store
        .find_user_by_email(&req.email)
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                axum::http::StatusCode::UNAUTHORIZED,
                "Invalid credentials".to_string(),
            )
        })?;

    // Verify password
    let is_valid = PasswordHasher::verify(&req.password, &user.password_hash)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !is_valid {
        return Err((
            axum::http::StatusCode::UNAUTHORIZED,
            "Invalid credentials".to_string(),
        ));
    }

    // Generate JWT
    let token = state
        .jwt_manager
        .generate(user.id, user.email.clone())
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(LoginResponse {
        token,
        user: UserResponse {
            id: user.id.to_string(),
            email: user.email,
            display_name: user.display_name,
            is_admin: user.is_admin,
        },
    }))
}
