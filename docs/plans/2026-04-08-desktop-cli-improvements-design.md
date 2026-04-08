# RustShare Desktop CLI Improvements Design

| Status | Approved |
| :--- | :--- |
| **Version** | 1.0.0 |
| **Author** | RustShare Team |
| **Date** | 2026-04-08 |

## 1. Overview

This design addresses three critical issues in the rustshare-desktop CLI:

1. **Sync not working** - Replace TCP port-based daemon with Unix socket communication
2. **Daemon should run as background process** - Implement proper daemon lifecycle management
3. **CLI needs full sync location CRUD** - Add remove/update/enable/disable commands

## 2. Goals

- Replace TCP port 4242 with Unix domain socket at `~/.config/rustshare/daemon.sock`
- Implement background daemon with explicit start/stop/status commands
- Extend CLI with full CRUD operations for sync locations
- Ensure configuration changes persist to both SQLite and config.toml

## 3. Non-Goals

- Auto-start daemon on CLI commands (explicit start only)
- macOS launchd integration (deferred)
- Windows named pipe support (deferred)
- GUI integration (Phase 2)

## 4. Architecture

### 4.1 Daemon Communication

```
┌─────────────┐         Unix Socket         ┌─────────────┐
│   CLI       │  ◄──────────────────────►   │   Daemon    │
│  (client)   │   ~/.config/rustshare/      │  (server)   │
└─────────────┘        daemon.sock          └─────────────┘
                              │                      │
                              │                      ▼
                              │              ┌─────────────┐
                              │              │   SQLite    │
                              │              │    (DB)     │
                              │              └─────────────┘
                              ▼
                    ┌─────────────────┐
                    │   config.toml   │
                    │   (persistent)  │
                    └─────────────────┘
```

### 4.2 File Layout

```
~/.config/rustshare/
├── config.toml        # User configuration
├── daemon.sock        # Unix socket for CLI↔Daemon RPC
├── daemon.pid         # PID file for lifecycle tracking
└── daemon.log         # Daemon stdout/stderr log
```

### 4.3 Protocol

**Transport:** Unix domain socket at `~/.config/rustshare/daemon.sock`
**Framing:** Line-delimited JSON (newline after each JSON-RPC message)
**Authentication:** None required - socket file permissions (chmod 0600) restrict access to the user

## 5. CLI Commands

### 5.1 Daemon Commands (New)

```bash
rustshare-desktop daemon start    # Fork to background, write PID file
rustshare-desktop daemon stop     # Read PID, send SIGTERM, cleanup
rustshare-desktop daemon status   # Check if daemon process is alive
rustshare-desktop daemon logs     # Tail daemon.log output
```

### 5.2 Sync Commands (Extended)

```bash
# Existing
rustshare-desktop sync add <remote_path> <local_path>

# New
rustshare-desktop sync remove <root_id>                    # Remove sync root
rustshare-desktop sync update <root_id> [options]          # Update configuration
rustshare-desktop sync enable <root_id>                    # Enable sync
rustshare-desktop sync disable <root_id>                   # Disable/pause sync

# Update options:
#   --local-path <path>              Change local path
#   --direction <bidir|up|down>      Change sync direction
#   --ignore-pattern <pattern>       Add ignore pattern
#   --remove-ignore <pattern>        Remove ignore pattern
#   --clear-ignores                  Clear all ignore patterns
```

## 6. RPC Protocol

### 6.1 Request Format

```json
{
  "jsonrpc": "2.0",
  "method": "sync.status",
  "params": {"root_id": "550e8400-e29b-41d4-a716-446655440000"},
  "id": 1
}
```

### 6.2 Response Format

```json
{
  "jsonrpc": "2.0",
  "result": {"status": "synced", "last_sync": "2026-04-08T10:00:00Z"},
  "id": 1
}
```

### 6.3 Methods

| Method | Description | Params | Returns |
|--------|-------------|--------|---------|
| `daemon.ping` | Health check | None | `{"alive": true}` |
| `daemon.stop` | Graceful shutdown | None | `{"status": "stopping"}` |
| `sync.request` | Trigger sync for root | `root_id` | `{"status": "queued"}` |
| `sync.status` | Get sync status | `root_id` | Status object |
| `config.update` | Notify of config change | `root_id`, `changes` | `{"applied": true}` |

## 7. Configuration Persistence

### 7.1 Update Workflow

All sync modification commands follow this flow:

1. **CLI validates** input (path exists, valid UUID, etc.)
2. **Update SQLite** via `SyncManager` (runtime state)
3. **Update config.toml** via `Config` struct (persistent config)
4. **Notify daemon** via socket RPC `config.update` (if daemon is running)

### 7.2 Config.toml Schema

