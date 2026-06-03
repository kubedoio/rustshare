// Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.
// This file is part of RustShare Vault Sync.

export interface RustShareVaultSyncSettings {
  rustshareUrl: string;
  authToken: string;
  deviceId: string;
  deviceName: string;
  vaultId: string;
  autoSyncIntervalMinutes: number;
  conflictStrategy: 'create_copy';
  // Event sync & retry settings
  debounceMs: number;
  maxRetries: number;
  retryBaseDelayMs: number;
  logSyncEvents: boolean;
}

export const DEFAULT_SETTINGS: RustShareVaultSyncSettings = {
  rustshareUrl: 'https://rustshare.io',
  authToken: '',
  deviceId: '',
  deviceName: '',
  vaultId: '',
  autoSyncIntervalMinutes: 0,
  conflictStrategy: 'create_copy',
  debounceMs: 1500,
  maxRetries: 5,
  retryBaseDelayMs: 2000,
  logSyncEvents: false,
};

export function validateSettings(settings: Partial<RustShareVaultSyncSettings>): string[] {
  const errors: string[] = [];

  if (!settings.rustshareUrl) {
    errors.push('RustShare URL is required');
  } else {
    try {
      const url = new URL(settings.rustshareUrl);
      if (url.protocol !== 'https:' && url.protocol !== 'http:') {
        errors.push('RustShare URL must use HTTP or HTTPS');
      }
    } catch {
      errors.push('RustShare URL is invalid');
    }
  }

  if ((settings.autoSyncIntervalMinutes ?? 0) < 0) {
    errors.push('Auto-sync interval cannot be negative');
  }

  if ((settings.debounceMs ?? 1500) < 0) {
    errors.push('Debounce delay cannot be negative');
  }

  if ((settings.maxRetries ?? 5) < 0) {
    errors.push('Max retries cannot be negative');
  }

  if ((settings.retryBaseDelayMs ?? 2000) < 0) {
    errors.push('Retry base delay cannot be negative');
  }

  return errors;
}
