use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use tracing::{debug, info, warn, Level};
use tracing_subscriber::FmtSubscriber;
use uuid::Uuid;

use platform::{desktop_token_store, get_device_id, PathManager};
use rustshare_desktop::api::auth::{interactive_pairing, DeviceToken};
use rustshare_desktop::config::{Config, FolderUpdate, SyncDirection};
use sync_domain::SyncRoot;
use sync_engine::{
    stop_daemon, wait_for_stop, ApiClient, DaemonHandle, Database, SocketClient, SyncManager,
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

    /// Server URL (defaults to the value in config.toml, or http://localhost:8080)
    #[arg(short, long)]
    server: Option<String>,

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
    /// Diagnose sync root health and known broken remote entries
    Doctor {
        /// Optional root ID to inspect
        root_id: Option<Uuid>,
        /// Maximum number of broken entries to print per root
        #[arg(long, default_value_t = 10)]
        limit: usize,
        /// Clear quarantined broken-remote entries before printing the report
        #[arg(long)]
        clear_quarantine: bool,
    },
    /// Re-check quarantined remote entries and optionally delete confirmed stale metadata
    CleanupRemote {
        /// Optional root ID to inspect
        root_id: Option<Uuid>,
        /// Maximum number of broken entries to process per root
        #[arg(long, default_value_t = 25)]
        limit: usize,
        /// Apply remote deletions after confirming the file is still missing
        #[arg(long)]
        apply: bool,
    },
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
}

#[derive(Subcommand)]
enum DaemonCommands {
    /// Start the sync daemon in the background
    Start,
    /// Run the daemon (internal use)
    #[command(hide = true)]
    Run,
    /// Stop the running sync daemon
    Stop,
    /// Check if the daemon is running
    Status,
    /// Show daemon logs
    Logs {
        /// Number of log lines to show
        #[arg(long, default_value_t = 200)]
        tail: usize,
        /// Continue streaming appended log lines
        #[arg(long)]
        follow: bool,
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

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Handle daemon start specially - must be done before tokio runtime
    // because daemonize forks and forking from async context causes issues
    if let Commands::Daemon {
        command: DaemonCommands::Start,
    } = &cli.command
    {
        return run_daemon_start(&cli);
    }

    // Run the async main for all other commands
    tokio::runtime::Runtime::new()?.block_on(async_main(cli))
}

fn run_daemon_start(cli: &Cli) -> Result<()> {
    #[cfg(unix)]
    use std::os::unix::process::CommandExt;
    use std::process::Command;

    let app_data_dir = PathManager::get_app_data_dir()?;
    let daemon_handle = DaemonHandle::new(app_data_dir.clone());

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

    println!("Starting RustShare daemon...");

    // Spawn daemon as detached process instead of using daemonize
    // This avoids the tokio runtime fork issues
    let log_path = app_data_dir.join("daemon.log");
    std::fs::create_dir_all(&app_data_dir)?;

    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(daemon_run_args(cli))
        .stdin(std::process::Stdio::null())
        .stdout(std::fs::File::create(&log_path)?)
        .stderr(std::fs::File::create(&log_path)?);

    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            nix::unistd::setsid()
                .map(|_| ())
                .map_err(std::io::Error::other)
        });
    }

    let child = command.spawn()?;

    // Write PID file
    std::fs::write(daemon_handle.pid_file(), child.id().to_string())?;

    println!("Daemon started successfully (PID: {})", child.id());
    Ok(())
}

fn daemon_run_args(cli: &Cli) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("--workspace"),
        cli.workspace.as_os_str().to_owned(),
        OsString::from("--db-name"),
        OsString::from(&cli.db_name),
    ];

    if let Some(server) = &cli.server {
        args.push(OsString::from("--server"));
        args.push(OsString::from(server));
    }

    if cli.verbose {
        args.push(OsString::from("--verbose"));
    }

    args.push(OsString::from("daemon"));
    args.push(OsString::from("run"));
    args
}

