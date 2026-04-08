# Desktop CLI Improvements Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace TCP daemon with Unix socket, add background process management, and extend CLI with full sync CRUD operations.

**Architecture:** Unix socket at `~/.config/rustshare/daemon.sock` for CLI↔daemon RPC, PID file for lifecycle management, extended config.rs with update/remove methods, new daemon.rs for process management.

**Tech Stack:** Rust, Tokio, Unix sockets, daemonize crate, nix crate for signals, serde_json for RPC

---

## Task 1: Add Dependencies

**Files:**
- Modify: `Cargo.toml`
- Modify: `apps/desktop/Cargo.toml`

**Step 1: Add workspace dependencies**

Add to workspace `Cargo.toml` `[workspace.dependencies]`:
```toml
daemonize = "0.5"
nix = { version = "0.29", features = ["process", "signal"] }
```

**Step 2: Add to desktop app dependencies**

Add to `apps/desktop/Cargo.toml` `[dependencies]`:
```toml
daemonize = { workspace = true }
nix = { workspace = true }
```

**Step 3: Commit**

```bash
git add Cargo.toml apps/desktop/Cargo.toml
git commit -m "deps: add daemonize and nix for Unix daemon support"
```

---

## Task 2: Create Socket RPC Module

**Files:**
- Create: `crates/sync-engine/src/socket.rs`
- Modify: `crates/sync-engine/src/lib.rs`

**Step 1: Create socket.rs with Unix socket server**

```rust
//! Unix socket RPC server for CLI↔Daemon communication

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tracing::{error, info, warn};

/// RPC Request
#[derive(Debug, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<Value>,
    pub id: Option<Value>,
}

/// RPC Response
#[derive(Debug, Serialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
    pub id: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Unix socket RPC server
pub struct SocketServer {
    socket_path: PathBuf,
}

impl SocketServer {
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    /// Start the socket server
    pub async fn start<F>(&self, handler: F) -> Result<()>
    where
        F: Fn(RpcRequest) -> RpcResponse + Send + Sync + 'static,
    {
        // Remove stale socket if exists
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)
                .context("Failed to remove stale socket")?;
        }

        let listener = UnixListener::bind(&self.socket_path)
            .context("Failed to bind to Unix socket")?;

        // Set permissions to user-only
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&self.socket_path, perms)
                .context("Failed to set socket permissions")?;
        }

        info!("Socket server listening on {:?}", self.socket_path);

        let handler = std::sync::Arc::new(handler);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let handler = handler.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, handler).await {
                            error!("Connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Accept error: {}", e);
                }
            }
        }
    }
}

async fn handle_connection<F>(stream: UnixStream, handler: std::sync::Arc<F>) -> Result<()>
where
    F: Fn(RpcRequest) -> RpcResponse + Send + Sync,
{
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let request: RpcRequest = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let response = RpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(RpcError {
                        code: -32700,
                        message: format!("Parse error: {}", e),
                        data: None,
                    }),
                    id: None,
                };
                send_response(&mut writer, response).await?;
                line.clear();
                continue;
            }
        };

        let response = handler(request);
        send_response(&mut writer, response).await?;
        line.clear();
    }

    Ok(())
}

async fn send_response(writer: &mut tokio::net::unix::OwnedWriteHalf, response: RpcResponse) -> Result<()> {
    let json = serde_json::to_string(&response)?;
    writer.write_all(json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}

/// Socket client for CLI
pub struct SocketClient {
    socket_path: PathBuf,
}

impl SocketClient {
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    pub async fn call(&self, request: RpcRequest) -> Result<RpcResponse> {
        let stream = UnixStream::connect(&self.socket_path)
            .await
            .context("Failed to connect to daemon socket. Is the daemon running?")?;

        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        // Send request
        let json = serde_json::to_string(&request)?;
        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        // Read response
        let mut line = String::new();
        reader.read_line(&mut line).await?;

        let response: RpcResponse = serde_json::from_str(&line)
            .context("Failed to parse RPC response")?;

        Ok(response)
    }

    /// Simple ping to check if daemon is alive
    pub async fn ping(&self) -> Result<bool> {
        let request = RpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "daemon.ping".to_string(),
            params: None,
            id: Some(1.into()),
        };

        match self.call(request).await {
            Ok(response) => Ok(response.error.is_none()),
            Err(_) => Ok(false),
        }
    }
}
```

