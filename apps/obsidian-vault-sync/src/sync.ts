// Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.
// This file is part of RustShare Vault Sync.

import { Vault, TFile, normalizePath } from 'obsidian';
import { RustShareAPI, VaultManifestEntry } from './api';
import { SyncState } from './state';
import { sha256ArrayBuffer, formatConflictFileName, shouldIgnorePath } from './utils';
import { SyncOperation, SyncOperationType } from './sync-queue';
import { syncLog } from './sync-log';

export interface SyncResult {
  uploaded: number;
  downloaded: number;
  deleted: number;
  conflicts: number;
  errors: string[];
}

class UploadConflictError extends Error {
  constructor(
    public path: string,
    public serverSha256?: string,
    public currentRev?: number,
  ) {
    super(`Conflict uploading ${path}`);
  }
}

export class SyncEngine {
  constructor(
    private vault: Vault,
    private api: RustShareAPI,
    private state: SyncState,
    private deviceName: string,
  ) {}

  async sync(): Promise<SyncResult> {
    // 1. Scan local files
    const localFiles = await this.scanLocalFiles();

    // 2. Fetch manifest
    const manifest = await this.api.getManifest(this.state.vault_id);
    this.state.last_server_rev = manifest.server_rev;

    // 3. Build remote file map
    const remoteFiles = new Map<string, VaultManifestEntry>();
    for (const entry of manifest.files) {
      remoteFiles.set(entry.path, entry);
    }

    // 4. Compare and sync
    const result: SyncResult = { uploaded: 0, downloaded: 0, deleted: 0, conflicts: 0, errors: [] };

    // Rename detection: find files that moved but kept their hash
    const detectedRenames = this.detectHashMatchRenames(localFiles);
    for (const rename of detectedRenames) {
      const remoteOld = remoteFiles.get(rename.oldPath);
      let renamed = false;
      if (remoteOld && !remoteOld.deleted) {
        try {
          await this.api.renameFile(this.state.vault_id, {
            old_path: rename.oldPath,
            new_path: rename.newPath,
            base_server_rev: remoteOld.server_rev,
            device_id: this.state.device_id,
          });
          remoteFiles.delete(rename.oldPath);
          renamed = true;
        } catch (e) {
          result.errors.push(`Rename failed: ${rename.oldPath} -> ${rename.newPath}: ${e}`);
          continue;
        }
      } else {
        syncLog.info(`No active remote entry for rename old path: ${rename.oldPath}`);
        delete this.state.files[rename.oldPath];
      }

      if (renamed && this.state.files[rename.oldPath]) {
        this.state.files[rename.newPath] = this.state.files[rename.oldPath];
        delete this.state.files[rename.oldPath];
        localFiles.delete(rename.oldPath);
      }
    }

    // Upload local-only or changed files
    for (const [path, localHash] of localFiles) {
      // Skip tombstoned files to avoid re-uploading user-deleted content
      if (this.state.tombstones[path]) {
        syncLog.info(`Skipping upload of tombstoned file: ${path}`);
        continue;
      }

      const remote = remoteFiles.get(path);
      if (!remote) {
        // New file — upload
        try {
          const resp = await this.uploadFile(path, 0);
          // resp.server_rev is authoritative
          result.uploaded++;
        } catch (e) {
          if (e instanceof UploadConflictError) {
            try {
              await this.handleUploadConflict(path, e.currentRev ?? manifest.server_rev, e.serverSha256);
              result.conflicts++;
            } catch (conflictErr) {
              result.errors.push(`Conflict resolution failed: ${path}: ${conflictErr}`);
            }
          } else {
            result.errors.push(`Upload failed: ${path}: ${e}`);
          }
        }
      } else if (remote.deleted) {
        // Remote deleted — conflict if local changed
        const localState = this.state.files[path];
        if (localState && localState.sha256 === localHash) {
          // Local unchanged — delete locally
          try {
            await this.deleteLocalFile(path, remote.server_rev);
            result.deleted++;
          } catch (e) {
            result.errors.push(`Delete failed: ${path}: ${e}`);
          }
        } else {
          // Local changed — conflict
          try {
            await this.handleUploadConflict(path, remote.server_rev, remote.sha256, localHash);
            result.conflicts++;
          } catch (e) {
            result.errors.push(`Conflict resolution failed: ${path}: ${e}`);
          }
        }
      } else if (remote.sha256 === localHash) {
        this.recordSyncedFile(path, localHash, remote.server_rev);
      } else if (remote.sha256 !== localHash) {
        // Both exist, hashes differ
        const localState = this.state.files[path];
        if (!localState) {
          // Fresh/empty sync state — don't destroy local data, treat as conflict
          try {
            await this.handleUploadConflict(path, remote.server_rev, remote.sha256, localHash);
            result.conflicts++;
          } catch (e) {
            result.errors.push(`Conflict resolution failed: ${path}: ${e}`);
          }
        } else if (localState.sha256 === localHash) {
          // Local unchanged since last sync — download remote
          try {
            await this.downloadFile(path, remote);
            result.downloaded++;
          } catch (e) {
            result.errors.push(`Download failed: ${path}: ${e}`);
          }
        } else if (remote.sha256 === localState.sha256) {
          // Local changed, remote unchanged — upload
          try {
            const resp = await this.uploadFile(path, remote.server_rev);
            // resp.server_rev is authoritative
            result.uploaded++;
          } catch (e) {
            if (e instanceof UploadConflictError) {
              try {
                await this.handleUploadConflict(path, e.currentRev ?? manifest.server_rev, e.serverSha256);
                result.conflicts++;
              } catch (conflictErr) {
                result.errors.push(`Conflict resolution failed: ${path}: ${conflictErr}`);
              }
            } else {
              result.errors.push(`Upload failed: ${path}: ${e}`);
            }
          }
        } else {
          // Both changed — conflict
          try {
            await this.handleUploadConflict(path, remote.server_rev, remote.sha256, localHash);
            result.conflicts++;
          } catch (e) {
            result.errors.push(`Conflict resolution failed: ${path}: ${e}`);
          }
        }
      }
    }

    // Download remote-only files, propagate local deletions, or clean up remote tombstones
    for (const [path, remote] of remoteFiles) {
      if (remote.deleted) {
        if (!localFiles.has(path) && this.state.files[path]) {
          // File was previously synced but is now deleted remotely and locally gone.
          // Clean up state and record tombstone.
          const capturedRev = this.state.files[path].server_rev;
          delete this.state.files[path];
          this.state.tombstones[path] = { deleted_at: new Date().toISOString(), server_rev: capturedRev };
        }
        continue;
      }

      if (!localFiles.has(path)) {
        if (this.state.files[path]) {
          // File was previously synced but is now gone locally → user deleted it
          try {
            await this.api.deleteFile(this.state.vault_id, path, remote.server_rev, this.state.device_id);
            result.deleted++;
            delete this.state.files[path];
            this.state.tombstones[path] = { deleted_at: new Date().toISOString(), server_rev: remote.server_rev + 1 };
          } catch (e) {
            result.errors.push(`Delete failed: ${path}: ${e}`);
          }
        } else {
          // File exists remotely but never synced locally → download
          try {
            await this.downloadFile(path, remote);
            result.downloaded++;
          } catch (e) {
            result.errors.push(`Download failed: ${path}: ${e}`);
          }
        }
      }
    }

    return result;
  }