async fn async_main(cli: Cli) -> Result<()> {
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

    let config = Config::load()?;
    let server = server.as_deref().unwrap_or(&config.server_url);
    let server = normalize_server_url(server);

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
    let token = match token_store.get_token(&device_id.to_string()) {
        Ok(Some(token)) => {
            info!("Loaded auth token from keychain for device {}", device_id);
            Some(token)
        }
        Ok(None) => {
            warn!("No auth token found in keychain for device {}.", device_id);
            None
        }
        Err(e) => {
            warn!("Failed to load auth token from keychain: {}", e);
            None
        }
    };

    // Fallback: try to load from token file (for daemon process)
    let token = token.or_else(|| {
        let token_path = app_data_dir.join("token.txt");
        match std::fs::read_to_string(&token_path) {
            Ok(token) => {
                let token = token.trim().to_string();
                info!("Loaded auth token from file for device {}", device_id);
                Some(token)
            }
            Err(e) => {
                debug!("Failed to load token from file: {}", e);
                None
            }
        }
    });

    if let Some(token) = token {
        client.set_token(token);
    } else {
        warn!(
            "No auth token available for device {}. Login required.",
            device_id
        );
    }

    let socket_path = app_data_dir.join("daemon.sock");
    let core = SyncManager::new(db, client.clone(), workspace.clone());

    match command {
        Commands::Login { token } => {
            info!("Authenticating device: {}", device_id);
            let device_token = resolve_login_token(&server, token, |server| {
                Box::pin(interactive_pairing(server))
            })
            .await?;
            if let Err(e) =
                token_store.save_token(&device_token.device_id.to_string(), &device_token.token)
            {
                warn!("Failed to save auth token to keychain: {}", e);
            } else {
                info!(
                    "Saved auth token to keychain for device {}",
                    device_token.device_id
                );
            }
            persist_daemon_token(&app_data_dir, &device_token.token)?;
            println!("✓ Authenticated successfully.");
        }
        Commands::Sync { action } => match action {
            SyncAction::Add {
                remote_path,
                local_path,
            } => {
                let root_id = Uuid::new_v4();
                let resolved_local_path = resolve_sync_local_path(&workspace, &local_path);
                let root = SyncRoot {
                    id: root_id,
                    remote_path: remote_path.clone(),
                    local_path: resolved_local_path.clone(),
                };

                // Register in database
                core.sync_root(root).await?;

                // Also add to config.toml for persistence
                let mut config = Config::load()?;
                config.add_sync_folder(root_id, resolved_local_path);
                config.save()?;

                println!("✓ Registered sync root {}", root_id);
            }
            SyncAction::List => {
                let roots = core.database().lock().await.get_sync_roots()?;
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
            SyncAction::Doctor {
                root_id,
                limit,
                clear_quarantine,
            } => {
                let daemon_handle = DaemonHandle::new(app_data_dir.clone());
                println!(
                    "Daemon: {}",
                    if daemon_handle.is_running() {
                        "running"
                    } else {
                        "stopped"
                    }
                );

                let config = Config::load()?;
                let db_arc = core.database();
                let db = db_arc.lock().await;
                if clear_quarantine {
                    let cleared = match root_id {
                        Some(root_id) => db.clear_broken_remote_entries(root_id)?,
                        None => db.clear_all_broken_remote_entries()?,
                    };
                    println!(
                        "Cleared {} quarantined broken remote entr{}",
                        cleared,
                        if cleared == 1 { "y" } else { "ies" }
                    );
                }

                let mut roots = db.get_sync_roots()?;
                if let Some(root_id) = root_id {
                    roots.retain(|root| root.id == root_id);
                }

                if roots.is_empty() {
                    println!("No matching sync roots found.");
                } else {
                    for root in roots {
                        print_root_diagnosis(&db, &config, &root, limit)?;
                    }
                }
            }
            SyncAction::CleanupRemote {
                root_id,
                limit,
                apply,
            } => {
                let cleanup_targets = {
                    let db_arc = core.database();
                    let db = db_arc.lock().await;
                    let mut roots = db.get_sync_roots()?;
                    if let Some(root_id) = root_id {
                        roots.retain(|root| root.id == root_id);
                    }

                    let mut targets = Vec::new();
                    for root in roots {
                        let entries = db.get_broken_remote_entries(root.id)?;
                        targets.push((root, entries));
                    }
                    targets
                };

                if cleanup_targets.is_empty() {
                    println!("No matching sync roots found.");
                } else {
                    let mut confirmed_missing = 0usize;
                    let mut deleted = 0usize;
                    let mut recovered = 0usize;
                    let mut delete_errors = 0usize;

                    for (root, entries) in cleanup_targets {
                        println!();
                        println!("Root {} ({})", root.id, root.remote_path);

                        if entries.is_empty() {
                            println!("  No quarantined broken remote entries.");
                            continue;
                        }

                        let total_entries = entries.len();
                        let mut seen_remote_ids = HashSet::new();
                        let mut processed = 0usize;

                        for entry in entries {
                            if processed >= limit {
                                break;
                            }
                            if !seen_remote_ids.insert(entry.remote_file_id) {
                                continue;
                            }
                            processed += 1;

                            match client.download_file(entry.remote_file_id).await {
                                Ok(_) => {
                                    recovered += 1;
                                    println!(
                                        "  Recovered: {} [{}]",
                                        entry.relative_path.display(),
                                        entry.remote_file_id
                                    );
                                    let db_arc = core.database();
                                    let db = db_arc.lock().await;
                                    let _ = db.clear_broken_remote_entries_for_path(
                                        root.id,
                                        &entry.relative_path,
                                    )?;
                                }
                                Err(error) => {
                                    let error_text = error.to_string();
                                    if !is_missing_remote_error(&error_text) {
                                        println!(
                                            "  Skipped: {} [{}] error={}",
                                            entry.relative_path.display(),
                                            entry.remote_file_id,
                                            error_text
                                        );
                                        continue;
                                    }

                                    confirmed_missing += 1;
                                    if apply {
                                        match client.delete_file(entry.remote_file_id).await {
                                            Ok(_) => {
                                                deleted += 1;
                                                println!(
                                                    "  Deleted stale remote entry: {} [{}]",
                                                    entry.relative_path.display(),
                                                    entry.remote_file_id
                                                );
                                                let db_arc = core.database();
                                                let db = db_arc.lock().await;
                                                let _ = db.clear_broken_remote_entries_for_path(
                                                    root.id,
                                                    &entry.relative_path,
                                                )?;
                                                db.mark_file_tombstone(
                                                    root.id,
                                                    &entry.relative_path,
                                                    "local",
                                                    chrono::Utc::now().timestamp(),
                                                )?;
                                            }
                                            Err(delete_error) => {
                                                delete_errors += 1;
                                                println!(
                                                    "  Delete failed: {} [{}] error={}",
                                                    entry.relative_path.display(),
                                                    entry.remote_file_id,
                                                    delete_error
                                                );
                                            }
                                        }
                                    } else {
                                        println!(
                                            "  Would delete stale remote entry: {} [{}]",
                                            entry.relative_path.display(),
                                            entry.remote_file_id
                                        );
                                    }
                                }
                            }
                        }

                        if processed == 0 {
                            println!("  No unique quarantined entries to process.");
                        } else if total_entries > processed {
                            println!(
                                "  ... {} additional entries not shown",
                                total_entries - processed
                            );
                        }
                    }

                    println!();
                    println!(
                        "Cleanup summary: confirmed_missing={} recovered={} deleted={} delete_errors={}",
                        confirmed_missing, recovered, deleted, delete_errors
                    );
                    if !apply {
                        println!(
                            "Dry run only. Re-run with --apply to delete confirmed stale metadata."
                        );
                    }
                }
            }
            SyncAction::Remove { root_id } => {
                // Remove from SQLite database
                let db_removed = {
                    let db_arc = core.database();
                    let db = db_arc.lock().await;
                    db.remove_sync_root(root_id)?
                };

                // Remove from config.toml
                let mut config = Config::load()?;
                let config_removed = config.remove_sync_folder(root_id);
                if config_removed {
                    config.save()?;
                }

                // Success if removed from either source
                if db_removed || config_removed {
                    println!("✓ Removed sync root {}", root_id);
                    // Notify daemon if running
                    notify_daemon_config_change(&app_data_dir, root_id).await?;
                } else {
                    println!("Sync root {} not found", root_id);
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
                let resolved_local_path = local_path
                    .as_deref()
                    .map(|path| resolve_sync_local_path(&workspace, path));

                // Build FolderUpdate from CLI args
                let updates = FolderUpdate {
                    local_path: resolved_local_path.clone(),
                    enabled: None,
                    direction: direction.map(|d| d.into()),
                    add_ignore_patterns: ignore_pattern,
                    remove_ignore_patterns: remove_ignore,
                    clear_ignores,
                };

                // Update config.toml
                let mut config = Config::load()?;
                let updated = config.update_sync_folder(root_id, updates);

                if updated {
                    config.save()?;
                }

                if let Some(local_path) = resolved_local_path {
                    let db_arc = core.database();
                    let db = db_arc.lock().await;
                    if let Some(mut root) = db
                        .get_sync_roots()?
                        .into_iter()
                        .find(|root| root.id == root_id)
                    {
                        root.local_path = local_path;
                        db.save_sync_root(&root)?;
                    }
                }

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
                let updated = config.set_folder_enabled(root_id, true);
                if updated {
                    config.save()?;
                }

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
                let updated = config.set_folder_enabled(root_id, false);
                if updated {
                    config.save()?;
                }

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
                    core.database()
                        .lock()
                        .await
                        .add_filter(root_id, &pattern, "exclude")?;
                    println!("✓ Added exclusion filter: {}", pattern);
                }
                FilterAction::List { root_id } => {
                    let filters = core.database().lock().await.get_filters(root_id)?;
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
        },
        Commands::Daemon { command } => {
            let app_data_dir = PathManager::get_app_data_dir()?;
            let daemon_handle = DaemonHandle::new(app_data_dir.clone());

            match command {
                DaemonCommands::Start => {
                    // Handled in main() before tokio runtime
                    unreachable!("Start command should be handled in main()");
                }
                DaemonCommands::Run => {
                    // Internal command: actually run the daemon
                    // This is spawned as a child process by the Start command
                    daemon_handle.write_pid()?;
                    run_daemon(core, socket_path, daemon_handle).await?;
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
                DaemonCommands::Logs { tail, follow } => {
                    let log_path = app_data_dir.join("daemon.log");
                    if log_path.exists() {
                        match read_log_tail(&log_path, tail) {
                            Ok(content) => {
                                if content.is_empty() {
                                    println!("Log file is empty");
                                } else {
                                    print!("{}", content);
                                }
                                if follow {
                                    follow_log_file(&log_path).await?;
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
            let status = core.current_state().await;
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

fn resolve_sync_local_path(workspace: &std::path::Path, local_path: &str) -> PathBuf {
    if let Some(stripped) = local_path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }

    let local_path = PathBuf::from(local_path);
    if local_path.is_absolute() {
        local_path
    } else {
        workspace.join(local_path)
    }
}

fn persist_daemon_token(app_data_dir: &std::path::Path, token: &str) -> Result<()> {
    let token_path = app_data_dir.join("token.txt");
    fs::create_dir_all(app_data_dir)?;
    fs::write(&token_path, token)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&token_path)?.permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(&token_path, permissions)?;
    }

    Ok(())
}

fn read_log_tail(log_path: &std::path::Path, tail: usize) -> Result<String> {
    use std::io::{BufRead, BufReader};

    let file = fs::File::open(log_path)?;
    let reader = BufReader::new(file);
    let lines = reader.lines().collect::<std::result::Result<Vec<_>, _>>()?;

    let start = lines.len().saturating_sub(tail);
    let mut output = lines[start..].join("\n");
    if !output.is_empty() {
        output.push('\n');
    }
    Ok(output)
}

async fn follow_log_file(log_path: &std::path::Path) -> Result<()> {
    use std::io::{Read, Seek, SeekFrom};
    use tokio::time::{sleep, Duration};

    let mut offset = fs::metadata(log_path).map(|meta| meta.len()).unwrap_or(0);
    println!("-- following {} (Ctrl+C to stop) --", log_path.display());

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("\nStopped following logs.");
                return Ok(());
            }
            _ = sleep(Duration::from_millis(500)) => {
                let metadata = match fs::metadata(log_path) {
                    Ok(meta) => meta,
                    Err(_) => continue,
                };

                if metadata.len() < offset {
                    offset = 0;
                }

                if metadata.len() == offset {
                    continue;
                }

                let mut file = fs::File::open(log_path)?;
                file.seek(SeekFrom::Start(offset))?;
                let mut buf = String::new();
                file.read_to_string(&mut buf)?;
                print!("{}", buf);
                offset = metadata.len();
            }
        }
    }
}

fn print_root_diagnosis(
    db: &Database,
    config: &Config,
    root: &SyncRoot,
    limit: usize,
) -> Result<()> {
    let enabled = config
        .get_sync_folder(root.id)
        .map(|folder| folder.enabled)
        .unwrap_or(true);
    let file_states = db.get_all_file_states(root.id)?;
    let broken_entries = db.get_broken_remote_entries(root.id)?;
    let local_exists = root.local_path.exists();

    println!();
    println!("Root {}", root.id);
    println!("  Local : {}", root.local_path.display());
    println!("  Remote: {}", root.remote_path);
    println!("  Enabled: {}", enabled);
    println!("  Local path exists: {}", local_exists);
    println!("  Indexed file states: {}", file_states.len());
    println!("  Broken remote entries: {}", broken_entries.len());

    if root.remote_path == "/" {
        println!("  Warning: this is a full account root mirror");
    }

    if !local_exists {
        println!("  Problem: local path is missing");
    }

    if file_states.is_empty() {
        println!("  Problem: no successful synced file state recorded yet");
    }

    if !broken_entries.is_empty() {
        println!("  Broken paths:");
        for entry in broken_entries.into_iter().take(limit) {
            println!(
                "  - {} [{}] last_seen={} error={}",
                entry.relative_path.display(),
                entry.remote_file_id,
                format_unix_timestamp(entry.last_seen_at),
                entry.error
            );
        }
    }

    Ok(())
}

fn format_unix_timestamp(timestamp: i64) -> String {
    chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| timestamp.to_string())
}

fn is_missing_remote_error(message: &str) -> bool {
    let lowered = message.to_ascii_lowercase();
    lowered.contains("404")
        || lowered.contains("410")
        || lowered.contains("not found")
        || lowered.contains("gone")
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
async fn run_daemon(
    mut core: SyncManager,
    socket_path: PathBuf,
    handle: DaemonHandle,
) -> anyhow::Result<()> {
    // Start the sync core (spawns background tasks)
    core.start_socket_server(socket_path).await?;
    core.start().await?;
    tracing::info!("Sync core started, daemon is running");

    // Wait for shutdown signal
    let shutdown = tokio::signal::ctrl_c();
    let _ = shutdown.await;
    tracing::info!("Received shutdown signal");

    // Cleanup
    handle.remove_pid()?;
    if handle.socket_path().exists() {
        std::fs::remove_file(handle.socket_path()).ok();
    }

    tracing::info!("Daemon shutdown complete");
    Ok(())
}

async fn notify_daemon_config_change(app_data_dir: &std::path::Path, root_id: Uuid) -> Result<()> {
    use sync_engine::SocketClient;

    let daemon_handle = DaemonHandle::new(app_data_dir.to_path_buf());

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
    use std::path::Path;

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
            normalize_server_url("https://localhost:8080"),
            "https://localhost:8080"
        );
    }

    #[test]
    fn normalize_server_url_defaults_to_https() {
        assert_eq!(
            normalize_server_url("localhost:8080"),
            "https://localhost:8080"
        );
    }

    #[test]
    fn daemon_start_forwards_workspace_server_db_and_verbose() {
        let cli = Cli::try_parse_from([
            "rustshare-desktop",
            "--workspace",
            "/tmp/rustshare-workspace",
            "--db-name",
            "custom.db",
            "--server",
            "localhost:8080",
            "--verbose",
            "daemon",
            "start",
        ])
        .unwrap();

        let args = daemon_run_args(&cli)
            .into_iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(
            args,
            vec![
                "--workspace",
                "/tmp/rustshare-workspace",
                "--db-name",
                "custom.db",
                "--server",
                "localhost:8080",
                "--verbose",
                "daemon",
                "run",
            ]
        );
    }

    #[test]
    fn sync_add_resolves_relative_paths_against_workspace() {
        let resolved = resolve_sync_local_path(Path::new("/tmp/workspace"), "mirror");
        assert_eq!(resolved, PathBuf::from("/tmp/workspace/mirror"));
    }

    #[test]
    fn sync_add_keeps_absolute_paths() {
        let resolved = resolve_sync_local_path(Path::new("/tmp/workspace"), "/srv/rustshare");
        assert_eq!(resolved, PathBuf::from("/srv/rustshare"));
    }

    #[test]
    fn persist_daemon_token_writes_token_file() {
        let tempdir = tempfile::tempdir().unwrap();
        persist_daemon_token(tempdir.path(), "test-token-123").unwrap();

        let written = std::fs::read_to_string(tempdir.path().join("token.txt")).unwrap();
        assert_eq!(written, "test-token-123");
    }

    #[test]
    fn daemon_logs_accepts_tail_and_follow_flags() {
        let cli = Cli::try_parse_from([
            "rustshare-desktop",
            "daemon",
            "logs",
            "--tail",
            "50",
            "--follow",
        ])
        .unwrap();

        match cli.command {
            Commands::Daemon {
                command: DaemonCommands::Logs { tail, follow },
            } => {
                assert_eq!(tail, 50);
                assert!(follow);
            }
            _ => panic!("expected daemon logs command"),
        }
    }

    #[test]
    fn sync_doctor_accepts_clear_quarantine_flag() {
        let root_id = Uuid::nil().to_string();
        let cli = Cli::try_parse_from([
            "rustshare-desktop",
            "sync",
            "doctor",
            &root_id,
            "--limit",
            "25",
            "--clear-quarantine",
        ])
        .unwrap();

        match cli.command {
            Commands::Sync {
                action:
                    SyncAction::Doctor {
                        root_id: parsed_root_id,
                        limit,
                        clear_quarantine,
                    },
            } => {
                assert_eq!(parsed_root_id, Some(Uuid::nil()));
                assert_eq!(limit, 25);
                assert!(clear_quarantine);
            }
            _ => panic!("expected sync doctor command"),
        }
    }

    #[test]
    fn sync_cleanup_remote_accepts_apply_flag() {
        let root_id = Uuid::nil().to_string();
        let cli = Cli::try_parse_from([
            "rustshare-desktop",
            "sync",
            "cleanup-remote",
            &root_id,
            "--limit",
            "12",
            "--apply",
        ])
        .unwrap();

        match cli.command {
            Commands::Sync {
                action:
                    SyncAction::CleanupRemote {
                        root_id: parsed_root_id,
                        limit,
                        apply,
                    },
            } => {
                assert_eq!(parsed_root_id, Some(Uuid::nil()));
                assert_eq!(limit, 12);
                assert!(apply);
            }
            _ => panic!("expected sync cleanup-remote command"),
        }
    }

    #[test]
    fn read_log_tail_returns_last_n_lines() {
        let tempdir = tempfile::tempdir().unwrap();
        let log_path = tempdir.path().join("daemon.log");
        std::fs::write(&log_path, "one\ntwo\nthree\nfour\n").unwrap();

        let output = read_log_tail(&log_path, 2).unwrap();
        assert_eq!(output, "three\nfour\n");
    }
}