**Step 2: Add socket module to lib.rs**

Modify `crates/sync-engine/src/lib.rs`:
```rust
pub mod manager;
pub mod client;
pub mod planner;
pub mod worker;
pub mod socket;  // Add this line

pub use manager::SyncManager;
pub use client::ApiClient;
pub use sync_domain::{SyncStatus, SyncRoot};
pub use client_state::Database;
pub use socket::{SocketServer, SocketClient, RpcRequest, RpcResponse};  // Add this line
```

**Step 3: Test compile**

```bash
cargo check -p sync-engine
```

**Step 4: Commit**

```bash
git add crates/sync-engine/src/socket.rs crates/sync-engine/src/lib.rs
git commit -m "feat(sync-engine): add Unix socket RPC module"
```

---

## Task 3: Create Daemon Process Management Module

**Files:**
- Create: `crates/sync-engine/src/daemon.rs`
- Modify: `crates/sync-engine/src/lib.rs`

**Step 1: Create daemon.rs with PID file management**

```rust
//! Daemon process management (PID files, forking, lifecycle)

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use std::process;
use tracing::{info, warn};

/// Daemon lifecycle manager
pub struct DaemonHandle {
    pid_file: PathBuf,
    socket_path: PathBuf,
}

impl DaemonHandle {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            pid_file: data_dir.join("daemon.pid"),
            socket_path: data_dir.join("daemon.sock"),
        }
    }

    /// Check if daemon is running
    pub fn is_running(&self) -> bool {
        if !self.pid_file.exists() {
            return false;
        }

        match fs::read_to_string(&self.pid_file) {
            Ok(pid_str) => {
                match pid_str.trim().parse::<u32>() {
                    Ok(pid) => Self::process_exists(pid),
                    Err(_) => false,
                }
            }
            Err(_) => false,
        }
    }

    /// Get PID of running daemon
    pub fn get_pid(&self) -> Option<u32> {
        if !self.pid_file.exists() {
            return None;
        }

        fs::read_to_string(&self.pid_file)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
    }

    /// Write PID file
    pub fn write_pid(&self) -> Result<()> {
        let pid = process::id();
        fs::write(&self.pid_file, pid.to_string())
            .context("Failed to write PID file")?;
        info!("Wrote PID file: {} (pid={})", self.pid_file.display(), pid);
        Ok(())
    }

    /// Remove PID file
    pub fn remove_pid(&self) -> Result<()> {
        if self.pid_file.exists() {
            fs::remove_file(&self.pid_file)
                .context("Failed to remove PID file")?;
            info!("Removed PID file: {}", self.pid_file.display());
        }
        Ok(())
    }

    /// Clean up stale files (PID file exists but process not running)
    pub fn cleanup_stale(&self) -> Result<()> {
        if self.pid_file.exists() && !self.is_running() {
            warn!("Removing stale PID file: {}", self.pid_file.display());
            fs::remove_file(&self.pid_file).ok();
        }

        if self.socket_path.exists() {
            warn!("Removing stale socket: {:?}", self.socket_path);
            fs::remove_file(&self.socket_path).ok();
        }

        Ok(())
    }

    /// Get socket path
    pub fn socket_path(&self) -> &PathBuf {
        &self.socket_path
    }

    /// Check if a process exists
    #[cfg(unix)]
    fn process_exists(pid: u32) -> bool {
        unsafe {
            libc::kill(pid as libc::pid_t, 0) == 0
        }
    }

    #[cfg(not(unix))]
    fn process_exists(pid: u32) -> bool {
        // Windows implementation would use OpenProcess
        true // Fallback
    }
}

/// Stop the daemon gracefully
pub fn stop_daemon(pid_file: &PathBuf) -> Result<()> {
    if !pid_file.exists() {
        anyhow::bail!("Daemon not running (no PID file)");
    }

    let pid_str = fs::read_to_string(pid_file)
        .context("Failed to read PID file")?;
    let pid = pid_str.trim().parse::<u32>()
        .context("Invalid PID in file")?;

    #[cfg(unix)]
    {
        use nix::sys::signal::{self, Signal};
        use nix::unistd::Pid;

        signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM)
            .context("Failed to send SIGTERM to daemon")?;

        info!("Sent SIGTERM to daemon (pid={})", pid);
    }

    #[cfg(not(unix))]
    {
        anyhow::bail!("Daemon stop not implemented for this platform");
    }

    Ok(())
}

/// Wait for daemon to stop
pub async fn wait_for_stop(pid_file: &PathBuf, timeout_secs: u64) -> Result<()> {
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);

    while start.elapsed() < timeout {
        if !pid_file.exists() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    anyhow::bail!("Timeout waiting for daemon to stop")
}
```