  async syncIncremental(operations: SyncOperation[]): Promise<SyncResult> {
    const result: SyncResult = { uploaded: 0, downloaded: 0, deleted: 0, conflicts: 0, errors: [] };

    const manifest = await this.api.getManifest(this.state.vault_id);
    this.state.last_server_rev = manifest.server_rev;

    const remoteFiles = new Map<string, VaultManifestEntry>();
    for (const entry of manifest.files) {
      remoteFiles.set(entry.path, entry);
    }

    const tombstones = this.state.tombstones;

    for (const op of operations) {
      try {
        switch (op.type) {
          case 'rename': {
            if (!op.oldPath) {
              syncLog.warn(`Rename operation missing oldPath for ${op.path}`);
              continue;
            }
            const remoteOld = remoteFiles.get(op.oldPath);
            if (remoteOld && !remoteOld.deleted) {
              await this.api.renameFile(this.state.vault_id, {
                old_path: op.oldPath,
                new_path: op.path,
                base_server_rev: remoteOld.server_rev,
                device_id: this.state.device_id,
              });
              if (this.state.files[op.oldPath]) {
                this.state.files[op.path] = this.state.files[op.oldPath];
                delete this.state.files[op.oldPath];
              }
            } else {
              syncLog.info(`No active remote entry for rename old path: ${op.oldPath}`);
              const file = this.vault.getAbstractFileByPath(normalizePath(op.path));
              if (!(file instanceof TFile)) {
                syncLog.warn(`File not found for rename fallback upload: ${op.path}`);
                continue;
              }
              const buffer = await this.vault.readBinary(file);
              const hash = await sha256ArrayBuffer(buffer);
              const remote = remoteFiles.get(op.path);

              if (remote?.deleted) {
                await this.handleUploadConflict(op.path, remote.server_rev, remote.sha256, hash);
                result.conflicts++;
              } else {
                try {
                  await this.uploadFile(op.path, remote?.server_rev ?? 0);
                  delete this.state.files[op.oldPath];
                  result.uploaded++;
                } catch (e) {
                  if (e instanceof UploadConflictError) {
                    await this.handleUploadConflict(op.path, e.currentRev ?? manifest.server_rev, e.serverSha256);
                    result.conflicts++;
                  } else {
                    throw e;
                  }
                }
              }
            }
            break;
          }
          case 'delete': {
            const remote = remoteFiles.get(op.path);
            if (remote && !remote.deleted) {
              await this.api.deleteFile(this.state.vault_id, op.path, remote.server_rev, this.state.device_id);
              tombstones[op.path] = { deleted_at: new Date().toISOString(), server_rev: remote.server_rev + 1 };
            } else if (remote && remote.deleted) {
              syncLog.info(`Remote already deleted: ${op.path}`);
            }
            delete this.state.files[op.path];
            break;
          }
          case 'create':
          case 'modify': {
            if (tombstones[op.path]) {
              syncLog.info(`Skipping upload of tombstoned file: ${op.path}`);
              continue;
            }
            const file = this.vault.getAbstractFileByPath(normalizePath(op.path));
            if (!(file instanceof TFile)) {
              syncLog.warn(`File not found for ${op.type}: ${op.path}`);
              continue;
            }
            const buffer = await this.vault.readBinary(file);
            const hash = await sha256ArrayBuffer(buffer);
            const remote = remoteFiles.get(op.path);

            if (!remote) {
              const resp = await this.api.uploadFile(this.state.vault_id, op.path, buffer, hash, 0, this.state.device_id);
              this.state.files[op.path] = {
                sha256: hash,
                server_rev: resp.server_rev,
                last_synced_at: new Date().toISOString(),
              };
              result.uploaded++;
            } else if (remote.deleted) {
              // changed + deleted = conflict
              syncLog.info(`Conflict: local ${op.type} vs remote delete for ${op.path}`);
              await this.handleUploadConflict(op.path, remote.server_rev, remote.sha256, hash);
              result.conflicts++;
            } else {
              try {
                const resp = await this.api.uploadFile(this.state.vault_id, op.path, buffer, hash, remote.server_rev, this.state.device_id);
                this.state.files[op.path] = {
                  sha256: hash,
                  server_rev: resp.server_rev,
                  last_synced_at: new Date().toISOString(),
                };
                result.uploaded++;
              } catch (e: any) {
                if (e.error === 'conflict') {
                  const serverSha256 = e.server_sha256;
                  try {
                    await this.handleUploadConflict(op.path, e.current_rev ?? remote.server_rev, serverSha256);
                    result.conflicts++;
                  } catch (conflictErr) {
                    result.errors.push(`Conflict resolution failed: ${op.path}: ${conflictErr}`);
                  }
                } else {
                  result.errors.push(`Upload failed: ${op.path}: ${e}`);
                }
              }
            }
            break;
          }
        }
      } catch (e) {
        result.errors.push(`Operation failed (${op.type} ${op.path}): ${e}`);
      }
    }

    return result;
  }

