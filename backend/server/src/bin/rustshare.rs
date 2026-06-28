//! RustShare CLI binary.
//!
//! Currently supports a single operator subcommand:
//!
//!   rustshare migrate-notes-okf [--dry-run] [--format json|text]
//!
//! The command loads the same environment configuration as the server, builds a
//! `NoteService`, and runs the OKF-native notes migration.

use anyhow::Result;
use rustshare_core::events::EventBroadcaster;
use rustshare_core::services::{FileService, FolderService, PermissionResolver};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_server::config::AppConfig;
use rustshare_server::services::note_service::NoteService;
use rustshare_storage::{EventStore, MetadataStore, ObjectStore};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let args: Vec<String> = std::env::args().collect();
    let mut dry_run = false;
    let mut format = "text".to_string();
    let mut subcommand: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "migrate-notes-okf" => subcommand = Some("migrate-notes-okf".to_string()),
            "--dry-run" => dry_run = true,
            "--format" => {
                i += 1;
                if i < args.len() {
                    format = args[i].clone();
                }
            }
            _ => {}
        }
        i += 1;
    }

    if subcommand.as_deref() != Some("migrate-notes-okf") {
        eprintln!("Usage: rustshare migrate-notes-okf [--dry-run] [--format json|text]");
        std::process::exit(1);
    }

    if format != "json" && format != "text" {
        eprintln!("Invalid format: {}. Use json or text.", format);
        std::process::exit(1);
    }

    let config = AppConfig::from_env().map_err(|errors| {
        eprintln!("\n❌ Configuration errors — migration cannot start:\n");
        for error in &errors {
            eprintln!("  ✗ {}", error);
        }
        eprintln!();
        anyhow::anyhow!("Configuration invalid")
    })?;

    let db_pool = PgPoolOptions::new()
        .max_connections(5)
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&config.database_url)
        .await?;

    sqlx::migrate!("../migrations").run(&db_pool).await?;

    let metadata_store = Arc::new(MetadataStore::new(db_pool.clone()));
    let event_store = Arc::new(EventStore::new(db_pool.clone()));
    let object_store = Arc::new(
        ObjectStore::new_with_options(
            config.rustfs_endpoint.clone(),
            config.rustfs_region.clone(),
            config.rustfs_bucket.clone(),
            rustshare_storage::ObjectStoreOptions {
                auto_create_bucket: config.object_store_auto_create_bucket,
            },
        )
        .await?,
    );

    let broadcaster = Arc::new(EventBroadcaster::new(100));
    let permission_resolver = Arc::new(PermissionResolver::new(Arc::new(
        PermissionResolverRepository::new(db_pool.clone()),
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

    let note_service = NoteService::new(
        file_service,
        folder_service,
        metadata_store,
        object_store,
        permission_resolver,
    );

    let tenant_id = std::env::var("RUSTSHARE_DEFAULT_TENANT_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(uuid::Uuid::nil);

    info!(
        tenant_id = %tenant_id,
        dry_run,
        "Starting OKF notes migration"
    );

    let report = note_service
        .migrate_notes_to_okf(tenant_id, dry_run)
        .await?;

    if format == "json" {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Notes migration report:");
        println!("  scanned:               {}", report.notes_scanned);
        println!("  already OKF:           {}", report.already_okf);
        println!("  missing frontmatter:   {}", report.missing_frontmatter);
        println!("  frontmatter to merge:  {}", report.frontmatter_to_merge);
        println!("  planned changes:       {}", report.planned_changes.len());
        println!("  conflicts:             {}", report.conflicts.len());
        println!("  skipped:               {}", report.skipped.len());

        if !report.planned_changes.is_empty() {
            println!("\nPlanned changes:");
            for change in &report.planned_changes {
                println!(
                    "  - {}\n    frontmatter: {}, manifest: {}, risk: {}, title_source: {}",
                    change.path,
                    change.frontmatter_action,
                    change.manifest_action,
                    change.risk_level,
                    change.title_source
                );
            }
        }

        if !report.conflicts.is_empty() {
            println!("\nConflicts:");
            for conflict in &report.conflicts {
                println!(
                    "  - {} [{}]: {}",
                    conflict.path, conflict.kind, conflict.message
                );
            }
        }

        if !report.skipped.is_empty() {
            println!("\nSkipped:");
            for skip in &report.skipped {
                println!("  - {}: {}", skip.path, skip.reason);
            }
        }
    }

    Ok(())
}