**Step 2: Add daemon module to lib.rs**

Modify `crates/sync-engine/src/lib.rs`:
```rust
pub mod manager;
pub mod client;
pub mod planner;
pub mod worker;
pub mod socket;
pub mod daemon;  // Add this line

pub use manager::SyncManager;
pub use client::ApiClient;
pub use sync_domain::{SyncStatus, SyncRoot};
pub use client_state::Database;
pub use socket::{SocketServer, SocketClient, RpcRequest, RpcResponse};
pub use daemon::{DaemonHandle, stop_daemon, wait_for_stop};  // Add this line
```

**Step 3: Test compile**

```bash
cargo check -p sync-engine
```

**Step 4: Commit**

```bash
git add crates/sync-engine/src/daemon.rs crates/sync-engine/src/lib.rs
git commit -m "feat(sync-engine): add daemon process management module"
```

---

## Task 4: Update Config with CRUD Methods

**Files:**
- Modify: `apps/desktop/src/config.rs`

**Step 1: Add FolderUpdate struct and update methods**

Add to `apps/desktop/src/config.rs` before the tests:

```rust
/// Update parameters for a sync folder
#[derive(Debug, Default)]
pub struct FolderUpdate {
    pub local_path: Option<PathBuf>,
    pub enabled: Option<bool>,
    pub direction: Option<SyncDirection>,
    pub add_ignore_patterns: Vec<String>,
    pub remove_ignore_patterns: Vec<String>,
    pub clear_ignores: bool,
}

impl Config {
    // ... existing methods ...

    /// Update an existing sync folder
    pub fn update_sync_folder(&mut self, folder_id: uuid::Uuid, updates: FolderUpdate) -> Result<bool> {
        let folder = match self.sync_folders.iter_mut().find(|f| f.folder_id == folder_id) {
            Some(f) => f,
            None => return Ok(false),
        };

        if let Some(path) = updates.local_path {
            folder.local_path = path;
        }

        if let Some(enabled) = updates.enabled {
            folder.enabled = enabled;
        }

        if let Some(direction) = updates.direction {
            folder.direction = direction;
        }

        if updates.clear_ignores {
            folder.ignore_patterns = default_ignore_patterns();
        } else {
            // Remove patterns
            for pattern in &updates.remove_ignore_patterns {
                folder.ignore_patterns.retain(|p| p != pattern);
            }
            // Add patterns
            for pattern in updates.add_ignore_patterns {
                if !folder.ignore_patterns.contains(&pattern) {
                    folder.ignore_patterns.push(pattern);
                }
            }
        }

        self.save()?;
        Ok(true)
    }

    /// Set folder enabled/disabled
    pub fn set_folder_enabled(&mut self, folder_id: uuid::Uuid, enabled: bool) -> Result<bool> {
        self.update_sync_folder(folder_id, FolderUpdate {
            enabled: Some(enabled),
            ..Default::default()
        })
    }

    /// Get a sync folder by ID
    pub fn get_sync_folder(&self, folder_id: uuid::Uuid) -> Option<&SyncFolderConfig> {
        self.sync_folders.iter().find(|f| f.folder_id == folder_id)
    }
}
```

**Step 2: Test compile**

```bash
cargo check -p rustshare-desktop
```

**Step 3: Commit**

```bash
git add apps/desktop/src/config.rs
git commit -m "feat(config): add update methods for sync folders"
```

---

## Task 5: Add Daemon Commands to CLI

**Files:**
- Modify: `apps/desktop/src/main.rs`

**Step 1: Update Commands enum to add Daemon subcommands**