  private detectHashMatchRenames(localFiles: Map<string, string>): Array<{ oldPath: string; newPath: string }> {
    const renames: Array<{ oldPath: string; newPath: string }> = [];
    const usedLocalPaths = new Set<string>();
    for (const [statePath, stateEntry] of Object.entries(this.state.files)) {
      if (localFiles.has(statePath)) {
        continue;
      }
      for (const [localPath, localHash] of localFiles) {
        if (usedLocalPaths.has(localPath)) continue;
        if (localPath !== statePath && stateEntry.sha256 === localHash) {
          renames.push({ oldPath: statePath, newPath: localPath });
          usedLocalPaths.add(localPath);
          break;
        }
      }
    }
    return renames;
  }

  private async scanLocalFiles(): Promise<Map<string, string>> {
    const files = new Map<string, string>();
    const allFiles = this.vault.getFiles();
    const batchSize = 50;
    for (let i = 0; i < allFiles.length; i += batchSize) {
      const batch = allFiles.slice(i, i + batchSize);
      await Promise.all(batch.map(async (file) => {
        const path = file.path;
        if (shouldIgnorePath(path)) return;
        if (file instanceof TFile) {
          const buffer = await this.vault.readBinary(file);
          const hash = await sha256ArrayBuffer(buffer);
          files.set(path, hash);
        }
      }));
      // Yield to event loop
      await new Promise(resolve => setTimeout(resolve, 0));
    }
    return files;
  }

