# Phase 5 Implementation Plan: Incremental Sync & Conflict Safety

## Architecture Overview

```
Obsidian Events (create/modify/delete/rename)
  -> SyncQueue (debounce 1500ms, batch, offline queue)
    -> syncIncremental(operations: SyncOperation[])
      -> SyncEngine (rename, tombstone, conflict handling)
        -> RustShareAPI
```

## New Modules

### 1. `src/sync-log.ts` — Sync Event Logging

- `SyncLog` class with in-memory ring buffer (last 500 entries)
- Levels: `debug`, `info`, `warn`, `error`
- Each entry: `{ timestamp, level, message, path?, error? }`
- Methods: `debug()`, `info()`, `warn()`, `error()`, `getRecent(limit?)`, `clear()`
- Exported singleton instance `syncLog`

### 2. `src/sync-queue.ts` — Debounced Sync Queue

```typescript
export type SyncOperationType = 'create' | 'modify' | 'delete' | 'rename';

export interface SyncOperation {
  path: string;
  type: SyncOperationType;
  oldPath?: string;
}

export interface SyncQueueOptions {
  debounceMs: number;
  maxRetries: number;
  retryBaseDelayMs: number;
  retryMaxDelayMs: number;
}
```

- `SyncQueue` class:
  - `pending: Map<string, SyncOperation>` — dedupes by path, latest wins
  - `offlineQueue: SyncOperation[]` — queued when network is down
  - `debounceTimer: number | null`
  - `retryCount: number`
  - `isRunning: boolean`
  - Methods:
    - `add(op: SyncOperation)` — add to pending, reset debounce timer
    - `flush()` — immediately sync all pending
    - `start()` / `stop()`
    - `isOnline()` / `setOnline(status)`
    - Private: `runSync()`, `scheduleRetry()`, `clearRetry()`
  - Retry logic: exponential backoff with jitter, capped at `retryMaxDelayMs`
  - Network detection: if any API call throws with `TypeError` or status 0, mark offline
  - On reconnect: flush offline queue first, then resume normal operation

## Enhanced Modules

### 3. `src/sync.ts` — Enhanced SyncEngine

**New methods:**
- `syncIncremental(operations: SyncOperation[]): Promise<SyncResult>`
  - For renames: call `api.renameFile()` directly
  - For creates/modifies: upload file
  - For deletes: call `api.deleteFile()` (tombstone)
  - After processing, refresh state from manifest

- `detectRenames(localFiles: Map<string, string>): Promise<Array<{oldPath: string, newPath: string}>>`
  - Fallback rename detection: compare local file hashes against state
  - If a hash in state exists locally under a different path, it's a rename

- `handleRemoteRename(oldPath: string, newPath: string, remote: VaultManifestEntry): Promise<void>`
  - Download file to new path, delete old path, update state

**Enhanced `sync()` method:**
- Before upload, check if file path is in state.tombstones — skip if tombstoned
- After detecting remote tombstone for locally-changed file: create conflict copy, NOT delete
- Handle all state table cases correctly

### 4. `src/state.ts` — Tombstone Tracking

```typescript
export interface TombstoneState {
  deleted_at: string;
  server_rev: number;
}

export interface SyncState {
  vault_id: string;
  device_id: string;
  device_name: string;
  last_server_rev: number;
  files: Record<string, LocalFileState>;
  tombstones: Record<string, TombstoneState>; // NEW
}
```

- `createEmptySyncState()` returns empty tombstones
- Migration: if loaded state has no tombstones, initialize to `{}`

### 5. `src/settings.ts` — New Configuration Options

```typescript
export interface RustShareVaultSyncSettings {
  rustshareUrl: string;
  authToken: string;
  deviceId: string;
  deviceName: string;
  vaultId: string;
  autoSyncIntervalMinutes: number;
  conflictStrategy: 'create_copy';
  // NEW:
  debounceMs: number;
  maxRetries: number;
  retryBaseDelayMs: number;
  logSyncEvents: boolean;
}
```

Defaults:
- `debounceMs: 1500`
- `maxRetries: 5`
- `retryBaseDelayMs: 2000`
- `logSyncEvents: false` (opt-in for performance)

### 6. `src/main.ts` — Event Listener Wiring

In `onload()`:
```typescript
// Register file event listeners
this.registerEvent(this.app.vault.on('create', (file) => {
  if (file instanceof TFile && !shouldIgnorePath(file.path)) {
    this.syncQueue.add({ path: file.path, type: 'create' });
  }
}));

this.registerEvent(this.app.vault.on('delete', (file) => {
  if (file instanceof TFile && !shouldIgnorePath(file.path)) {
    this.syncQueue.add({ path: file.path, type: 'delete' });
  }
}));

this.registerEvent(this.app.vault.on('rename', (file, oldPath) => {
  if (file instanceof TFile && !shouldIgnorePath(file.path)) {
    this.syncQueue.add({ path: file.path, type: 'rename', oldPath });
  }
}));

this.registerEvent(this.app.metadataCache.on('changed', (file) => {
  if (file instanceof TFile && !shouldIgnorePath(file.path)) {
    this.syncQueue.add({ path: file.path, type: 'modify' });
  }
}));
```

Note: `metadataCache.on('changed')` fires for file modifications. We don't need a separate modify event on vault.

### 7. `src/api.ts` — Minor Enhancements

- `renameFile` already accepts `RenameRequest` with headers — verify it sends the new headers correctly
- Conflict error parsing: ensure `client_rev` from server is parsed as `client_base_server_rev`

## State Table Implementation

| Local State | Remote State | Action |
|---|---|---|
| unchanged | unchanged | do nothing |
| changed | unchanged | upload |
| unchanged | changed | download |
| changed | changed | conflict |
| deleted | unchanged | upload tombstone |
| unchanged | deleted | delete locally OR move to trash |
| deleted | changed | conflict (preserve local delete = conflict?) |
| changed | deleted | conflict |

For "deleted + changed" and "changed + deleted": create conflict copy.

## Quality Gates
- [ ] All new code has TypeScript strict typing
- [ ] No `any` types except where absolutely necessary
- [ ] All async errors caught and logged
- [ ] Debounce prevents upload storms
- [ ] Offline queue persists across retries
- [ ] Tombstones prevent re-upload of deleted files
- [ ] Renames use server endpoint, not delete+create
- [ ] Plugin builds with 0 errors