Replace the existing `Daemon` command with:

```rust
/// Manage the sync daemon
#[derive(Subcommand)]
enum DaemonCommands {
    /// Start the daemon in the background
    Start,
    /// Stop the running daemon
    Stop,
    /// Check daemon status
    Status,
    /// View daemon logs
    Logs,
}

// Then update the Commands enum:
#[derive(Subcommand)]
enum Commands {
    /// Login to account
    Login {
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
```

**Step 2: Add daemon command handling in main()**

Add the daemon command match arm in the main() function:

```rust
Commands::Daemon { command } => match command {
    DaemonCommands::Start => {
        let handle = DaemonHandle::new(app_data_dir.clone());
        
        if handle.is_running() {
            println!("Daemon is already running (pid={})", 
                handle.get_pid().unwrap_or(0));
            return Ok(());
        }

        handle.cleanup_stale()?;

        // Fork to background
        let log_path = app_data_dir.join("daemon.log");
        let daemonize = daemonize::Daemonize::new()
            .pid_file(handle.get_pid_file())
            .working_directory(&workspace)
            .stdout(fs::File::create(&log_path)?)
            .stderr(fs::File::create(&log_path)?);

        match daemonize.start() {
            Ok(_) => {
                // We are now in the daemon process
                info!("Daemon started");
                
                // Write our PID
                handle.write_pid()?;
                
                // Start the sync core
                let rt = tokio::runtime::Runtime::new()?;
                rt.block_on(async {
                    if let Err(e) = run_daemon(core, handle).await {
                        error!("Daemon error: {}", e);
                    }
                });
            }
            Err(e) => {
                anyhow::bail!("Failed to start daemon: {}", e);
            }
        }
    }
    DaemonCommands::Stop => {
        let handle = DaemonHandle::new(app_data_dir.clone());
        
        if !handle.is_running() {
            println!("Daemon is not running");
            handle.cleanup_stale()?;
            return Ok(());
        }

        stop_daemon(&handle.pid_file)?;
        
        // Wait for shutdown
        match wait_for_stop(&handle.pid_file, 5).await {
            Ok(_) => println!("✓ Daemon stopped"),
            Err(_) => {
                println!("⚠ Daemon did not stop gracefully, cleaning up");
                handle.cleanup_stale()?;
            }
        }
    }
    DaemonCommands::Status => {
        let handle = DaemonHandle::new(app_data_dir.clone());
        
        if handle.is_running() {
            let pid = handle.get_pid().unwrap_or(0);
            println!("✓ Daemon is running (pid={})", pid);
            
            // Check socket responsiveness
            let client = SocketClient::new(handle.socket_path().clone());
            match client.ping().await {
                Ok(true) => println!("✓ Daemon is responsive"),
                Ok(false) => println!("⚠ Daemon is not responding to pings"),
                Err(e) => println!("⚠ Cannot connect: {}", e),
            }
        } else {
            println!("✗ Daemon is not running");
            handle.cleanup_stale()?;
        }
    }
    DaemonCommands::Logs => {
        let log_path = app_data_dir.join("daemon.log");
        if log_path.exists() {
            let contents = fs::read_to_string(&log_path)?;
            print!("{}", contents);
        } else {
            println!("No log file found");
        }
    }
},
```

**Step 3: Add necessary imports**

Add to imports in `main.rs`:
```rust
use rustshare_desktop::config::{Config, FolderUpdate, SyncDirection};
use sync_engine::{SocketClient, DaemonHandle, stop_daemon, wait_for_stop};
```

**Step 4: Test compile**

```bash
cargo check -p rustshare-desktop
```

**Step 5: Commit**

```bash
git add apps/desktop/src/main.rs
git commit -m "feat(cli): add daemon start/stop/status/logs commands"
```

---

## Task 6: Add Sync Remove/Update/Enable/Disable Commands

**Files:**
- Modify: `apps/desktop/src/main.rs`

**Step 1: Extend SyncAction enum**

Add to `SyncAction`:

```rust
#[derive(Subcommand)]
enum SyncAction {
    /// Add a remote root to sync locally
    Add {
        remote_path: String,
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
        /// Change local path
        #[arg(long)]
        local_path: Option<String>,
        /// Change sync direction (bidir, up, down)
        #[arg(long, value_enum)]
        direction: Option<DirectionArg>,
        /// Add ignore pattern
        #[arg(long)]
        ignore_pattern: Vec<String>,
        /// Remove ignore pattern
        #[arg(long)]
        remove_ignore: Vec<String>,
        /// Clear all ignore patterns
        #[arg(long)]
        clear_ignores: bool,
    },
    /// Enable a sync root
    Enable {
        root_id: Uuid,
    },
    /// Disable a sync root
    Disable {
        root_id: Uuid,
    },
    // ... rest of existing commands
}

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
```

**Step 2: Add command handlers**

Add match arms in the Sync command handler:

```rust
Commands::Sync { action } => match action {
    SyncAction::Add { remote_path, local_path } => {
        // ... existing code
    }
    SyncAction::List => {
        // ... existing code
    }
    SyncAction::Remove { root_id } => {
        // Remove from database
        core.manager.database().lock().await.remove_sync_root(root_id)?;
        
        // Remove from config
        let mut config = Config::load()?;
        if config.remove_sync_folder(root_id)? {
            println!("✓ Removed sync root {}", root_id);
        } else {
            println!("⚠ Sync root {} not found in config", root_id);
        }
        
        // Notify daemon if running
        notify_daemon_config_change(&app_data_dir, root_id).await?;
    }
    SyncAction::Update { root_id, local_path, direction, ignore_pattern, remove_ignore, clear_ignores } => {
        let mut config = Config::load()?;
        
        let updates = FolderUpdate {
            local_path: local_path.map(PathBuf::from),
            direction: direction.map(|d| d.into()),
            add_ignore_patterns: ignore_pattern,
            remove_ignore_patterns: remove_ignore,
            clear_ignores,
            ..Default::default()
        };
        
        if config.update_sync_folder(root_id, updates)? {
            println!("✓ Updated sync root {}", root_id);
            notify_daemon_config_change(&app_data_dir, root_id).await?;
        } else {
            anyhow::bail!("Sync root {} not found", root_id);
        }
    }
    SyncAction::Enable { root_id } => {
        let mut config = Config::load()?;
        if config.set_folder_enabled(root_id, true)? {
            println!("✓ Enabled sync root {}", root_id);
            notify_daemon_config_change(&app_data_dir, root_id).await?;
        } else {
            anyhow::bail!("Sync root {} not found", root_id);
        }
    }
    SyncAction::Disable { root_id } => {
        let mut config = Config::load()?;
        if config.set_folder_enabled(root_id, false)? {
            println!("✓ Disabled sync root {}", root_id);
            notify_daemon_config_change(&app_data_dir, root_id).await?;
        } else {
            anyhow::bail!("Sync root {} not found", root_id);
        }
    }
    // ... rest of existing handlers
},
```

**Step 3: Add helper function**

Add helper function:

```rust
async fn notify_daemon_config_change(app_data_dir: &PathBuf, root_id: Uuid) -> Result<()> {
    let handle = DaemonHandle::new(app_data_dir.clone());
    
    if handle.is_running() {
        let client = SocketClient::new(handle.socket_path().clone());
        let request = RpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "config.update".to_string(),
            params: Some(serde_json::json!({
                "root_id": root_id,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            })),
            id: Some(1.into()),
        };
        
        match client.call(request).await {
            Ok(_) => tracing::debug!("Notified daemon of config change"),
            Err(e) => tracing::warn!("Failed to notify daemon: {}", e),
        }
    }
    
    Ok(())
}
```

**Step 4: Test compile**

```bash
cargo check -p rustshare-desktop
```

**Step 5: Commit**

```bash
git add apps/desktop/src/main.rs
git commit -m "feat(cli): add sync remove/update/enable/disable commands"
```

---

## Task 7: Update SyncManager to Use Unix Socket

**Files:**
- Modify: `crates/sync-engine/src/manager.rs`
- Modify: `crates/sync-engine/src/lib.rs`

**Step 1: Replace TCP RPC with Unix socket in manager.rs**

Replace the TCP-based RPC with Unix socket:

