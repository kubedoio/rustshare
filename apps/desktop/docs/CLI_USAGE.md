# RustShare Desktop CLI Usage Guide

## Overview

The `rustshare-desktop` CLI provides command-line control over the RustShare sync daemon. The daemon runs as a background process, synchronizing files between your local workspace and the RustShare server.

## Quick Start

```bash
# Login to your account
rustshare-desktop login

# Start the sync daemon
rustshare-desktop daemon start

# Check daemon status
rustshare-desktop daemon status

# Add a folder to sync
rustshare-desktop sync add "/remote/project" "./project"

# List configured sync roots
rustshare-desktop sync list
```

## Commands

### Authentication

#### `login`
Authenticate with the RustShare server using device pairing (default) or explicit token.

```bash
# Default: device pairing flow
rustshare-desktop login

# With explicit token
rustshare-desktop login --token <your-api-token>
```

### Daemon Management

#### `daemon start`
Start the sync daemon in the background.

```bash
rustshare-desktop daemon start
```

The daemon will:
- Fork to the background
- Write its PID to `~/.config/rustshare/daemon.pid`
- Create a Unix socket at `~/.config/rustshare/daemon.sock`
- Begin synchronizing configured folders
- Log output to `~/.config/rustshare/daemon.log`

#### `daemon stop`
Stop the running daemon gracefully.

```bash
rustshare-desktop daemon stop
```

Sends SIGTERM to the daemon process and waits for graceful shutdown (up to 10 seconds).

#### `daemon status`
Check if the daemon is running.

```bash
rustshare-desktop daemon status
```

Output examples:
```
Daemon is running (PID: 12345)
Daemon is responsive
```

or:

```
Daemon is not running
```

#### `daemon logs`
Display the daemon log output.

```bash
rustshare-desktop daemon logs
```

### Sync Management

#### `sync add <remote_path> <local_path>`
Add a remote folder to sync locally.

```bash
rustshare-desktop sync add "/remote/project" "./project"
```

This creates a new sync root with:
- A unique UUID assigned automatically
- Bidirectional sync (default)
- Default ignore patterns (hidden files, temp files)

#### `sync list`
List all configured sync roots.

```bash
rustshare-desktop sync list
```

Example output:
```
Configured Sync Roots:
- [550e8400-e29b-41d4-a716-446655440000] ./project (Remote: /remote/project) [enabled] direction=Bidirectional
- [660f9511-f30c-52e5-b827-557766551111] ./docs (Remote: /remote/docs) [disabled] direction=UploadOnly
```

#### `sync remove <root_id>`
Remove a sync root configuration.

```bash
rustshare-desktop sync remove 550e8400-e29b-41d4-a716-446655440000
```

Removes the sync root from both the database and config file.

#### `sync update <root_id> [options]`
Update sync root configuration.

```bash
# Change local path
rustshare-desktop sync update 550e8400-e29b-41d4-a716-446655440000 --local-path "/new/path"

# Change sync direction (bidir, up, down)
rustshare-desktop sync update 550e8400-e29b-41d4-a716-446655440000 --direction up

# Add ignore patterns
rustshare-desktop sync update 550e8400-e29b-41d4-a716-446655440000 --ignore-pattern "*.log" --ignore-pattern "*.tmp"

# Remove ignore patterns
rustshare-desktop sync update 550e8400-e29b-41d4-a716-446655440000 --remove-ignore "*.log"

# Clear all ignores and reset to defaults
rustshare-desktop sync update 550e8400-e29b-41d4-a716-446655440000 --clear-ignores
```

**Direction options:**
- `bidir` - Bidirectional sync (default)
- `up` - Upload only (local changes pushed to server)
- `down` - Download only (remote changes pulled to local)

#### `sync enable <root_id>`
Enable a previously disabled sync root.

```bash
rustshare-desktop sync enable 550e8400-e29b-41d4-a716-446655440000
```

#### `sync disable <root_id>`
Disable a sync root (pause synchronization).

```bash
rustshare-desktop sync disable 550e8400-e29b-41d4-a716-446655440000
```

### General

#### `status`
Show current sync status.

```bash
rustshare-desktop status
```

## Configuration Files

| File | Location | Purpose |
|------|----------|---------|
| `config.toml` | `~/.config/rustshare/config.toml` | User settings, sync folders |
| `daemon.sock` | `~/.config/rustshare/daemon.sock` | Unix socket for CLI↔Daemon |
| `daemon.pid` | `~/.config/rustshare/daemon.pid` | Daemon process ID |
| `daemon.log` | `~/.config/rustshare/daemon.log` | Daemon log output |
| `rustshare.db` | `~/.local/share/rustshare/rustshare.db` | SQLite database |

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `RUSTSHARE_WORKSPACE` | `~/RustShare` | Default workspace root |

## Examples

### Complete Workflow

```bash
# 1. Login
rustshare-desktop login

# 2. Start daemon
rustshare-desktop daemon start

# 3. Add folders to sync
rustshare-desktop sync add "/projects/work" "./work"
rustshare-desktop sync add "/projects/personal" "./personal"

# 4. Check status
rustshare-desktop daemon status
rustshare-desktop status

# 5. List sync roots
rustshare-desktop sync list

# 6. Pause one folder
rustshare-desktop sync disable <personal-uuid>

# 7. Later, resume it
rustshare-desktop sync enable <personal-uuid>

# 8. View logs if needed
rustshare-desktop daemon logs

# 9. Stop daemon when done
rustshare-desktop daemon stop
```

### Migration Example

```bash
# Old folder moving to new location
OLD_UUID="550e8400-e29b-41d4-a716-446655440000"

# Update the path
rustshare-desktop sync update $OLD_UUID --local-path "/new/location"
```

## Troubleshooting

### Daemon won't start

```bash
# Check if already running
rustshare-desktop daemon status

# View logs for errors
rustshare-desktop daemon logs

# Force cleanup and retry
rm ~/.config/rustshare/daemon.pid
rm ~/.config/rustshare/daemon.sock
rustshare-desktop daemon start
```

### Sync not working

```bash
# Check daemon is responsive
rustshare-desktop daemon status

# Verify sync root is enabled
rustshare-desktop sync list

# Check logs
rustshare-desktop daemon logs
```

### Permission denied on socket

The Unix socket is created with 0600 permissions (user-only). If you get permission errors:

```bash
# Check socket permissions
ls -la ~/.config/rustshare/daemon.sock

# Should show: srwx------ ... daemon.sock
# If not, stop and restart daemon
rustshare-desktop daemon stop
rm ~/.config/rustshare/daemon.sock
rustshare-desktop daemon start
```
