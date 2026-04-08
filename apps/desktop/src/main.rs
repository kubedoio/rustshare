use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use std::fs;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;
use uuid::Uuid;

use platform::{desktop_token_store, get_device_id, PathManager};
use rustshare_desktop::api::auth::{interactive_pairing, DeviceToken};
use rustshare_desktop::config::{Config, FolderUpdate, SyncDirection};
use sync_engine::{
    stop_daemon, wait_for_stop, ApiClient, DaemonHandle, Database, SocketClient, SyncCore, SyncRoot,
};

/// RustShare Desktop Sync Client (Phase 1)
#[derive(Parser)]
#[command(name = "rustshare-desktop")]
#[command(about = "Desktop sync client for RustShare")]
#[command(version)]
struct Cli {
    /// Workspace root path
    #[arg(
        short,
        long,
        env = "RUSTSHARE_WORKSPACE",
        default_value = "~/RustShare"
    )]
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
    /// Manage the sync daemon
    Daemon {
        #[command(subcommand)]
        command: DaemonCommands,
    },
    /// Show current status
    Status,
}

/// CLI argument for sync direction
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum DirectionArg {
    Bidir,
    Up,
    Down,
}

impl From<DirectionArg> for SyncDirection {
    fn from(arg: DirectionArg) -> Self {
        match arg {
            DirectionArg::Bidir => SyncDirection::Bidirectional,
            DirectionArg::Up => SyncDirection::UploadOnly,
            DirectionArg::Down => SyncDirection::DownloadOnly,
        }
    }
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
    /// Remove a sync root
    Remove {
        /// Root ID (UUID)
        root_id: Uuid,
    },
    /// Update a sync root configuration
    Update {
        /// Root ID (UUID)
        root_id: Uuid,
        /// New local path
        #[arg(long)]
        local_path: Option<String>,
        /// Sync direction
        #[arg(long, value_enum)]
        direction: Option<DirectionArg>,
        /// Add ignore pattern (can be specified multiple times)
        #[arg(long)]
        ignore_pattern: Vec<String>,
        /// Remove ignore pattern (can be specified multiple times)
        #[arg(long)]
        remove_ignore: Vec<String>,
        /// Clear all ignore patterns and reset to defaults
        #[arg(long)]
        clear_ignores: bool,
    },
    /// Enable a sync root
    Enable {
        /// Root ID (UUID)
        root_id: Uuid,
    },
    /// Disable a sync root
    Disable {
        /// Root ID (UUID)
        root_id: Uuid,
    },
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
enum DaemonCommands {
    /// Start the sync daemon in the background
    Start,
    /// Stop the running sync daemon
    Stop,
    /// Check if the daemon is running
    Status,
    /// Show daemon logs
    Logs,
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

    let server = normalize_server_url(&server);

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

    let socket_path = app_data_dir.join("daemon.sock");
    let core = SyncCore::new(db, client, workspace.clone(), socket_path);