```rust
use crate::client::ApiClient;
use crate::socket::{SocketServer, RpcRequest, RpcResponse, RpcError};
use client_state::Database;
use file_ops::FsWatcher;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::info;
use sync_domain::SyncRoot;
use serde_json::Value;
use uuid::Uuid;

pub struct SyncManager {
    database: Arc<Mutex<Database>>,
    _client: ApiClient,
    workspace_root: PathBuf,
}

impl SyncManager {
    pub fn new(database: Database, client: ApiClient, workspace_root: PathBuf) -> Self {
        Self {
            database: Arc::new(Mutex::new(database)),
            _client: client,
            workspace_root,
        }
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        info!("Starting Sync Manager...");
        
        let (tx, mut rx) = mpsc::channel(100);
        let mut watcher = FsWatcher::new(tx)?;
        watcher.watch(&self.workspace_root)?;

        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                info!("FS Event: {:?}", event);
                // Trigger planning/syncing logic here
            }
        });

        Ok(())
    }

    pub async fn sync_root(&self, sync_root: SyncRoot) -> anyhow::Result<()> {
        info!("Syncing root: {}", sync_root.remote_path);
        
        let db = self.database.lock().await;
        db.save_sync_root(&sync_root)?;

        let filters = db.get_filters(sync_root.id)?;
        info!("Active filters for root: {:?}", filters);

        Ok(())
    }

    pub fn database(&self) -> Arc<Mutex<Database>> {
        self.database.clone()
    }

    /// Start the Unix socket RPC server
    pub async fn start_socket_server(&self, socket_path: PathBuf) -> anyhow::Result<()> {
        let db = self.database.clone();
        
        let server = SocketServer::new(socket_path);
        
        server.start(move |request| {
            handle_rpc_request(request, db.clone())
        }).await
    }
}

fn handle_rpc_request(
    request: RpcRequest,
    db: Arc<Mutex<Database>>,
) -> RpcResponse {
    info!("Received RPC: {}", request.method);
    
    let result = match request.method.as_str() {
        "daemon.ping" => {
            Some(Value::Bool(true))
        }
        "daemon.stop" => {
            // Signal shutdown - in real impl, use a channel
            Some(serde_json::json!({"status": "stopping"}))
        }
        "sync.request" => {
            // Trigger sync for path in params
            Some(serde_json::json!({"status": "queued"}))
        }
        "sync.status" => {
            // Query status for path in params
            Some(serde_json::json!({"status": "synced"}))
        }
        "config.update" => {
            // Handle config update notification
            info!("Config update received: {:?}", request.params);
            Some(serde_json::json!({"applied": true}))
        }
        _ => None,
    };

    let error = if result.is_none() { 
        Some(RpcError {
            code: -32601,
            message: "Method not found".to_string(),
            data: None,
        })
    } else { 
        None 
    };

    RpcResponse {
        jsonrpc: "2.0".to_string(),
        result,
        error,
        id: request.id,
    }
}
```

**Step 2: Update SyncCore in lib.rs**

```rust
use anyhow::Result;
use std::path::PathBuf;

pub struct SyncCore {
    pub manager: SyncManager,
    socket_path: PathBuf,
}

impl SyncCore {
    pub fn new(database: Database, client: ApiClient, workspace_root: PathBuf, socket_path: PathBuf) -> Self {
        let manager = SyncManager::new(database, client, workspace_root);
        Self { manager, socket_path }
    }

    pub async fn start(&self) -> Result<()> {
        // Start socket server in background
        let socket_path = self.socket_path.clone();
        let manager = &self.manager;
        
        tokio::spawn(async move {
            if let Err(e) = manager.start_socket_server(socket_path).await {
                tracing::error!("Socket server error: {}", e);
            }
        });
        
        // Start sync manager
        self.manager.start().await
    }

    pub async fn register_root(&self, root: SyncRoot) -> Result<()> {
        self.manager.sync_root(root).await
    }

    pub fn get_status(&self) -> SyncStatus {
        SyncStatus::Idle
    }
}
```

**Step 3: Test compile**

```bash
cargo check -p sync-engine
```

**Step 4: Commit**

```bash
git add crates/sync-engine/src/manager.rs crates/sync-engine/src/lib.rs
git commit -m "refactor(sync-engine): replace TCP RPC with Unix socket"
```

