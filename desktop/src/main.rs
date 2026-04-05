//! RustShare Desktop Sync Client
//!
//! A selective sync client for RustShare that provides:
//! - Device pairing and authentication
//! - Local file synchronization
//! - Real-time and polling-based sync
//!
//! Usage:
//!   rustshare-desktop login          # Device pairing flow
//!   rustshare-desktop sync add <folder-id>  # Add folder to sync
//!   rustshare-desktop sync remove <folder-id> # Remove from sync
//!   rustshare-desktop daemon         # Run sync daemon
//!   rustshare-desktop status         # Show sync status

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use rustshare_desktop::api::auth::{interactive_pairing, DeviceAuth};
use rustshare_desktop::config::{Config, SyncDirection};
use rustshare_desktop::db::Database;
use rustshare_desktop::sync::engine::SyncEngine;
use rustshare_desktop::{ensure_directories, get_or_create_device_id};

/// RustShare Desktop Sync Client
#[derive(Parser)]
#[command(name = "rustshare-desktop")]
#[command(about = "Desktop sync client for RustShare")]
#[command(version)]
struct Cli {
    /// Configuration file path
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Server URL override
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
    /// Authenticate with device pairing flow
    Login {
        /// Non-interactive mode (use existing token if available)
        #[arg(long)]
        non_interactive: bool,
    },

    /// Logout and clear credentials
    Logout,

    /// Manage sync folders
    #[command(subcommand)]
    Sync(SyncCommands),

    /// Run the sync daemon
    Daemon {
        /// Run in foreground (don't daemonize)
        #[arg(short, long)]
        foreground: bool,
    },

    /// Show sync status
    Status {
        /// Watch for changes (continuous updates)
        #[arg(short, long)]
        watch: bool,
    },

    /// Pause syncing
    Pause,

    /// Resume syncing
    Resume,

    /// Force a full sync
    ForceSync,

    /// Show configuration
    Config {
        /// Edit configuration
        #[arg(short, long)]
        edit: bool,
    },

    /// Show version information
    Version,
}

#[derive(Subcommand)]
enum SyncCommands {
    /// Add a folder to sync
    Add {
        /// Folder ID on the server
        folder_id: uuid::Uuid,
        /// Local path to sync to
        #[arg(short, long)]
        path: Option<PathBuf>,
        /// Sync direction
        #[arg(short, long, value_enum, default_value = "bidirectional")]
        direction: DirectionArg,
    },

    /// Remove a folder from sync
    Remove {
        /// Folder ID on the server
        folder_id: uuid::Uuid,
        /// Also delete local files
        #[arg(short, long)]
        delete: bool,
    },

    /// List synced folders
    List,

    /// Pause sync for a folder
    Pause {
        folder_id: uuid::Uuid,
    },