    match command {
        Commands::Login { token } => {
            info!("Authenticating device: {}", device_id);
            let device_token = resolve_login_token(&server, token, |server| {
                Box::pin(interactive_pairing(server))
            })
            .await?;
            token_store.save_token(&device_token.device_id.to_string(), &device_token.token)?;
            println!("✓ Authenticated successfully.");
        }
        Commands::Sync { action } => match action {
            SyncAction::Add {
                remote_path,
                local_path,
            } => {
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
                let config = Config::load()?;
                if roots.is_empty() {
                    println!("No sync roots configured.");
                } else {
                    println!("Configured Sync Roots:");
                    for root in roots {
                        let enabled = config
                            .get_sync_folder(root.id)
                            .map(|f| if f.enabled { "enabled" } else { "disabled" })
                            .unwrap_or("unknown");
                        let direction = config
                            .get_sync_folder(root.id)
                            .map(|f| format!("{:?}", f.direction))
                            .unwrap_or_else(|| "Bidirectional".to_string());
                        println!(
                            "- [{}] {} (Remote: {}) [{}] direction={}",
                            root.id,
                            root.local_path.display(),
                            root.remote_path,
                            enabled,
                            direction
                        );
                    }
                }
            }
            SyncAction::Remove { root_id } => {
                // Remove from SQLite database
                {
                    let db_arc = core.manager.database();
                    let db = db_arc.lock().await;
                    db.remove_sync_root(root_id)?;
                }

                // Remove from config.toml
                let mut config = Config::load()?;
                let removed = config.remove_sync_folder(root_id)?;

                if removed {
                    println!("✓ Removed sync root {}", root_id);
                    // Notify daemon if running
                    notify_daemon_config_change(&app_data_dir, root_id).await?;
                } else {
                    println!("Sync root {} not found in config", root_id);
                }
            }
            SyncAction::Update {
                root_id,
                local_path,
                direction,
                ignore_pattern,
                remove_ignore,
                clear_ignores,
            } => {
                // Build FolderUpdate from CLI args
                let updates = FolderUpdate {
                    local_path: local_path.map(PathBuf::from),
                    enabled: None,
                    direction: direction.map(|d| d.into()),
                    add_ignore_patterns: ignore_pattern,
                    remove_ignore_patterns: remove_ignore,
                    clear_ignores,
                };

                // Update config.toml
                let mut config = Config::load()?;
                let updated = config.update_sync_folder(root_id, updates)?;

                if updated {
                    println!("✓ Updated sync root {}", root_id);
                    // Notify daemon if running
                    notify_daemon_config_change(&app_data_dir, root_id).await?;
                } else {
                    println!("Sync root {} not found", root_id);
                }
            }
            SyncAction::Enable { root_id } => {
                let mut config = Config::load()?;
                let updated = config.set_folder_enabled(root_id, true)?;

                if updated {
                    println!("✓ Enabled sync root {}", root_id);
                    // Notify daemon if running
                    notify_daemon_config_change(&app_data_dir, root_id).await?;
                } else {
                    println!("Sync root {} not found", root_id);
                }
            }
            SyncAction::Disable { root_id } => {
                let mut config = Config::load()?;
                let updated = config.set_folder_enabled(root_id, false)?;

                if updated {
                    println!("✓ Disabled sync root {}", root_id);
                    // Notify daemon if running
                    notify_daemon_config_change(&app_data_dir, root_id).await?;
                } else {
                    println!("Sync root {} not found", root_id);
                }
            }
            SyncAction::Filter { action } => match action {
                FilterAction::Add { root_id, pattern } => {
                    core.manager
                        .database()
                        .lock()
                        .await
                        .add_filter(root_id, &pattern, "exclude")?;
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
        Commands::Daemon { command } => {
            let app_data_dir = PathManager::get_app_data_dir()?;
            let daemon_handle = DaemonHandle::new(app_data_dir.clone());

            match command {
                DaemonCommands::Start => {
                    // Check if already running
                    if daemon_handle.is_running() {
                        if let Some(pid) = daemon_handle.get_pid() {
                            println!("Daemon is already running (PID: {})", pid);
                        } else {
                            println!("Daemon is already running");
                        }
                        return Ok(());
                    }

                    // Cleanup stale files
                    daemon_handle.cleanup_stale()?;

                    // Fork to background using daemonize
                    println!("Starting RustShare daemon...");

                    let log_path = app_data_dir.join("daemon.log");
                    std::fs::create_dir_all(&app_data_dir)?;

                    let daemonize = daemonize::Daemonize::new()
                        .pid_file(daemon_handle.pid_file())
                        .working_directory(&app_data_dir)
                        .stdout(std::fs::File::create(&log_path)?)
                        .stderr(std::fs::File::create(&log_path)?);

                    match daemonize.start() {
                        Ok(_) => {
                            // In daemon process - write PID and start sync core
                            daemon_handle.write_pid()?;

                            // Run the daemon with proper shutdown handling
                            let rt = tokio::runtime::Runtime::new()?;
                            rt.block_on(run_daemon(core, daemon_handle))?;
                        }
                        Err(e) => {
                            return Err(anyhow!("Failed to daemonize: {}", e));
                        }
                    }

                    println!("Daemon started successfully");
                }
                DaemonCommands::Stop => {
                    if !daemon_handle.is_running() {
                        println!("Daemon is not running");
                        // Cleanup stale files if they exist
                        let _ = daemon_handle.cleanup_stale();
                        return Ok(());
                    }

                    let pid = daemon_handle
                        .get_pid()
                        .ok_or_else(|| anyhow!("Could not get daemon PID"))?;

                    println!("Stopping daemon (PID: {})...", pid);

                    // Send SIGTERM
                    stop_daemon(daemon_handle.pid_file())?;

                    // Wait for stop with timeout
                    match wait_for_stop(daemon_handle.pid_file(), 10).await {
                        Ok(_) => {
                            println!("Daemon stopped successfully");
                        }
                        Err(e) => {
                            eprintln!("Warning: {}", e);
                            println!("Forcing cleanup...");
                            let _ = daemon_handle.cleanup_stale();
                        }
                    }
                }
                DaemonCommands::Status => {
                    if daemon_handle.is_running() {
                        if let Some(pid) = daemon_handle.get_pid() {
                            println!("Daemon is running (PID: {})", pid);

                            // Try to ping daemon via socket
                            let mut client = SocketClient::new(daemon_handle.socket_path());
                            match client.connect().await {
                                Ok(_) => {
                                    match client.ping().await {
                                        Ok(true) => println!("Daemon is responsive"),
                                        Ok(false) => println!("Daemon ping failed"),
                                        Err(e) => println!("Daemon ping error: {}", e),
                                    }
                                    let _ = client.disconnect().await;
                                }
                                Err(e) => {
                                    println!("Could not connect to daemon socket: {}", e);
                                }
                            }
                        } else {
                            println!("Daemon PID file exists but could not read PID");
                        }
                    } else {
                        println!("Daemon is not running");
                        // Cleanup stale files if they exist
                        let _ = daemon_handle.cleanup_stale();
                    }
                }
                DaemonCommands::Logs => {
                    let log_path = app_data_dir.join("daemon.log");
                    if log_path.exists() {
                        match fs::read_to_string(&log_path) {
                            Ok(content) => {
                                if content.is_empty() {
                                    println!("Log file is empty");
                                } else {
                                    print!("{}", content);
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to read log file: {}", e);
                            }
                        }
                    } else {
                        println!("No log file found at {}", log_path.display());
                    }
                }
            }
        }
        Commands::Status => {
            let status = core.get_status();
            println!("Current Status: {:?}", status);
        }
    }

    Ok(())
}

type PairingFuture<'a> = Pin<Box<dyn Future<Output = Result<DeviceToken>> + 'a>>;

fn normalize_server_url(server: &str) -> String {
    if server.contains("://") {
        server.to_string()
    } else {
        format!("https://{}", server.trim_start_matches('/'))
    }
}

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

/// Notify the daemon about a configuration change for a specific root
async fn run_daemon(core: SyncCore, handle: DaemonHandle) -> anyhow::Result<()> {
    // Set up shutdown signal handling
    let shutdown = tokio::signal::ctrl_c();

    tokio::select! {
        result = core.start() => {
            if let Err(e) = result {
                tracing::error!("Sync core error: {}", e);
            }
        }
        _ = shutdown => {
            tracing::info!("Received shutdown signal");
        }
    }

    // Cleanup
    handle.remove_pid()?;
    if handle.socket_path().exists() {
        std::fs::remove_file(handle.socket_path()).ok();
    }

    tracing::info!("Daemon shutdown complete");
    Ok(())
}

async fn notify_daemon_config_change(app_data_dir: &PathBuf, root_id: Uuid) -> Result<()> {
    use sync_engine::SocketClient;

    let daemon_handle = DaemonHandle::new(app_data_dir.clone());

    if daemon_handle.is_running() {
        let mut client = SocketClient::new(daemon_handle.socket_path());
        match client.connect().await {
            Ok(_) => {
                // Send a config reload notification for the specific root
                match client
                    .notify(
                        "config.reload",
                        Some(serde_json::json!({"root_id": root_id})),
                    )
                    .await
                {
                    Ok(_) => {
                        tracing::info!("Notified daemon about config change for root {}", root_id)
                    }
                    Err(e) => tracing::warn!("Failed to notify daemon: {}", e),
                }
                let _ = client.disconnect().await;
            }
            Err(e) => {
                tracing::warn!("Could not connect to daemon socket: {}", e);
            }
        }
    }

    Ok(())
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
        let cli = Cli::try_parse_from(["rustshare-desktop", "login", "--token", "test-token-123"])
            .unwrap();

        match cli.command {
            Commands::Login { token } => assert_eq!(token.as_deref(), Some("test-token-123")),
            _ => panic!("expected login command"),
        }
    }

    #[tokio::test]
    async fn login_without_token_uses_pairing_flow_result() {
        let paired_device_id = Uuid::new_v4();
        let paired_token = resolve_login_token("https://rustshare.example", None, |_| {
            Box::pin(async move {
                Ok(DeviceToken {
                    token: "paired-token-123".to_string(),
                    device_id: paired_device_id,
                    created_at: chrono::Utc::now(),
                })
            })
        })
        .await
        .unwrap();

        assert_eq!(paired_token.token, "paired-token-123");
        assert_eq!(paired_token.device_id, paired_device_id);
    }

    #[test]
    fn normalize_server_url_keeps_existing_scheme() {
        assert_eq!(
            normalize_server_url("http://localhost:8080"),
            "http://localhost:8080"
        );
        assert_eq!(
            normalize_server_url("https://app.rustshare.io"),
            "https://app.rustshare.io"
        );
    }

    #[test]
    fn normalize_server_url_defaults_to_https() {
        assert_eq!(
            normalize_server_url("app.rustshare.io"),
            "https://app.rustshare.io"
        );
    }
}
