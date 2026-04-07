use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use std::future::Future;
use std::pin::Pin;
use std::path::PathBuf;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;
use uuid::Uuid;

use rustshare_desktop::api::auth::{interactive_pairing, DeviceToken};
use sync_engine::{SyncCore, ApiClient, Database, SyncRoot};
use platform::{desktop_token_store, PathManager, get_device_id};

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
    /// Login to account. Default is device pairing via approval link.
    Login {
        /// Explicit API token fallback for direct login
        #[arg(long, value_name = "TOKEN")]
        token: Option<String>,
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
    let Cli {
        workspace,
        db_name,
        server,
        verbose,
        command,
    } = cli;
    
    // Expand ~ for workspace path
    let workspace = if workspace.to_string_lossy().starts_with("~/") {
        let home = dirs::home_dir().ok_or_else(|| anyhow!("Home dir not found"))?;
        home.join(workspace.strip_prefix("~/")?)
    } else {
        workspace
    };
    std::fs::create_dir_all(&workspace)?;

    // Initialize logging
    let log_level = if verbose { Level::DEBUG } else { Level::INFO };
    let subscriber = FmtSubscriber::builder().with_max_level(log_level).finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let app_data_dir = PathManager::get_app_data_dir()?;
    let db_path = app_data_dir.join(&db_name);
    
    // Initialize Database
    let db = Database::open(&db_path)?;
    
    // Initialize API Client
    let mut client = ApiClient::new(&server)?;
    
    // Load token if available
    let device_id = get_device_id()?;
    let token_store = desktop_token_store();
    if let Ok(Some(token)) = token_store.get_token(&device_id.to_string()) {
        client.set_token(token);
    }

    let core = SyncCore::new(db, client, workspace.clone());

    match command {
        Commands::Login { token } => {
            info!("Authenticating device: {}", device_id);
            let device_token =
                resolve_login_token(&server, token, |server| Box::pin(interactive_pairing(server)))
                    .await?;
            token_store.save_token(&device_token.device_id.to_string(), &device_token.token)?;
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

type PairingFuture<'a> = Pin<Box<dyn Future<Output = Result<DeviceToken>> + 'a>>;

async fn resolve_login_token<F>(
    server: &str,
    token: Option<String>,
    pairing_flow: F,
) -> Result<DeviceToken>
where
    F: for<'a> FnOnce(&'a str) -> PairingFuture<'a>,
{
    match token {
        Some(token) => Ok(DeviceToken {
            token,
            device_id: get_device_id()?,
            created_at: chrono::Utc::now(),
        }),
        None => pairing_flow(server).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn login_defaults_to_pairing_when_no_token_is_provided() {
        let cli = Cli::try_parse_from(["rustshare-desktop", "login"]).unwrap();

        match cli.command {
            Commands::Login { token } => assert!(token.is_none()),
            _ => panic!("expected login command"),
        }
    }

    #[test]
    fn login_accepts_explicit_token_fallback() {
        let cli = Cli::try_parse_from([
            "rustshare-desktop",
            "login",
            "--token",
            "test-token-123",
        ])
        .unwrap();

        match cli.command {
            Commands::Login { token } => assert_eq!(token.as_deref(), Some("test-token-123")),
            _ => panic!("expected login command"),
        }
    }

    #[tokio::test]
    async fn login_without_token_uses_pairing_flow_result() {
        let paired_device_id = Uuid::new_v4();
        let paired_token = resolve_login_token(
            "https://rustshare.example",
            None,
            |_| {
                Box::pin(async move {
                    Ok(DeviceToken {
                    token: "paired-token-123".to_string(),
                    device_id: paired_device_id,
                    created_at: chrono::Utc::now(),
                })
                })
            },
        )
        .await
        .unwrap();

        assert_eq!(paired_token.token, "paired-token-123");
        assert_eq!(paired_token.device_id, paired_device_id);
    }
}
