import { describe, it, expect, beforeEach } from 'vitest';
import { Vault, TFile } from './mocks/obsidian';
import { SyncEngine } from '../src/sync';
import { createEmptySyncState, SyncState } from '../src/state';
import { VaultManifest, VaultManifestEntry } from '../src/api';
import { sha256ArrayBuffer } from '../src/utils';

function makeBuffer(text: string): ArrayBuffer {
  return new TextEncoder().encode(text).buffer;
}

async function hash(text: string): Promise<string> {
  return sha256ArrayBuffer(makeBuffer(text));
}

async function readText(vault: Vault, path: string): Promise<string> {
  const file = vault.getAbstractFileByPath(path);
  if (!(file instanceof TFile)) return '';
  const buffer = await vault.readBinary(file);
  return new TextDecoder().decode(buffer);
}

function makeManifestEntry(
  path: string,
  overrides: Partial<VaultManifestEntry> = {}
): VaultManifestEntry {
  return {
    path,
    sha256: overrides.sha256 ?? '',
    size: overrides.size ?? 0,
    server_rev: overrides.server_rev ?? 1,
    mtime_server: overrides.mtime_server ?? new Date().toISOString(),
    deleted: overrides.deleted ?? false,
    ...overrides,
  };
}

class MockAPI {
  uploads: Array<{ path: string; rev: number }> = [];
  downloads: Array<{ path: string }> = [];
  deletes: Array<{ path: string; rev: number }> = [];
  renames: Array<{ oldPath: string; newPath: string }> = [];
  manifest: VaultManifest = {
    vault_id: 'test-vault',
    adapter: 'ObsidianVault',
    server_rev: 0,
    generated_at: new Date().toISOString(),
    files: [],
  };

  conflictPaths: Set<string> = new Set();
  conflictResponse: Record<string, unknown> = {
    error: 'conflict',
    current_rev: 2,
    server_sha256: 'abc',
  };

  async uploadFile(
    _vaultId: string,
    path: string,
    _content: ArrayBuffer,
    _sha256: string,
    baseServerRev: number,
    _deviceId: string
  ) {
    if (this.conflictPaths.has(path)) {
      const resp =
        typeof this.conflictResponse === 'function'
          ? (this.conflictResponse as any)(path, baseServerRev)
          : { ...this.conflictResponse };
      throw resp;
    }
    this.uploads.push({ path, rev: baseServerRev });
    return { server_rev: baseServerRev + 1 };
  }

  async downloadFile(_vaultId: string, path: string): Promise<ArrayBuffer> {
    this.downloads.push({ path });
    return makeBuffer('remote content');
  }

  async deleteFile(
    _vaultId: string,
    path: string,
    baseServerRev: number,
    _deviceId: string
  ) {
    this.deletes.push({ path, rev: baseServerRev });
  }

  async renameFile(
    _vaultId: string,
    req: { old_path: string; new_path: string; base_server_rev: number; device_id: string }
  ) {
    this.renames.push({ oldPath: req.old_path, newPath: req.new_path });
  }

  async getManifest(_vaultId: string): Promise<VaultManifest> {
    return {
      ...this.manifest,
      generated_at: new Date().toISOString(),
    };
  }
}