```toml
server_url = "https://api.rustshare.io"
sync_interval = "30s"
bandwidth_limit_kbps = 0
max_concurrent_uploads = 3
max_concurrent_downloads = 3
enable_websocket = true
upload_chunk_size = 5242880

[retry]
max_attempts = 5
initial_delay = "1s"
max_delay = "300s"
backoff_multiplier = 2.0

[[sync_folders]]
folder_id = "550e8400-e29b-41d4-a716-446655440000"
local_path = "/Users/alice/work/project"
enabled = true
direction = "bidirectional"
ignore_patterns = [".*", "*.tmp", "*.swp", ".DS_Store"]

[[sync_folders]]
folder_id = "660f9511-f30c-52e5-b827-557766551111"
local_path = "/Users/alice/docs"
enabled = false
direction = "upload_only"
ignore_patterns = [".*"]
```

### 7.3 SyncDirection Enum

```rust
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncDirection {
    #[default]
    Bidirectional,
    UploadOnly,
    DownloadOnly,
}
```

## 8. Daemon Lifecycle

### 8.1 Start Sequence

1. CLI receives `daemon start` command
2. Check if daemon already running (read PID file, check process exists)
3. Fork process to background (using `daemonize` or double-fork)
4. Create Unix socket at `~/.config/rustshare/daemon.sock`
5. Write PID to `~/.config/rustshare/daemon.pid`
6. Redirect stdout/stderr to `~/.config/rustshare/daemon.log`
7. Start RPC server loop

### 8.2 Stop Sequence

1. CLI receives `daemon stop` command
2. Read PID from `~/.config/rustshare/daemon.pid`
3. Send SIGTERM to process
4. Wait for graceful shutdown (with timeout)
5. Cleanup PID file and socket file

### 8.3 Crash Recovery

- On startup, if PID file exists but process not running → remove stale files
- On startup, if socket file exists → remove and recreate
- Daemon logs all errors to `daemon.log` for debugging

## 9. Error Handling

| Scenario | Behavior |
|----------|----------|
| Daemon not running | CLI commands show clear error: "Daemon not running. Run `rustshare-desktop daemon start`" |
| Socket permission denied | Error with file path and suggested fix |
| Config file locked | Retry with backoff, then error |
| Invalid root_id | Clear error: "Sync root not found: {id}" |
| Path doesn't exist | Validation error before any changes |
| PID file stale | Auto-cleanup on next daemon start |

## 10. Security Considerations

1. **Socket permissions:** Created with mode 0600 (user read/write only)
2. **PID file:** Created with mode 0644 (readable for status checks)
3. **No network exposure:** Unix socket is local-only
4. **Token storage:** Continue using keyring/keychain for auth tokens

## 11. Testing Strategy

| Test | Description |
|------|-------------|
| Unit | Config struct CRUD operations |
| Unit | RPC message serialization/deserialization |
| Integration | Daemon start/stop/status lifecycle |
| Integration | CLI commands update both DB and config.toml |
| Integration | Socket communication CLI↔Daemon |
| E2E | Full sync workflow with daemon |

## 12. Implementation Notes

### 12.1 Dependencies to Add

```toml
[dependencies]
daemonize = "0.5"           # Unix daemonization
nix = { version = "0.29", features = ["process", "signal"] }  # Signal handling
```

### 12.2 Files to Modify

| File | Changes |
|------|---------|
| `apps/desktop/src/main.rs` | Add daemon commands, extend sync commands |
| `apps/desktop/src/config.rs` | Add update/remove methods, FolderUpdate struct |
| `crates/sync-engine/src/manager.rs` | Replace TCP with Unix socket, add daemon lifecycle |
| `crates/sync-engine/src/lib.rs` | Update SyncCore to use socket path |

### 12.3 New Files

| File | Purpose |
|------|---------|
| `crates/sync-engine/src/daemon.rs` | Daemon process management (PID file, forking) |
| `crates/sync-engine/src/socket.rs` | Unix socket RPC server/client |

## 13. Acceptance Criteria

- [ ] `rustshare-desktop daemon start` starts daemon in background
- [ ] `rustshare-desktop daemon status` correctly reports running/stopped
- [ ] `rustshare-desktop daemon stop` gracefully shuts down daemon
- [ ] Daemon listens on Unix socket, not TCP port
- [ ] `sync add` works and persists to config.toml
- [ ] `sync remove <root_id>` removes sync root
- [ ] `sync update --local-path` updates local path
- [ ] `sync update --direction` changes sync direction
- [ ] `sync enable/disable` toggles sync state
- [ ] All config changes persist across daemon restarts
- [ ] Socket has correct permissions (0600)

## 14. Open Questions

None - all decisions finalized during design review.
