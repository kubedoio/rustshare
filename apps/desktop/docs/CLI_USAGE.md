# RustShare Desktop CLI Usage Guide

## Overview

The `rustshare-desktop` binary is the current desktop client shipped from this repository.

Today it is a CLI plus background daemon, not a finished GUI shell. The CLI handles login, sync-root management, daemon lifecycle, and recovery commands. The daemon does the actual scanning, planning, upload, download, delete, and health logging.

Current implementation notes:

- default server: `https://app.rustshare.io`
- default workspace: `~/RustShare`
- macOS runtime state: `~/Library/Application Support/io.rustshare.RustShare/`
- live sync engine: the shared crate in `crates/sync-engine`
- root `/` is supported, but it means a full-account mirror

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
- Write its PID to the app-data directory as `daemon.pid`
- Create a Unix socket there as `daemon.sock`
- Begin synchronizing configured folders
- Log output to `daemon.log`

On macOS, those files live under:

```text
~/Library/Application Support/io.rustshare.RustShare/
```

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

# Show the last 50 lines
rustshare-desktop daemon logs --tail 50

# Follow the log stream
rustshare-desktop daemon logs --follow
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
- Sync scoped to the requested remote subtree only
- Directory structure mirrored before file transfer begins
- File deletes recorded as tombstones so deleted paths do not immediately get recreated
- Zero-byte files uploaded as normal files

Notes:
- `remote_path` is the source subtree on the server.
- `local_path` is the relative destination under the workspace root.
- Empty directories inside the selected subtree are mirrored locally and remotely.
- Nested files keep their relative paths. The client does not flatten uploads into the sync root.
- `remote_path` may be `/` when you intentionally want a full-account root mirror.
- For a full-account mirror, prefer using `sync doctor` regularly because `/` roots naturally surface more stale or historical server content.

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

#### `sync doctor [root_id]`
Diagnose sync root health and report known broken remote entries.

```bash
# Inspect every configured root
rustshare-desktop sync doctor

# Inspect one root and print up to 25 broken entries
rustshare-desktop sync doctor 550e8400-e29b-41d4-a716-446655440000 --limit 25

# Clear quarantined broken-entry records for one root before re-checking it
rustshare-desktop sync doctor 550e8400-e29b-41d4-a716-446655440000 --clear-quarantine
```

The report shows:
- whether the daemon is running
- whether the local path exists
- how many synced file states are indexed for the root
- how many broken remote entries are currently quarantined
- example broken paths and their latest error message

#### `sync cleanup-remote [root_id]`
Re-check quarantined remote entries and optionally delete confirmed stale remote metadata.

```bash
# Dry run a cleanup for one root
rustshare-desktop sync cleanup-remote 550e8400-e29b-41d4-a716-446655440000

# Process up to 100 entries per root
rustshare-desktop sync cleanup-remote --limit 100

# Delete only entries that still return a fresh 404/410 check
rustshare-desktop sync cleanup-remote 550e8400-e29b-41d4-a716-446655440000 --apply
```

Behavior:
- dry run is the default
- each candidate is re-checked against the server before deletion
- only confirmed missing remote files are deleted with `--apply`
- recovered files are removed from quarantine automatically
- when stale metadata is deleted with `--apply`, the client records a tombstone so the path stays deleted instead of being recreated on the next cycle

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

`status` is currently coarse. For real debugging, prefer:

- `rustshare-desktop daemon status`
- `rustshare-desktop daemon logs --tail 200`
- `rustshare-desktop sync doctor <root-uuid>`

## Configuration Files

| File | Location | Purpose |
|------|----------|---------|
| `config.toml` | platform config dir | user settings and sync folder metadata |
| `daemon.sock` | app-data dir | Unix socket for CLI↔daemon RPC |
| `daemon.pid` | app-data dir | daemon process ID |
| `daemon.log` | app-data dir | daemon log output |
| `rustshare.db` | app-data dir | SQLite state for sync roots, file states, upload sessions, tombstones, and quarantine |
| `device_id` | app-data dir | stable local device identifier |
| `token.txt` | app-data dir | daemon-readable auth token fallback |

On macOS, the app-data dir resolves to:

```text
~/Library/Application Support/io.rustshare.RustShare/
```

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
rustshare-desktop daemon logs --follow

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
rustshare-desktop daemon logs --tail 100

# Force cleanup and retry
rm ~/Library/Application\ Support/io.rustshare.RustShare/daemon.pid
rm ~/Library/Application\ Support/io.rustshare.RustShare/daemon.sock
rustshare-desktop daemon start
```

### Sync not working

```bash
# Check daemon is responsive
rustshare-desktop daemon status

# Verify sync root is enabled
rustshare-desktop sync list

# Check logs
rustshare-desktop daemon logs --tail 100

# Diagnose a specific root
rustshare-desktop sync doctor <root-uuid>

# Dry-run stale remote cleanup
rustshare-desktop sync cleanup-remote <root-uuid>
```

What "working" should look like with current sync behavior:
- A nested remote path like `/projects/work/specs/api/openapi.yaml` appears locally as `./work/specs/api/openapi.yaml`.
- Creating an empty folder in the synced subtree mirrors the folder even before any file exists inside it.
- Deleting a previously synced folder propagates as a folder delete after its child entries have converged.
- Creating an empty file syncs a real zero-byte remote file and then settles to idle.
- Files outside the configured `remote_path` are ignored for that sync root.
- A root configured for `/` mirrors the whole account tree, not just one folder.
- Missing remote blobs are quarantined per path so one broken server entry does not stall the whole root forever.
- Deletes are remembered with tombstones so removing a file on one side does not immediately recreate it from stale state.

If `sync doctor` reports broken remote entries:
- the client will skip those broken downloads temporarily and continue syncing the rest of the root
- repeated `404 Not Found` download failures usually mean stale server metadata that should be cleaned up remotely
- start with `rustshare-desktop sync cleanup-remote <root-uuid>` to see what would be removed
- use `--apply` only after reviewing the dry-run output

### Permission denied on socket

The Unix socket is created with 0600 permissions (user-only). If you get permission errors:

```bash
# Check socket permissions
ls -la ~/Library/Application\ Support/io.rustshare.RustShare/daemon.sock

# Should show: srwx------ ... daemon.sock
# If not, stop and restart daemon
rustshare-desktop daemon stop
rm ~/Library/Application\ Support/io.rustshare.RustShare/daemon.sock
rustshare-desktop daemon start
```