  private async uploadFile(path: string, baseServerRev: number): Promise<{ server_rev: number }> {
    const file = this.vault.getAbstractFileByPath(normalizePath(path));
    if (!(file instanceof TFile)) {
      throw new Error(`File not found for upload: ${path}`);
    }
    const buffer = await this.vault.readBinary(file);
    const hash = await sha256ArrayBuffer(buffer);
    let resp: { server_rev: number };
    try {
      resp = await this.api.uploadFile(this.state.vault_id, path, buffer, hash, baseServerRev, this.state.device_id);
    } catch (e: any) {
      if (e.error === 'conflict') {
        throw new UploadConflictError(path, e.server_sha256, e.current_rev);
      }
      throw e;
    }
    this.recordSyncedFile(path, hash, resp.server_rev);
    return resp;
  }

  private async downloadFile(path: string, remote: VaultManifestEntry): Promise<void> {
    const data = await this.api.downloadFile(this.state.vault_id, path);

    // Verify downloaded content integrity
    if (remote.sha256) {
      const computedHash = await sha256ArrayBuffer(data);
      if (computedHash !== remote.sha256) {
        throw new Error(`SHA-256 mismatch for ${path}: expected ${remote.sha256}, got ${computedHash}`);
      }
    }

    const normalizedPath = normalizePath(path);
    // Ensure parent directories exist
    const lastSlash = normalizedPath.lastIndexOf('/');
    if (lastSlash > 0) {
      const dir = normalizedPath.slice(0, lastSlash);
      await this.vault.adapter.mkdir(normalizePath(dir));
    }
    await this.vault.adapter.writeBinary(normalizedPath, data);
    this.recordSyncedFile(path, remote.sha256 ?? await sha256ArrayBuffer(data), remote.server_rev);
  }

  private recordSyncedFile(path: string, sha256: string, serverRev: number): void {
    this.state.files[path] = {
      sha256,
      server_rev: serverRev,
      last_synced_at: new Date().toISOString(),
    };
  }

  private async deleteLocalFile(path: string, remoteServerRev?: number): Promise<void> {
    const file = this.vault.getAbstractFileByPath(normalizePath(path));
    if (!(file instanceof TFile)) return;
    await this.vault.delete(file);
    delete this.state.files[path];
    if (remoteServerRev !== undefined) {
      this.state.tombstones[path] = { deleted_at: new Date().toISOString(), server_rev: remoteServerRev };
    }
  }

  private async createConflictCopy(path: string): Promise<void> {
    const file = this.vault.getAbstractFileByPath(normalizePath(path));
    if (!(file instanceof TFile)) return;
    let conflictName = formatConflictFileName(path, this.deviceName, new Date());
    let counter = 1;
    while (this.vault.getAbstractFileByPath(normalizePath(conflictName))) {
      const lastDot = conflictName.lastIndexOf('.');
      const ext = lastDot > 0 ? conflictName.slice(lastDot) : '';
      const base = lastDot > 0 ? conflictName.slice(0, lastDot) : conflictName;
      conflictName = `${base} (${counter})${ext}`;
      counter++;
    }
    await this.vault.copy(file, normalizePath(conflictName));
  }

  private async handleUploadConflict(path: string, remoteRev: number, remoteSha256?: string, localHash?: string): Promise<void> {
    await this.createConflictCopy(path);
    if (remoteSha256) {
      const data = await this.api.downloadFile(this.state.vault_id, path);
      const normalizedPath = normalizePath(path);
      const lastSlash = normalizedPath.lastIndexOf('/');
      if (lastSlash > 0) {
        const dir = normalizedPath.slice(0, lastSlash);
        await this.vault.adapter.mkdir(normalizePath(dir));
      }
      await this.vault.adapter.writeBinary(normalizedPath, data);
      const hash = remoteSha256 ?? await sha256ArrayBuffer(data);
      this.state.files[path] = {
        sha256: hash,
        server_rev: remoteRev,
        last_synced_at: new Date().toISOString(),
      };
    } else {
      // Server file is tombstoned; our local version wins as conflict copy
      // The original path can be re-uploaded on next sync
      console.log(`Server file ${path} is deleted; local version saved as conflict copy`);
      // Update state so we don't re-conflict on next sync
      if (localHash) {
        this.state.files[path] = {
          sha256: localHash,
          server_rev: remoteRev,
          last_synced_at: new Date().toISOString(),
        };
      }
    }
  }

}
