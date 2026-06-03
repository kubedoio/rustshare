// Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.
// This file is part of RustShare Vault Sync.

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

export class SyncQueue {
  private pending = new Map<string, SyncOperation>();
  private offlineQueue: SyncOperation[] = [];
  private debounceTimer: number | null = null;
  private retryTimer: number | null = null;
  private retryCount = 0;
  private isRunning = false;
  private online = true;
  private active = false;
  private lastRetryAfterMs?: number;

  constructor(
    private options: SyncQueueOptions,
    private onSync: (ops: SyncOperation[]) => Promise<void>,
  ) {}

  add(op: SyncOperation): void {
    if (op.type === 'rename' && op.oldPath) {
      const existing = this.pending.get(op.oldPath);
      if (existing) {
        this.pending.delete(op.oldPath);
        const updatedOp: SyncOperation = {
          path: op.path,
          type: existing.type === 'create' ? 'create' : 'rename',
          oldPath: existing.type === 'create' ? undefined : op.oldPath,
        };
        this.pending.set(op.path, updatedOp);
        this.scheduleRun();
        return;
      }
    }

    this.pending.set(op.path, op);
    this.scheduleRun();
  }

  flush(): void {
    this.clearDebounceTimer();
    void this.runSync();
  }

  start(): void {
    this.active = true;
  }

  stop(): void {
    this.active = false;
    this.clearTimers();
  }

  isOnline(): boolean {
    return this.online;
  }

  setOnline(online: boolean): void {
    const wasOffline = !this.online;
    this.online = online;
    if (online && wasOffline) {
      this.retryCount = 0;
      this.clearRetryTimer();
      for (const op of this.offlineQueue) {
        this.pending.set(op.path, op);
      }
      this.offlineQueue = [];
      this.flush();
    }
  }

  getPendingCount(): number {
    return this.pending.size;
  }

  getOfflineQueueCount(): number {
    return this.offlineQueue.length;
  }

  private scheduleRun(): void {
    if (!this.active) return;
    this.clearDebounceTimer();
    this.debounceTimer = window.setTimeout(() => {
      this.debounceTimer = null;
      void this.runSync();
    }, this.options.debounceMs);
  }

  private async runSync(): Promise<void> {
    if (this.isRunning) return;

    const ops = Array.from(this.pending.values());
    if (ops.length === 0) return;

    this.pending.clear();
    this.isRunning = true;

    try {
      await this.onSync(ops);
      this.retryCount = 0;
    } catch (error: any) {
      if (error?.status === 409) {
        throw error;
      }
      if (error?.status === 429 && typeof error?.retry_after === 'number' && !Number.isNaN(error.retry_after)) {
        this.lastRetryAfterMs = error.retry_after * 1000;
      }
      if (this.isRetryableError(error)) {
        this.online = false;
        const maxOfflineQueue = 1000;
        if (this.offlineQueue.length + ops.length > maxOfflineQueue) {
          const toDrop = this.offlineQueue.length + ops.length - maxOfflineQueue;
          this.offlineQueue = this.offlineQueue.slice(toDrop);
          console.warn(`Dropped ${toDrop} oldest operations from offline queue (max ${maxOfflineQueue})`);
        }
        this.offlineQueue.push(...ops);
        this.scheduleRetry();
      } else {
        console.error('Sync queue error (non-retryable):', error);
      }
    } finally {
      this.isRunning = false;
    }
  }

  private scheduleRetry(): void {
    if (!this.active) return;
    if (this.retryCount >= this.options.maxRetries) return;
    this.clearRetryTimer();
    let delay: number;
    if (this.lastRetryAfterMs !== undefined && !Number.isNaN(this.lastRetryAfterMs)) {
      delay = Math.min(this.lastRetryAfterMs, this.options.retryMaxDelayMs) + Math.floor(Math.random() * 500);
    } else {
      delay = Math.min(this.options.retryBaseDelayMs * Math.pow(2, this.retryCount) + Math.floor(Math.random() * 1000), this.options.retryMaxDelayMs);
    }
    this.lastRetryAfterMs = undefined; // consume it
    this.retryCount++;
    this.retryTimer = window.setTimeout(() => {
      this.retryTimer = null;
      for (const op of this.offlineQueue) {
        this.pending.set(op.path, op);
      }
      this.offlineQueue = [];
      void this.runSync();
    }, delay);
  }

  private clearTimers(): void {
    this.clearDebounceTimer();
    this.clearRetryTimer();
  }

  private clearDebounceTimer(): void {
    if (this.debounceTimer !== null) {
      window.clearTimeout(this.debounceTimer);
      this.debounceTimer = null;
    }
  }

  private clearRetryTimer(): void {
    if (this.retryTimer !== null) {
      window.clearTimeout(this.retryTimer);
      this.retryTimer = null;
    }
  }

  private isNetworkError(error: unknown): boolean {
    if (error instanceof TypeError) return true;
    if (error instanceof Error) {
      const msg = error.message;
      if (
        msg.includes('fetch') ||
        msg.includes('network') ||
        msg.includes('offline') ||
        msg.includes('Failed to fetch')
      ) {
        return true;
      }
      // 429 Too Many Requests is a retryable network condition
      if ((error as any).status === 429) {
        return true;
      }
    }
    return false;
  }

  private isRetryableError(error: unknown): boolean {
    if (this.isNetworkError(error)) return true;
    const status = (error as any)?.status;
    if (status === 429) return true;
    if (typeof status === 'number' && status >= 500 && status < 600) return true;
    // Also check message prefix for HTTP status codes
    const msg = (error as any)?.message || '';
    if (/^HTTP 429:/.test(msg)) return true;
    if (/^HTTP 5\d\d:/.test(msg)) return true;
    return false;
  }
}
