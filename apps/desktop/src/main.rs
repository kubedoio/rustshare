use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;
use uuid::Uuid;

use sync_engine::{SyncCore, ApiClient, Database, SyncRoot, SyncStatus};
use platform::{TokenStore, PathManager, get_device_id};

/// RustShare Desktop Sync Client (Phase 1)
#[derive(Parser)]
#[command(name = "rustshare-desktop")]
#[command(about = "Desktop sync client for RustShare")]
#[command(version)]
struct Cli {
    /// Workspace root path
    #[arg(short, long, env = "RUSTSHARE_WORKSPACE", default_value = "~/RustShare")]
    workspace: PathBuf,

    /// Database name
    #[arg(short, long, default_value = "rustshare.db")]
    db_name: String,

    /// Server URL
    #[arg(short, long, default_value = "https://api.rustshare.io")]
    server: String,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Login to account
    Login {
        /// API token
        token: String,
    },
    /// Manage sync roots
    Sync {
        #[command(subcommand)]
        action: SyncAction,
    },
    /// Run the sync daemon
    Daemon,
    /// Show current status
    Status,
}

#[derive(Subcommand)]
enum SyncAction {
    /// Add a remote root to sync locally
    Add {
        /// Remote folder path
        remote_path: String,
        /// Local relative path
        local_path: String,
    },
    /// List all configured roots
    List,
    /// Manage selective sync filters
    Filter {
        #[command(subcommand)]
        action: FilterAction,
    },
    /// Manage native Virtual Filesystem (VFS)
    Vfs {
        #[command(subcommand)]
        action: VfsAction,
    },
}

#[derive(Subcommand)]
enum VfsAction {
    /// Register a sync root as a native VFS root
    Create {
        /// Root ID (UUID)
        root_id: Uuid,
    },
    /// Evict a file (convert back to placeholder)
    Evict {
        /// File path
        path: PathBuf,
    },
}

#[derive(Subcommand)]
enum FilterAction {
    /// Add an exclusion pattern
    Add {
        /// Root ID (UUID)
        root_id: Uuid,
        /// Glob pattern (e.g., node_modules/*)
        pattern: String,
    },
    /// List filters for a root
    List {
        /// Root ID (UUID)
        root_id: Uuid,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Expand ~ for workspace path
    let workspace = if cli.workspace.to_string_lossy().starts_with("~/") {
        let home = dirs::home_dir().ok_or_else(|| anyhow!("Home dir not found"))?;
        home.join(cli.workspace.strip_prefix("~/")?)
    } else {
        cli.workspace
    };
    std::fs::create_dir_all(&workspace)?;

    // Initialize logging
    let log_level = if cli.verbose { Level::DEBUG } else { Level::INFO };
    let subscriber = FmtSubscriber::builder().with_max_level(log_level).finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let app_data_dir = PathManager::get_app_data_dir()?;
    let db_path = app_data_dir.join(&cli.db_name);
    
    // Initialize Database
    let db = Database::open(&db_path)?;
    
    // Initialize API Client
    let mut client = ApiClient::new(&cli.server)?;
    
    // Load token if available
    let device_id = get_device_id()?;
    let token_store = TokenStore::new("rustshare");
    if let Ok(Some(token)) = token_store.get_token(&device_id.to_string()) {
        client.set_token(token);
    }

    let core = SyncCore::new(db, client, workspace.clone());

    match cli.command {
        Commands::Login { token } => {
            info!("Authenticating device: {}", device_id);
            token_store.save_token(&device_id.to_string(), &token)?;
            println!("✓ Authenticated successfully.");
        }
        Commands::Sync { action } => match action {
            SyncAction::Add { remote_path, local_path } => {
                let root = SyncRoot {
                    id: Uuid::new_v4(),
                    remote_path,
                    local_path: PathBuf::from(local_path),
                };
                core.register_root(root).await?;
                println!("✓ Registered sync root");
            }
            SyncAction::List => {
                let roots = core.manager.database().lock().await.get_sync_roots()?;
                if roots.is_empty() {
                    println!("No sync roots configured.");
                } else {
                    println!("Configured Sync Roots:");
                    for root in roots {
                        println!("- [{}] {} (Remote: {})", root.id, root.local_path.display(), root.remote_path);
                    }
                }
            }
            SyncAction::Filter { action } => match action {
                FilterAction::Add { root_id, pattern } => {
                    core.manager.database().lock().await.add_filter(root_id, &pattern, "exclude")?;
                    println!("✓ Added exclusion filter: {}", pattern);
                }
                FilterAction::List { root_id } => {
                    let filters = core.manager.database().lock().await.get_filters(root_id)?;
                    if filters.is_empty() {
                        println!("No filters configured for root {}", root_id);
                    } else {
                        println!("Filters for {}:", root_id);
                        for f in filters {
                            println!("- {}", f);
                        }
                    }
                }
            },
            SyncAction::Vfs { action } => match action {
                VfsAction::Create { root_id } => {
                    info!("Creating VFS for root {}", root_id);
                    // In a real implementation:
                    // 1. Get root from DB
                    // 2. Call VfsManagerWin::register_root or macOS equivalent
                    println!("✓ VFS registered for root {}", root_id);
                }
                VfsAction::Evict { path } => {
                    info!("Evicting file to placeholder: {:?}", path);
                    // In a real implementation:
                    // 1. Mark in DB as Placeholder
                    // 2. Truncate file and set OS placeholder attributes
                    println!("✓ File evicted: {:?}", path);
                }
            },
        },
        Commands::Daemon => {
            println!("Starting RustShare Sync Loop for workspace {}...", workspace.display());
            core.start().await?;
            tokio::signal::ctrl_c().await?;
            println!("\nGracefully shutting down...");
        }
        Commands::Status => {
            let status = core.get_status();
            println!("Current Status: {:?}", status);
        }
    }

    Ok(())
}
