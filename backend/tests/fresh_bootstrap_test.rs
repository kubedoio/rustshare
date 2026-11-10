//! Release-blocker regression: fresh-install bootstrap must succeed.
//!
//! After the #210/#217 Application cutover, a brand-new database crashed at
//! boot: the code reads/writes `templates.application_config`, but no migration
//! ever renamed the column from its legacy name (`module_config`). The server
//! therefore died while seeding default templates with:
//!
//! ```text
//! Failed to seed default templates: ... column "application_config" of
//! relation "templates" does not exist
//! ```
//!
//! This test reproduces the real failure path end-to-end against a genuinely
//! fresh database:
//!
//! 1. create a brand-new database;
//! 2. run the full migration chain from source (the same embedded migrations
//!    the server applies at boot);
//! 3. assert the schema is correct (`application_config` present,
//!    `module_config` gone);
//! 4. construct the real service graph (as `bootstrap::run` does) and call
//!    `TemplateService::ensure_default_templates` — the exact call that
//!    crashed the fresh-install boot;
//! 5. verify the default templates were actually inserted and their JSON
//!    `application_config` is readable.
//!
//! A schema-only test would not catch this: the crash is in the seeding SQL
//! against the real schema.
//!
//! Run with: cargo test --test fresh_bootstrap_test -- --ignored
//! (requires DATABASE_URL and S3-compatible object storage, as CI provides).

use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use rustshare_core::events::EventBroadcaster;
use rustshare_core::services::{FileService, FolderService, PermissionResolver};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_server::services::template_service::TemplateService;
use rustshare_storage::{EventStore, MetadataStore, ObjectStore, ObjectStoreOptions};

fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string())
}
/// Create a brand-new, uniquely named database on the same server, and return
/// its connection URL together with the admin pool (kept open so the database
/// can be dropped afterwards).
async fn create_fresh_database() -> (PgPool, String) {
    let admin_url = database_url();
    let admin_pool = PgPool::connect(&admin_url)
        .await
        .expect("Failed to connect to database");

    let db_name = format!("rustshare_fresh_boot_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE DATABASE {db_name}"))
        .execute(&admin_pool)
        .await
        .expect("Failed to create fresh test database");

    // Build the connection URL for the fresh database by replacing the
    // database component of the original URL.
    let fresh_url = if let Some(idx) = admin_url.rfind('/') {
        format!("{}/{}", &admin_url[..idx + 1], db_name)
    } else {
        panic!("DATABASE_URL does not look like postgres://host/db: {admin_url}")
    };

    (admin_pool, fresh_url)
}

async fn drop_fresh_database(admin_pool: &PgPool, fresh_url: &str) {
    // Terminate any lingering connections to the fresh database before
    // dropping it (e.g. after a failed assertion that leaks the pool).
    let db_name = fresh_url.rsplit('/').next().unwrap_or_default();
    sqlx::query(&format!(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{db_name}' AND pid <> pg_backend_pid()"
    ))
    .execute(admin_pool)
    .await
    .ok();
    sqlx::query(&format!("DROP DATABASE IF EXISTS {db_name}"))
        .execute(admin_pool)
        .await
        .expect("Failed to drop fresh test database");
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn fresh_database_bootstrap_seeds_default_templates() {
    let (admin_pool, fresh_url) = create_fresh_database().await;

    let result = fresh_database_bootstrap_inner(&fresh_url).await;

    drop_fresh_database(&admin_pool, &fresh_url).await;
    result.expect("fresh-database bootstrap must succeed");
}

async fn fresh_database_bootstrap_inner(fresh_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Full migration chain on a brand-new database — the same embedded
    //    migrations the server applies at boot (`bootstrap.rs`).
    let pool = PgPool::connect(fresh_url).await?;
    sqlx::migrate!("../migrations").run(&pool).await?;

    // 2. Schema assertions: the renamed column exists, the legacy one is gone.
    let has_application_config: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_name = 'templates' AND column_name = 'application_config'
        )",
    )
    .fetch_one(&pool)
    .await?;
    assert!(
        has_application_config,
        "templates.application_config must exist after migrations"
    );

    let has_module_config: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_name = 'templates' AND column_name = 'module_config'
        )",
    )
    .fetch_one(&pool)
    .await?;
    assert!(
        !has_module_config,
        "legacy templates.module_config must not remain after migrations"
    );

    // 3. Construct the real service graph the way bootstrap does.
    let metadata_store = Arc::new(MetadataStore::new(pool.clone()));
    let event_store = Arc::new(EventStore::new(pool.clone()));

    let s3_endpoint = std::env::var("RUSTFS_ENDPOINT")
        .or_else(|_| std::env::var("RUSTFS_PUBLIC_ENDPOINT"))
        .unwrap_or_else(|_| "http://localhost:9000".to_string());
    let s3_region = std::env::var("RUSTFS_REGION").unwrap_or_else(|_| "us-east-1".to_string());
    let s3_bucket =
        std::env::var("RUSTFS_BUCKET").unwrap_or_else(|_| "rustshare-files".to_string());
    let object_store = Arc::new(
        ObjectStore::new_with_options(
            s3_endpoint,
            s3_region,
            s3_bucket,
            ObjectStoreOptions {
                auto_create_bucket: true,
            },
        )
        .await?,
    );

    let broadcaster = Arc::new(EventBroadcaster::new(100));
    let permission_resolver = Arc::new(PermissionResolver::new(Arc::new(
        PermissionResolverRepository::new(pool.clone()),
    )));

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
    let template_service = TemplateService::new(file_service, folder_service, metadata_store);

    // 4. THE regression: this exact call crashed the fresh-install boot with
    //    `column "application_config" of relation "templates" does not exist`.
    let default_tenant_id = std::env::var("RUSTSHARE_DEFAULT_TENANT_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(Uuid::nil);
    template_service
        .ensure_default_templates(default_tenant_id)
        .await
        .map_err(|e| format!("default template seeding failed: {e}"))?;

    // 5. Default templates were actually inserted and are readable.
    let template_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM templates WHERE tenant_id = $1 AND system_template = true",
    )
    .bind(default_tenant_id)
    .fetch_one(&pool)
    .await?;
    assert!(
        template_count >= 5,
        "expected at least 5 seeded system templates, found {template_count}"
    );

    // The kanban template carries real application_config JSON; verify it
    // round-trips through the renamed column.
    let kanban_config: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT application_config FROM templates
         WHERE template_key = 'template_default_kanban' AND tenant_id = $1",
    )
    .bind(default_tenant_id)
    .fetch_one(&pool)
    .await?;
    let kanban_config = kanban_config.expect("kanban template must have an application_config");
    assert!(
        kanban_config.get("kanban").is_some(),
        "kanban application_config must contain its config, got: {kanban_config}"
    );

    Ok(())
}
