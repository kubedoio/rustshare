# RustShare Vault Sync

> **Disclaimer:** Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.

Sync your local Obsidian vault to RustShare — a self-hosted vault synchronization backend that supports local Markdown-based vaults.

## Features

- **Manual sync** — Full vault scan with SHA-256 change detection
- **Incremental sync** — Automatic upload/download on file create, modify, delete, rename
- **Conflict resolution** — Never silently overwrite; creates conflict copies
- **Offline queue** — Changes queued while offline, synced when connection restored
- **Tombstone support** — Deleted files properly tracked, preventing resurrection
- **Content-addressed storage** — Deduplicated by SHA-256, preserving byte-for-byte fidelity
- **Status bar** — Real-time sync status (disconnected, syncing, synced, conflict, error, offline)

## Requirements

- Obsidian v0.15.0 or later (desktop only)
- A RustShare instance with Vault Sync API enabled (`/api/vault-sync/v1`)

## Installation (Manual)

This plugin is not yet available in the Obsidian Community Plugins marketplace.

1. **Build the plugin** (or download a pre-built release):
   ```bash
   npm install
   npm run build
   ```

2. **Create the plugin directory** in your Obsidian vault:
   ```bash
   mkdir -p /path/to/your/vault/.obsidian/plugins/rustshare-vault-sync
   ```

3. **Copy the plugin files**:
   ```bash
   cp main.js manifest.json styles.css \
      /path/to/your/vault/.obsidian/plugins/rustshare-vault-sync/
   ```

4. **Enable the plugin** in Obsidian:
   - Settings → Community Plugins → Turn on Community Plugins
   - Find "RustShare Vault Sync" and enable it

## Configuration

Open **Settings → RustShare Vault Sync**:

| Setting | Description |
|---|---|
| **RustShare URL** | Your RustShare instance URL (e.g., `https://rustshare.io`) |
| **Device name** | Friendly name for this device (e.g., "My MacBook") |
| **Vault ID** | Existing vault UUID to connect to, or leave empty to create a new vault |
| **Auto-sync interval** | Minutes between automatic full syncs (0 to disable) |

## Usage

### Connect to RustShare

Open the command palette (`Ctrl/Cmd + P`) and run:

> **RustShare Vault Sync: Connect or create vault**

The plugin will:
1. Request a device pairing code from RustShare
2. Show you a pairing code and a URL to approve it
3. Poll for your approval (checks every 3 seconds)
4. Once approved, register this device and create or connect to a vault

No manual token entry is required.

### Manual Sync

Open the command palette and run:

> **RustShare Vault Sync: Sync vault to RustShare**

This performs a full bidirectional sync:
1. Scans all local files and computes SHA-256 hashes
2. Fetches the server manifest
3. Uploads local-only or changed files
4. Downloads remote-only or changed files
5. Creates conflict copies when both sides changed
6. Updates local sync state

### Incremental Sync

The plugin automatically listens for file changes in your vault and queues incremental sync operations. Changes are debounced (default 1500ms) and batched for efficiency.

### Conflict Files

When a file is changed both locally and on the server, the plugin:
1. Creates a conflict copy of your local version: `<filename> (RustShare conflicted copy <device> <timestamp>)<ext>`
2. Downloads the server version to the original path
3. Updates sync state to match the server

### Ignored Paths

The following paths are ignored by default:

```
.obsidian/workspace.json
.obsidian/workspace-mobile.json
.obsidian/plugins/*/data.json
.trash/
.git/
.DS_Store
Thumbs.db
*.tmp
*.swp
```

## Troubleshooting

### "Failed to register device"

- Check that your RustShare URL is correct and reachable
- Ensure you approved the pairing code on the RustShare website
- Check the Obsidian console (Ctrl/Cmd + Shift + I) for detailed errors

### Sync seems slow on large vaults

The initial full sync scans and hashes every file. For vaults with thousands of files or large attachments, this may take a minute. Subsequent incremental syncs are much faster.

### Conflict copies keep appearing

This was a bug in versions before the 2026-06-03 fixes. Update to the latest plugin version.

## Development

```bash
# Install dependencies
npm install

# Run tests
npx vitest run

# Build for production
npm run build

# Watch mode for development
node esbuild.config.mjs
```

## Architecture

```
src/
  main.ts           # Plugin entry point, event listeners, commands
  api.ts            # RustShare API client
  sync.ts           # Full and incremental sync engine
  sync-queue.ts     # Debounced operation queue with retry
  state.ts          # Sync state persistence and migration
  settings.ts       # Plugin settings and validation
  sync-log.ts       # In-memory sync event logging
  utils.ts          # SHA-256, path filtering, conflict naming
  ui/
    settings-tab.ts # Settings UI panel
    status-bar.ts   # Status bar component
```

## License

See the main RustShare repository LICENSE file.