---

## Task 8: Update Main.rs to Use New SyncCore Constructor

**Files:**
- Modify: `apps/desktop/src/main.rs`

**Step 1: Update SyncCore instantiation**

Find where `SyncCore::new` is called and add the socket path:

```rust
let socket_path = app_data_dir.join("daemon.sock");
let core = SyncCore::new(db, client, workspace.clone(), socket_path);
```

**Step 2: Update the daemon run function**

Add the `run_daemon` helper function:

```rust
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
```

**Step 3: Test compile**

```bash
cargo check -p rustshare-desktop
```

**Step 4: Commit**

```bash
git add apps/desktop/src/main.rs
git commit -m "refactor: update main to use Unix socket SyncCore"
```

---

## Task 9: Add Tests for Config CRUD

**Files:**
- Modify: `apps/desktop/src/config.rs`

**Step 1: Add tests for new methods**

Add to the test module at the bottom of config.rs:

```rust
#[test]
fn test_update_sync_folder() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    let mut config = Config::default();
    let folder_id = Uuid::new_v4();
    
    // Add initial folder
    config.add_sync_folder(folder_id, "/test/path".into()).unwrap();
    
    // Update folder
    let updates = FolderUpdate {
        local_path: Some("/new/path".into()),
        enabled: Some(false),
        direction: Some(SyncDirection::UploadOnly),
        add_ignore_patterns: vec!["*.log".to_string()],
        ..Default::default()
    };
    
    assert!(config.update_sync_folder(folder_id, updates).unwrap());
    
    let folder = config.get_sync_folder(folder_id).unwrap();
    assert_eq!(folder.local_path, PathBuf::from("/new/path"));
    assert!(!folder.enabled);
    assert_eq!(folder.direction, SyncDirection::UploadOnly);
    assert!(folder.ignore_patterns.contains(&"*.log".to_string()));
}

#[test]
fn test_set_folder_enabled() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    let mut config = Config::default();
    let folder_id = Uuid::new_v4();
    
    config.add_sync_folder(folder_id, "/test/path".into()).unwrap();
    
    // Disable
    assert!(config.set_folder_enabled(folder_id, false).unwrap());
    assert!(!config.get_sync_folder(folder_id).unwrap().enabled);
    
    // Enable
    assert!(config.set_folder_enabled(folder_id, true).unwrap());
    assert!(config.get_sync_folder(folder_id).unwrap().enabled);
}

#[test]
fn test_remove_sync_folder() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    let mut config = Config::default();
    let folder_id = Uuid::new_v4();
    
    config.add_sync_folder(folder_id, "/test/path".into()).unwrap();
    assert_eq!(config.sync_folders.len(), 1);
    
    assert!(config.remove_sync_folder(folder_id).unwrap());
    assert!(config.sync_folders.is_empty());
    
    // Removing again returns false
    assert!(!config.remove_sync_folder(folder_id).unwrap());
}
```

**Step 2: Run tests**

```bash
cargo test -p rustshare-desktop --lib
```

**Step 3: Commit**

```bash
git add apps/desktop/src/config.rs
git commit -m "test(config): add tests for CRUD operations"
```

---

## Task 10: Final Integration Test

**Files:**
- None (run tests)

**Step 1: Build entire project**

```bash
cargo build -p rustshare-desktop
```

**Step 2: Run all tests**

```bash
cargo test -p rustshare-desktop -p sync-engine
```

**Step 3: Check formatting**

```bash
cargo fmt -- --check
```

**Step 4: Fix any formatting issues**

```bash
cargo fmt
```

**Step 5: Final commit**

```bash
git add -A
git commit -m "chore: format code"
```

---

## Summary

This implementation plan transforms the rustshare-desktop CLI from a foreground TCP daemon to a proper Unix socket-based background daemon with full CRUD support for sync locations.

Key changes:
1. Unix socket at `~/.config/rustshare/daemon.sock` replaces TCP port 4242
2. PID file at `~/.config/rustshare/daemon.pid` enables lifecycle management
3. New daemon commands: `start`, `stop`, `status`, `logs`
4. New sync commands: `remove`, `update`, `enable`, `disable`
5. All changes persist to both SQLite and config.toml