describe('SyncEngine', () => {
  let vault: Vault;
  let api: MockAPI;
  let state: SyncState;
  let engine: SyncEngine;

  beforeEach(() => {
    vault = new Vault();
    api = new MockAPI();
    state = createEmptySyncState('test-vault', 'device-123', 'test-device');
    engine = new SyncEngine(vault, api as any, state, 'test-device');
  });

  describe('full sync', () => {
    it('uploads new file when local file exists and remote manifest is empty', async () => {
      vault.addFile('notes/hello.md', 'hello world');
      const helloHash = await hash('hello world');

      const result = await engine.sync();

      expect(result.uploaded).toBe(1);
      expect(api.uploads).toHaveLength(1);
      expect(api.uploads[0]).toEqual({ path: 'notes/hello.md', rev: 0 });
      expect(state.files['notes/hello.md']).toBeDefined();
      expect(state.files['notes/hello.md'].sha256).toBe(helloHash);
      expect(state.files['notes/hello.md'].server_rev).toBe(1);
    });

    it('downloads remote-only file when local vault is empty', async () => {
      const remoteHash = await hash('remote content');
      api.manifest.files.push(
        makeManifestEntry('notes/remote.md', {
          sha256: remoteHash,
          server_rev: 1,
        })
      );

      const result = await engine.sync();

      expect(result.downloaded).toBe(1);
      expect(api.downloads).toHaveLength(1);
      expect(api.downloads[0]).toEqual({ path: 'notes/remote.md' });
      expect(state.files['notes/remote.md']).toBeDefined();
      expect(state.files['notes/remote.md'].sha256).toBe(remoteHash);
      expect(state.files['notes/remote.md'].server_rev).toBe(1);
      expect(vault.getAbstractFileByPath('notes/remote.md')).toBeInstanceOf(TFile);
    });

    it('propagates local delete to server and records tombstone', async () => {
      state.files['notes/old.md'] = {
        sha256: await hash('old content'),
        server_rev: 1,
        last_synced_at: new Date().toISOString(),
      };
      api.manifest.files.push(
        makeManifestEntry('notes/old.md', { server_rev: 1 })
      );

      const result = await engine.sync();

      expect(result.deleted).toBe(1);
      expect(api.deletes).toHaveLength(1);
      expect(api.deletes[0]).toEqual({ path: 'notes/old.md', rev: 1 });
      expect(state.files['notes/old.md']).toBeUndefined();
      expect(state.tombstones['notes/old.md']).toBeDefined();
      expect(state.tombstones['notes/old.md'].server_rev).toBe(2);
    });

    it('deletes local file when remote deleted and local unchanged', async () => {
      vault.addFile('notes/gone.md', 'same content');
      const contentHash = await hash('same content');
      state.files['notes/gone.md'] = {
        sha256: contentHash,
        server_rev: 1,
        last_synced_at: new Date().toISOString(),
      };
      api.manifest.files.push(
        makeManifestEntry('notes/gone.md', { server_rev: 2, deleted: true })
      );

      const result = await engine.sync();

      expect(result.deleted).toBe(1);
      expect(vault.getAbstractFileByPath('notes/gone.md')).toBeNull();
      expect(state.files['notes/gone.md']).toBeUndefined();
      expect(state.tombstones['notes/gone.md']).toBeDefined();
      expect(state.tombstones['notes/gone.md'].server_rev).toBe(2);
    });

    it('creates conflict copy when remote deleted but local changed', async () => {
      vault.addFile('notes/gone.md', 'new content');
      state.files['notes/gone.md'] = {
        sha256: await hash('old content'),
        server_rev: 1,
        last_synced_at: new Date().toISOString(),
      };
      api.manifest.files.push(
        makeManifestEntry('notes/gone.md', { server_rev: 2, deleted: true })
      );

      const result = await engine.sync();

      expect(result.conflicts).toBe(1);
      // Original should be preserved
      expect(vault.getAbstractFileByPath('notes/gone.md')).toBeInstanceOf(TFile);
      // Conflict copy should exist
      const files = vault.getFiles();
      expect(files.length).toBe(2);
      const conflictFile = files.find((f) => f.path.includes('conflicted copy'));
      expect(conflictFile).toBeDefined();
    });

    it('creates conflict copy when both local and remote changed', async () => {
      vault.addFile('notes/both.md', 'local B');
      state.files['notes/both.md'] = {
        sha256: await hash('state A'),
        server_rev: 1,
        last_synced_at: new Date().toISOString(),
      };
      api.manifest.files.push(
        makeManifestEntry('notes/both.md', {
          sha256: await hash('remote C'),
          server_rev: 2,
        })
      );

      const result = await engine.sync();

      expect(result.conflicts).toBe(1);
      const files = vault.getFiles();
      expect(files.length).toBe(2);
      const conflictFile = files.find((f) => f.path.includes('conflicted copy'));
      expect(conflictFile).toBeDefined();
    });

    it('detects rename by hash match and calls rename API', async () => {
      const content = 'content X';
      const contentHash = await hash(content);
      vault.addFile('notes/new.md', content);
      state.files['notes/old.md'] = {
        sha256: contentHash,
        server_rev: 1,
        last_synced_at: new Date().toISOString(),
      };
      api.manifest.files.push(
        makeManifestEntry('notes/old.md', { server_rev: 1 })
      );

      const result = await engine.sync();

      expect(api.renames.length).toBeGreaterThanOrEqual(1);
      expect(api.renames).toContainEqual({
        oldPath: 'notes/old.md',
        newPath: 'notes/new.md',
      });
      expect(state.files['notes/old.md']).toBeUndefined();
      expect(state.files['notes/new.md']).toBeDefined();
      expect(state.files['notes/new.md'].sha256).toBe(contentHash);
    });

    it('uploads detected rename when remote old path is missing', async () => {
      const content = 'content X';
      const contentHash = await hash(content);
      vault.addFile('notes/new.md', content);
      state.files['notes/old.md'] = {
        sha256: contentHash,
        server_rev: 1,
        last_synced_at: new Date().toISOString(),
      };

      const result = await engine.sync();

      expect(result.uploaded).toBe(1);
      expect(api.renames).toHaveLength(0);
      expect(api.uploads).toEqual([{ path: 'notes/new.md', rev: 0 }]);
      expect(state.files['notes/old.md']).toBeUndefined();
      expect(state.files['notes/new.md']).toBeDefined();
      expect(state.files['notes/new.md'].sha256).toBe(contentHash);
    });

    it('records detected rename state when remote new path already has same content', async () => {
      const content = 'content X';
      const contentHash = await hash(content);
      vault.addFile('notes/new.md', content);
      state.files['notes/old.md'] = {
        sha256: contentHash,
        server_rev: 1,
        last_synced_at: new Date().toISOString(),
      };
      api.manifest.files.push(
        makeManifestEntry('notes/new.md', {
          sha256: contentHash,
          server_rev: 4,
        })
      );

      const result = await engine.sync();

      expect(result.uploaded).toBe(0);
      expect(result.downloaded).toBe(0);
      expect(result.conflicts).toBe(0);
      expect(api.renames).toHaveLength(0);
      expect(api.uploads).toHaveLength(0);
      expect(state.files['notes/old.md']).toBeUndefined();
      expect(state.files['notes/new.md']).toBeDefined();
      expect(state.files['notes/new.md'].sha256).toBe(contentHash);
      expect(state.files['notes/new.md'].server_rev).toBe(4);
    });
  });

  describe('incremental sync', () => {
    it('creates and uploads new file with baseServerRev=0', async () => {
      vault.addFile('notes/new.md', 'new content');

      const result = await engine.syncIncremental([
        { path: 'notes/new.md', type: 'create' },
      ]);

      expect(result.uploaded).toBe(1);
      expect(api.uploads).toHaveLength(1);
      expect(api.uploads[0]).toEqual({ path: 'notes/new.md', rev: 0 });
      expect(state.files['notes/new.md']).toBeDefined();
      expect(state.files['notes/new.md'].server_rev).toBe(1);
    });

    it('handles modify conflict by creating conflict copy and downloading remote', async () => {
      vault.addFile('notes/edit.md', 'local new');
      state.files['notes/edit.md'] = {
        sha256: await hash('local old'),
        server_rev: 1,
        last_synced_at: new Date().toISOString(),
      };
      api.manifest.files.push(
        makeManifestEntry('notes/edit.md', {
          sha256: await hash('remote content'),
          server_rev: 2,
        })
      );
      api.conflictPaths.add('notes/edit.md');
      api.conflictResponse = {
        error: 'conflict',
        current_rev: 2,
        server_sha256: 'abc',
      };

      const result = await engine.syncIncremental([
        { path: 'notes/edit.md', type: 'modify' },
      ]);

      expect(result.conflicts).toBe(1);
      expect(api.uploads).toHaveLength(0); // upload threw conflict, not recorded
      expect(api.downloads).toHaveLength(1);

      const files = vault.getFiles();
      expect(files.length).toBe(2);

      // Original path should now have remote content
      const originalText = await readText(vault, 'notes/edit.md');
      expect(originalText).toBe('remote content');

      // Conflict copy should have local content
      const conflictFile = files.find((f) => f.path.includes('conflicted copy'));
      expect(conflictFile).toBeDefined();
      const conflictText = await readText(vault, conflictFile!.path);
      expect(conflictText).toBe('local new');

      expect(state.files['notes/edit.md'].sha256).toBe('abc');
      expect(state.files['notes/edit.md'].server_rev).toBe(2);
    });

    it('deletes file and records tombstone', async () => {
      state.files['notes/del.md'] = {
        sha256: await hash('del'),
        server_rev: 1,
        last_synced_at: new Date().toISOString(),
      };
      api.manifest.files.push(
        makeManifestEntry('notes/del.md', { server_rev: 1 })
      );

      const result = await engine.syncIncremental([
        { path: 'notes/del.md', type: 'delete' },
      ]);

      expect(api.deletes).toHaveLength(1);
      expect(api.deletes[0]).toEqual({ path: 'notes/del.md', rev: 1 });
      expect(state.files['notes/del.md']).toBeUndefined();
      expect(state.tombstones['notes/del.md']).toBeDefined();
      expect(state.tombstones['notes/del.md'].server_rev).toBe(2);
    });

    it('renames file and moves state entry', async () => {
      state.files['notes/src.md'] = {
        sha256: await hash('src'),
        server_rev: 1,
        last_synced_at: new Date().toISOString(),
      };
      api.manifest.files.push(
        makeManifestEntry('notes/src.md', { server_rev: 1 })
      );

      const result = await engine.syncIncremental([
        { path: 'notes/dst.md', type: 'rename', oldPath: 'notes/src.md' },
      ]);

      expect(api.renames).toHaveLength(1);
      expect(api.renames[0]).toEqual({
        oldPath: 'notes/src.md',
        newPath: 'notes/dst.md',
      });
      expect(state.files['notes/src.md']).toBeUndefined();
      expect(state.files['notes/dst.md']).toBeDefined();
      expect(state.files['notes/dst.md'].sha256).toBe(await hash('src'));
    });

    it('uploads renamed file when remote old path is missing', async () => {
      vault.addFile('notes/dst.md', 'src');
      state.files['notes/src.md'] = {
        sha256: await hash('src'),
        server_rev: 1,
        last_synced_at: new Date().toISOString(),
      };

      const result = await engine.syncIncremental([
        { path: 'notes/dst.md', type: 'rename', oldPath: 'notes/src.md' },
      ]);

      expect(result.uploaded).toBe(1);
      expect(api.renames).toHaveLength(0);
      expect(api.uploads).toEqual([{ path: 'notes/dst.md', rev: 0 }]);
      expect(state.files['notes/src.md']).toBeUndefined();
      expect(state.files['notes/dst.md']).toBeDefined();
      expect(state.files['notes/dst.md'].sha256).toBe(await hash('src'));
    });
  });

  describe('tombstone behavior', () => {
    it('prevents re-upload of a file with a tombstone', async () => {
      state.tombstones['notes/dead.md'] = {
        deleted_at: new Date().toISOString(),
        server_rev: 1,
      };
      vault.addFile('notes/dead.md', 'recreated');

      const result = await engine.sync();

      expect(result.uploaded).toBe(0);
      expect(api.uploads).toHaveLength(0);
      expect(state.files['notes/dead.md']).toBeUndefined();
    });
  });
});
