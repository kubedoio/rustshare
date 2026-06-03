// Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.
// This file is part of RustShare Vault Sync.

export async function sha256ArrayBuffer(data: ArrayBuffer): Promise<string> {
  const hashBuffer = await crypto.subtle.digest('SHA-256', data);
  const hashArray = Array.from(new Uint8Array(hashBuffer));
  return hashArray.map(b => b.toString(16).padStart(2, '0')).join('');
}

export async function sha256File(file: File): Promise<string> {
  const arrayBuffer = await file.arrayBuffer();
  return sha256ArrayBuffer(arrayBuffer);
}

export function generateDeviceId(): string {
  const array = new Uint8Array(16);
  crypto.getRandomValues(array);
  return Array.from(array)
    .map(b => b.toString(16).padStart(2, '0'))
    .join('');
}

export function formatConflictFileName(originalPath: string, deviceName: string, date: Date): string {
  const lastDot = originalPath.lastIndexOf('.');
  const ext = lastDot > 0 ? originalPath.slice(lastDot) : '';
  const basename = lastDot > 0 ? originalPath.slice(0, lastDot) : originalPath;
  const lastSlash = basename.lastIndexOf('/');
  const dir = lastSlash >= 0 ? basename.slice(0, lastSlash + 1) : '';
  const name = lastSlash >= 0 ? basename.slice(lastSlash + 1) : basename;
  const timestamp = date.toISOString().replace(/[-:T]/g, '').slice(0, 12); // YYYYMMDDHHMM
  const safeDeviceName = deviceName.replace(/[^a-zA-Z0-9_-]/g, '_');
  return `${dir}${name} (RustShare conflicted copy ${safeDeviceName} ${timestamp})${ext}`;
}

export const DEFAULT_IGNORED_PATHS = [
  '.obsidian/',
  '.git/',
  '.DS_Store',
  'node_modules/',
  'Thumbs.db',
  '.rustshare-sync-state.json',
];

export function shouldIgnorePath(path: string): boolean {
  for (const ignored of DEFAULT_IGNORED_PATHS) {
    if (ignored.endsWith('/')) {
      // Directory patterns: match at start or after a slash
      if (path.startsWith(ignored) || path.includes('/' + ignored)) return true;
    } else {
      // File patterns: exact match or suffix after a slash
      if (path === ignored || path.endsWith('/' + ignored)) return true;
    }
  }
  return false;
}

export function detectCloudSyncFolder(vaultPath: string): string | null {
  const cloudIndicators = ['Dropbox', 'iCloud Drive', 'OneDrive', 'Google Drive', 'Box Sync', 'pCloud'];
  for (const indicator of cloudIndicators) {
    if (vaultPath.includes(indicator)) return indicator;
  }
  return null;
}
