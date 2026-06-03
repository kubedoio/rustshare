// Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.
// This file is part of RustShare Vault Sync.

export interface SyncLogEntry {
  timestamp: string;
  level: 'debug' | 'info' | 'warn' | 'error';
  message: string;
  path?: string;
  error?: string;
}

export class SyncLog {
  private buffer: (SyncLogEntry | undefined)[];
  private head: number;
  private count: number;

  constructor(private maxEntries: number = 500) {
    this.buffer = new Array<SyncLogEntry | undefined>(maxEntries);
    this.head = 0;
    this.count = 0;
  }

  private push(level: SyncLogEntry['level'], message: string, path?: string, error?: string): void {
    const entry: SyncLogEntry = {
      timestamp: new Date().toISOString(),
      level,
      message,
      path,
      error,
    };
    this.buffer[this.head] = entry;
    this.head = (this.head + 1) % this.maxEntries;
    if (this.count < this.maxEntries) {
      this.count++;
    }
  }

  debug(message: string, path?: string): void {
    this.push('debug', message, path);
  }

  info(message: string, path?: string): void {
    this.push('info', message, path);
  }

  warn(message: string, path?: string): void {
    this.push('warn', message, path);
  }

  error(message: string, path?: string, error?: string): void {
    this.push('error', message, path, error);
  }

  getRecent(limit?: number): SyncLogEntry[] {
    const result: SyncLogEntry[] = [];
    const max = limit !== undefined ? Math.min(limit, this.count) : this.count;
    for (let i = 0; i < max; i++) {
      const idx = (this.head - 1 - i + this.maxEntries) % this.maxEntries;
      const entry = this.buffer[idx];
      if (entry !== undefined) {
        result.push(entry);
      }
    }
    return result;
  }

  clear(): void {
    this.buffer = new Array<SyncLogEntry | undefined>(this.maxEntries);
    this.head = 0;
    this.count = 0;
  }
}

export const syncLog = new SyncLog();
