# SPEC-002: Obsidian Vault Adapter and Plugin MVP

## Purpose

Define the MVP behavior for the Obsidian-side RustShare plugin.

> Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.

## Plugin Name

Preferred:

```text
RustShare Vault Sync
```

Acceptable descriptive name:

```text
RustShare Vault Sync for Obsidian
```

Do not use names forbidden in SPEC-005.

## MVP Scope

The MVP plugin supports:

```text
- desktop-first operation
- settings page
- RustShare URL configuration
- token-based authentication
- device registration
- local vault scan
- manual sync command
- Markdown upload/download
- attachment upload/download
- manifest comparison
- local sync state
- status bar indicator
- conflict file creation
```

## Non-MVP Scope

```text
- mobile support
- official community plugin submission
- automatic Markdown 3-way merge
- sharing controls inside Obsidian
- full .obsidian config sync
- real-time collaboration
```

## Local Sync State

The plugin stores local sync state in its plugin data, not inside Markdown files.

Example:

```json
{
  "vault_id": "uuid",
  "device_id": "uuid",
  "device_name": "Senol-MacBook",
  "last_server_rev": 142,
  "files": {
    "Architecture/RustShare.md": {
      "sha256": "...",
      "server_rev": 41,
      "last_synced_at": "2026-06-01T12:00:00Z"
    }
  }
}
```

## Default Ignored Paths

```text
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

## Manual Sync Flow

```text
1. User clicks “Sync vault to RustShare”.
2. Plugin scans local vault files.
3. Plugin fetches RustShare manifest.
4. Plugin compares local files with manifest.
5. Upload local-only or changed files.
6. Download remote-only or changed files.
7. Reject conflicting operations and create conflict files.
8. Update local sync state.
9. Show sync result.
```

## Warning for Double-Sync

If the vault path appears to be inside a known cloud sync folder, warn the user:

```text
This vault appears to be inside another sync folder. Using multiple sync engines on the same vault can cause conflicts. Continue only if RustShare Vault Sync is the intended sync path.
```

## Status UI

Minimum status states:

```text
Disconnected
Connected
Syncing
Synced
Conflict
Error
Offline
```

## Acceptance Criteria

```text
- Plugin connects to RustShare using token or device flow.
- Plugin creates/maps remote vault.
- Plugin uploads Markdown files.
- Plugin uploads attachments.
- Plugin downloads remote files.
- Plugin preserves folder structure.
- Plugin does not rewrite Markdown body.
- Plugin creates conflict files instead of overwriting.
- Plugin follows naming guardrails.
```