    /// Resume sync for a folder
    Resume {
        folder_id: uuid::Uuid,
    },
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum DirectionArg {
    Bidirectional,
    Upload,
    Download,
}

impl From<DirectionArg> for SyncDirection {
    fn from(arg: DirectionArg) -> Self {
        match arg {
            DirectionArg::Bidirectional => SyncDirection::Bidirectional,
            DirectionArg::Upload => SyncDirection::UploadOnly,
            DirectionArg::Download => SyncDirection::DownloadOnly,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let log_level = if cli.verbose {
        Level::DEBUG
    } else {
        Level::INFO
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .with_target(false)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    // Load configuration
    let mut config = if let Some(config_path) = cli.config {
        Config::load_from(&config_path)?
    } else {
        Config::load()?
    };

    // Apply server URL override
    if let Some(server_url) = cli.server {
        config.server_url = server_url;
        config.save()?;
    }

    // Ensure directories exist
    ensure_directories()?;

    // Get or create device ID
    let _device_id = get_or_create_device_id()?;

    // Execute command
    match cli.command {
        Commands::Login { non_interactive } => {
            cmd_login(&config, non_interactive).await?;
        }
        Commands::Logout => {
            cmd_logout(&config).await?;
        }
        Commands::Sync(subcmd) => {
            cmd_sync(&config, subcmd).await?;
        }
        Commands::Daemon { foreground } => {
            cmd_daemon(&config, foreground).await?;
        }
        Commands::Status { watch } => {
            cmd_status(&config, watch).await?;
        }
        Commands::Pause => {
            cmd_pause().await?;
        }
        Commands::Resume => {
            cmd_resume().await?;
        }
        Commands::ForceSync => {
            cmd_force_sync(&config).await?;
        }
        Commands::Config { edit } => {
            cmd_config(&config, edit)?;
        }
        Commands::Version => {
            cmd_version();
        }
    }

    Ok(())
}

/// Login command
async fn cmd_login(config: &Config, non_interactive: bool) -> Result<()> {
    let auth = DeviceAuth::new(config.clone());

    // Check if already logged in
    if let Some(token) = auth.load_token()? {
        info!("Already authenticated (token created: {})", token.created_at);
        
        if non_interactive {
            println!("Already authenticated.");
            return Ok(());
        }

        println!("Already authenticated. Re-authenticate? [y/N]");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Keeping existing authentication.");
            return Ok(());
        }
    }

    // Start interactive pairing
    let token = interactive_pairing(config).await?;
    info!("Successfully authenticated with device ID: {}", token.device_id);

    Ok(())
}

/// Logout command
async fn cmd_logout(config: &Config) -> Result<()> {
    let auth = DeviceAuth::new(config.clone());
    
    auth.clear_token()?;
    rustshare_desktop::clear_local_data()?;
    
    info!("Logged out and cleared all local data");
    println!("✓ Logged out and cleared all local data.");
    
    Ok(())
}

/// Sync subcommand
async fn cmd_sync(config: &Config, subcmd: SyncCommands) -> Result<()> {
    match subcmd {
        SyncCommands::Add { folder_id, path, direction } => {
            cmd_sync_add(config, folder_id, path, direction.into()).await?;
        }
        SyncCommands::Remove { folder_id, delete } => {
            cmd_sync_remove(config, folder_id, delete).await?;
        }
        SyncCommands::List => {
            cmd_sync_list(config).await?;
        }
        SyncCommands::Pause { folder_id } => {
            cmd_sync_pause(folder_id).await?;
        }
        SyncCommands::Resume { folder_id } => {
            cmd_sync_resume(folder_id).await?;
        }
    }
    
    Ok(())
}

/// Add a folder to sync
async fn cmd_sync_add(
    config: &Config,
    folder_id: uuid::Uuid,
    path: Option<PathBuf>,
    direction: SyncDirection,
) -> Result<()> {
    // Check authentication
    let auth = DeviceAuth::new(config.clone());
    if auth.load_token()?.is_none() {
        anyhow::bail!("Not authenticated. Run 'rustshare-desktop login' first.");
    }

    // Determine local path
    let local_path = match path {
        Some(p) => p,
        None => {
            // Default to ~/RustShare/{folder_id}
            let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
            home.join("RustShare").join(folder_id.to_string())
        }
    };

    // Create local directory if it doesn't exist
    if !local_path.exists() {
        std::fs::create_dir_all(&local_path)?;
        info!("Created local directory: {}", local_path.display());
    }

    // Add to sync
    let engine_config = rustshare_desktop::sync::engine::SyncEngineConfig::from(config);
    let api_client = rustshare_desktop::api::ApiClient::with_token(
        config,
        auth.load_token()?.unwrap().token,
    )?;
    let db = Database::open()?;
    
    let engine = SyncEngine::new(engine_config, api_client, db).await?;
    engine.add_folder(folder_id, local_path.clone(), direction).await?;
    
    println!("✓ Added folder {} to sync at {}", folder_id, local_path.display());
    println!("  Direction: {:?}", direction);
    
    Ok(())
}

/// Remove a folder from sync
async fn cmd_sync_remove(config: &Config, folder_id: uuid::Uuid, delete: bool) -> Result<()> {
    let auth = DeviceAuth::new(config.clone());
    let token = auth.load_token()?;
    
    if let Some(token) = token {
        let api_client = rustshare_desktop::api::ApiClient::with_token(config, token.token)?;
        let db = Database::open()?;
        let engine_config = rustshare_desktop::sync::engine::SyncEngineConfig::from(config);
        
        let engine = SyncEngine::new(engine_config, api_client, db).await?;
        
        if delete {
            // Get local path before removing
            if let Some(local_path) = config.get_folder_local_path(folder_id) {
                println!("Deleting local files at {}...", local_path.display());
                tokio::fs::remove_dir_all(local_path).await.ok();
            }
        }
        
        engine.remove_folder(folder_id).await?;
    } else {
        // Just update config
        let mut config = Config::load()?;
        config.remove_sync_folder(folder_id)?;
    }
    
    println!("✓ Removed folder {} from sync", folder_id);
    Ok(())
}

/// List synced folders
async fn cmd_sync_list(_config: &Config) -> Result<()> {
    let config = Config::load()?;
    
    if config.sync_folders.is_empty() {
        println!("No folders configured for sync.");
        println!("Use 'rustshare-desktop sync add <folder-id>' to add a folder.");
        return Ok(());
    }
    
    println!("Synced folders:");
    println!();
    
    for folder in &config.sync_folders {
        let status = if folder.enabled { "✓" } else { "✗" };
        println!("{} {} -> {}", status, folder.folder_id, folder.local_path.display());
        println!("    Direction: {:?}", folder.direction);
    }
    
    Ok(())
}

/// Pause sync for a folder
async fn cmd_sync_pause(_folder_id: uuid::Uuid) -> Result<()> {
    println!("Pause not yet implemented");
    Ok(())
}

/// Resume sync for a folder
async fn cmd_sync_resume(_folder_id: uuid::Uuid) -> Result<()> {
    println!("Resume not yet implemented");
    Ok(())
}

/// Run the sync daemon
async fn cmd_daemon(config: &Config, foreground: bool) -> Result<()> {
    // Check authentication
    let auth = DeviceAuth::new(config.clone());
    let token = match auth.load_token()? {
        Some(t) => t,
        None => {
            anyhow::bail!("Not authenticated. Run 'rustshare-desktop login' first.");
        }
    };

    info!("Starting RustShare Desktop Sync Daemon");
    info!("Server: {}", config.server_url);
    info!("Device ID: {}", token.device_id);

    if !foreground {
        // TODO: Implement proper daemonization
        info!("Running in background mode (simulated)");
    }

    // Create and start sync engine
    let engine_config = rustshare_desktop::sync::engine::SyncEngineConfig::from(config);
    let api_client = rustshare_desktop::api::ApiClient::with_token(config, token.token)?;
    let db = Database::open()?;

    let mut engine = SyncEngine::new(engine_config, api_client, db).await?;
    engine.start().await?;

    println!("✓ Sync daemon started");
    println!("  Press Ctrl+C to stop");

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;
    
    info!("Shutting down...");
    engine.stop().await?;
    info!("Sync daemon stopped");

    Ok(())
}

/// Show sync status
async fn cmd_status(config: &Config, watch: bool) -> Result<()> {
    let auth = DeviceAuth::new(config.clone());
    
    // Authentication status
    match auth.load_token()? {
        Some(token) => {
            println!("Authentication: ✓ Authenticated");
            println!("  Device ID: {}", token.device_id);
            println!("  Token created: {}", token.created_at);
        }
        None => {
            println!("Authentication: ✗ Not authenticated");
        }
    }
    
    println!();
    
    // Server configuration
    println!("Server: {}", config.server_url);
    
    println!();
    
    // Sync folders
    if config.sync_folders.is_empty() {
        println!("Sync folders: None configured");
    } else {
        println!("Sync folders:");
        for folder in &config.sync_folders {
            let status = if folder.enabled { "enabled" } else { "disabled" };
            println!("  {} -> {} ({}, {:?})", 
                folder.folder_id, 
                folder.local_path.display(),
                status,
                folder.direction
            );
        }
    }
    
    println!();
    
    // Sync status
    if let Some(token) = auth.load_token()? {
        let api_client = rustshare_desktop::api::ApiClient::with_token(config, token.token)?;
        let db = Database::open()?;
        let engine_config = rustshare_desktop::sync::engine::SyncEngineConfig::from(config);
        
        let engine = SyncEngine::new(engine_config, api_client, db).await?;
        let status = engine.get_status().await;
        
        println!("Sync status: {}", if status.is_running { "running" } else { "stopped" });
        println!("Pending items: {}", status.total_pending);
    }
    
    if watch {
        println!();
        println!("Watching for changes... (Press Ctrl+C to exit)");
        // TODO: Implement watch mode with periodic updates
        tokio::signal::ctrl_c().await?;
    }
    
    Ok(())
}

/// Pause syncing
async fn cmd_pause() -> Result<()> {
    println!("Pause not yet implemented");
    Ok(())
}

/// Resume syncing
async fn cmd_resume() -> Result<()> {
    println!("Resume not yet implemented");
    Ok(())
}

/// Force a full sync
async fn cmd_force_sync(config: &Config) -> Result<()> {
    let auth = DeviceAuth::new(config.clone());
    let token = match auth.load_token()? {
        Some(t) => t,
        None => {
            anyhow::bail!("Not authenticated. Run 'rustshare-desktop login' first.");
        }
    };

    info!("Forcing full sync...");

    let engine_config = rustshare_desktop::sync::engine::SyncEngineConfig::from(config);
    let api_client = rustshare_desktop::api::ApiClient::with_token(config, token.token)?;
    let db = Database::open()?;

    let engine = SyncEngine::new(engine_config, api_client, db).await?;
    engine.force_sync().await?;

    println!("✓ Full sync completed");
    Ok(())
}

/// Show/edit configuration
fn cmd_config(_config: &Config, edit: bool) -> Result<()> {
    let config_path = Config::config_path()?;
    
    if edit {
        // Open in default editor
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
        std::process::Command::new(editor)
            .arg(&config_path)
            .status()?;
    } else {
        println!("Configuration file: {}", config_path.display());
        println!();
        let content = std::fs::read_to_string(&config_path)?;
        println!("{}", content);
    }
    
    Ok(())
}

/// Show version information
fn cmd_version() {
    println!("RustShare Desktop {}", rustshare_desktop::VERSION);
    println!("Device ID: {}", get_or_create_device_id().unwrap_or(uuid::Uuid::nil()));
}
