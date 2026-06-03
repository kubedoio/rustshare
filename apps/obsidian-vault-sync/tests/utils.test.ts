import { describe, it, expect, vi } from 'vitest';
import {
  sha256ArrayBuffer,
  generateDeviceId,
  formatConflictFileName,
  shouldIgnorePath,
  detectCloudSyncFolder,
} from '../src/utils';

describe('sha256ArrayBuffer', () => {
  it('returns correct hex hash for known input', async () => {
    const encoder = new TextEncoder();
    const data = encoder.encode('hello').buffer;
    const hash = await sha256ArrayBuffer(data);
    // Known SHA-256 of "hello"
    expect(hash).toBe(
      '2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824'
    );
  });
});

describe('generateDeviceId', () => {
  it('returns a 32-character hex string', () => {
    const id = generateDeviceId();
    expect(id).toMatch(/^[0-9a-f]{32}$/);
  });

  it('returns different values on successive calls', () => {
    const id1 = generateDeviceId();
    const id2 = generateDeviceId();
    expect(id1).not.toBe(id2);
  });
});

describe('formatConflictFileName', () => {
  it('produces correct format with extension', () => {
    const result = formatConflictFileName('notes.md', 'MyDevice', new Date('2024-01-15T09:30:00.000Z'));
    expect(result).toBe('notes (RustShare conflicted copy MyDevice 202401150930).md');
  });

  it('produces correct format without extension', () => {
    const result = formatConflictFileName('README', 'Dev-1', new Date('2023-12-25T18:05:00.000Z'));
    expect(result).toBe('README (RustShare conflicted copy Dev-1 202312251805)');
  });

  it('preserves directory path in output', () => {
    const result = formatConflictFileName('folder/file.txt', 'Device', new Date('2024-06-02T11:10:00.000Z'));
    expect(result).toBe('folder/file (RustShare conflicted copy Device 202406021110).txt');
  });

  it('sanitizes device name', () => {
    const result = formatConflictFileName('doc.md', 'Device@Name!', new Date('2024-01-01T00:00:00.000Z'));
    expect(result).toBe('doc (RustShare conflicted copy Device_Name_ 202401010000).md');
  });
});

describe('shouldIgnorePath', () => {
  it('returns true for .obsidian/', () => {
    expect(shouldIgnorePath('.obsidian/app.json')).toBe(true);
    expect(shouldIgnorePath('.obsidian/')).toBe(true);
  });

  it('returns true for .git/', () => {
    expect(shouldIgnorePath('.git/config')).toBe(true);
    expect(shouldIgnorePath('notes/.git/')).toBe(true);
  });

  it('returns true for .DS_Store', () => {
    expect(shouldIgnorePath('.DS_Store')).toBe(true);
    expect(shouldIgnorePath('folder/.DS_Store')).toBe(true);
  });

  it('returns true for node_modules/', () => {
    expect(shouldIgnorePath('node_modules/')).toBe(true);
    expect(shouldIgnorePath('src/node_modules/pkg/')).toBe(true);
  });

  it('returns true for Thumbs.db', () => {
    expect(shouldIgnorePath('Thumbs.db')).toBe(true);
    expect(shouldIgnorePath('images/Thumbs.db')).toBe(true);
  });

  it('returns false for normal file paths', () => {
    expect(shouldIgnorePath('notes/hello.md')).toBe(false);
    expect(shouldIgnorePath('README.md')).toBe(false);
    expect(shouldIgnorePath('deep/nested/path/file.txt')).toBe(false);
  });
});

describe('detectCloudSyncFolder', () => {
  it('detects Dropbox in path', () => {
    expect(detectCloudSyncFolder('/Users/me/Dropbox/Vault')).toBe('Dropbox');
  });

  it('detects iCloud Drive in path', () => {
    expect(detectCloudSyncFolder('/Users/me/Library/Mobile Documents/iCloud Drive/Vault')).toBe('iCloud Drive');
  });

  it('detects OneDrive in path', () => {
    expect(detectCloudSyncFolder('C:/Users/me/OneDrive/Documents/Vault')).toBe('OneDrive');
  });

  it('detects Google Drive in path', () => {
    expect(detectCloudSyncFolder('/home/user/Google Drive/vault')).toBe('Google Drive');
  });

  it('returns null for non-cloud paths', () => {
    expect(detectCloudSyncFolder('/home/user/vault')).toBeNull();
    expect(detectCloudSyncFolder('C:/Users/me/Documents/Vault')).toBeNull();
  });
});
