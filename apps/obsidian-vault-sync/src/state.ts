// Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.
// This file is part of RustShare Vault Sync.

export interface LocalFileState {
  sha256: string;
  server_rev: number;
  last_synced_at: string;
}

export interface TombstoneState {
  deleted_at: string;
  server_rev: number;
}

export interface SyncState {
  version: number;
  vault_id: string;
  device_id: string;
  device_name: string;
  last_server_rev: number;
  files: Record<string, LocalFileState>;
  tombstones: Record<string, TombstoneState>;
}

export function createEmptySyncState(vaultId: string, deviceId: string, deviceName: string): SyncState {
  return {
    version: 1,
    vault_id: vaultId,
    device_id: deviceId,
    device_name: deviceName,
    last_server_rev: 0,
    files: {},
    tombstones: {},
  };
}

export function migrateSyncState(state: Partial<SyncState> & Record<string, any>): SyncState {
  const version = state.version ?? 0;
  if (version === 1) {
    // Current version, return as-is with defaults
    return {
      version: 1,
      vault_id: state.vault_id || '',
      device_id: state.device_id || '',
      device_name: state.device_name || '',
      last_server_rev: state.last_server_rev ?? 0,
      files: state.files || {},
      tombstones: state.tombstones || {},
    };
  }
  // Future migrations go here
  return createEmptySyncState(state.vault_id || '', state.device_id || '', state.device_name || '');
}

const TOMBSTONE_RETENTION_DAYS = 30;

export function pruneTombstones(state: SyncState): void {
  const cutoff = new Date();
  cutoff.setDate(cutoff.getDate() - TOMBSTONE_RETENTION_DAYS);
  const cutoffIso = cutoff.toISOString();

  for (const [path, tombstone] of Object.entries(state.tombstones)) {
    if (tombstone.deleted_at < cutoffIso) {
      delete state.tombstones[path];
    }
  }
}
