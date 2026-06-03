import { describe, it, expect, vi, beforeEach } from 'vitest';
import { SyncQueue, SyncOperation } from '../src/sync-queue';

// Polyfill window.setTimeout/clearTimeout for Node environment
Object.assign(globalThis, { window: globalThis });

describe('SyncQueue', () => {
  const options = {
    debounceMs: 100,
    maxRetries: 5,
    retryBaseDelayMs: 1000,
    retryMaxDelayMs: 10000,
  };

  let onSync: ReturnType<typeof vi.fn>;
  let queue: SyncQueue;

  beforeEach(() => {
    vi.useFakeTimers();
    onSync = vi.fn().mockResolvedValue(undefined);
    queue = new SyncQueue(options, onSync);
    queue.start();
  });

  afterEach(() => {
    queue.stop();
    vi.useRealTimers();
  });

  it('add() schedules a sync after debounce', async () => {
    queue.add({ path: '/a.md', type: 'create' });
    expect(onSync).not.toHaveBeenCalled();

    vi.advanceTimersByTime(options.debounceMs);
    await Promise.resolve();

    expect(onSync).toHaveBeenCalledTimes(1);
    expect(onSync).toHaveBeenCalledWith([{ path: '/a.md', type: 'create' }]);
  });

  it('multiple adds within debounce period result in single sync call', async () => {
    queue.add({ path: '/a.md', type: 'create' });
    vi.advanceTimersByTime(options.debounceMs - 10);
    queue.add({ path: '/b.md', type: 'modify' });
    vi.advanceTimersByTime(options.debounceMs - 10);
    queue.add({ path: '/c.md', type: 'delete' });

    vi.advanceTimersByTime(options.debounceMs);
    await Promise.resolve();

    expect(onSync).toHaveBeenCalledTimes(1);
    const ops = onSync.mock.calls[0][0] as SyncOperation[];
    expect(ops).toHaveLength(3);
  });

  it('flush() immediately triggers sync', async () => {
    queue.add({ path: '/a.md', type: 'create' });
    queue.flush();
    await Promise.resolve();

    expect(onSync).toHaveBeenCalledTimes(1);
  });

  it('stop() prevents scheduled syncs', async () => {
    queue.add({ path: '/a.md', type: 'create' });
    queue.stop();

    vi.advanceTimersByTime(options.debounceMs * 10);
    await Promise.resolve();

    expect(onSync).not.toHaveBeenCalled();
  });

  it('network error causes offline queueing and retry with exponential backoff', async () => {
    const networkError = new Error('Failed to fetch');
    onSync.mockRejectedValue(networkError);
    vi.spyOn(Math, 'random').mockReturnValue(0);

    queue.add({ path: '/a.md', type: 'create' });
    queue.flush();
    await Promise.resolve();

    // First failure: offline queue should have the op
    expect(queue.isOnline()).toBe(false);
    expect(queue.getOfflineQueueCount()).toBe(1);
    expect(onSync).toHaveBeenCalledTimes(1);

    // Advance by first retry delay: base * 2^0 = 1000
    vi.advanceTimersByTime(1000);
    await Promise.resolve();
    expect(onSync).toHaveBeenCalledTimes(2);

    // Advance by second retry delay: base * 2^1 = 2000
    vi.advanceTimersByTime(2000);
    await Promise.resolve();
    expect(onSync).toHaveBeenCalledTimes(3);

    vi.restoreAllMocks();
  });

  it('setOnline(true) drains offline queue and flushes', async () => {
    const networkError = new Error('network offline');
    onSync.mockRejectedValue(networkError);

    queue.add({ path: '/a.md', type: 'create' });
    queue.flush();
    await Promise.resolve();

    expect(queue.isOnline()).toBe(false);
    expect(queue.getOfflineQueueCount()).toBe(1);

    onSync.mockResolvedValue(undefined);
    queue.setOnline(true);
    await Promise.resolve();

    expect(queue.isOnline()).toBe(true);
    expect(queue.getOfflineQueueCount()).toBe(0);
    expect(onSync).toHaveBeenCalledTimes(2);
  });

  it('rename deduplication: pending op for oldPath replaced by rename to newPath', () => {
    queue.add({ path: '/old.md', type: 'modify' });
    expect(queue.getPendingCount()).toBe(1);

    queue.add({ path: '/new.md', type: 'rename', oldPath: '/old.md' });
    expect(queue.getPendingCount()).toBe(1);

    const pending = Array.from((queue as any).pending.values());
    expect(pending[0].path).toBe('/new.md');
    expect(pending[0].type).toBe('rename');
    expect(pending[0].oldPath).toBe('/old.md');
  });

  it('rename deduplication: create on oldPath becomes create on newPath', () => {
    queue.add({ path: '/old.md', type: 'create' });
    queue.add({ path: '/new.md', type: 'rename', oldPath: '/old.md' });

    const pending = Array.from((queue as any).pending.values());
    expect(pending).toHaveLength(1);
    expect(pending[0].path).toBe('/new.md');
    expect(pending[0].type).toBe('create');
    expect(pending[0].oldPath).toBeUndefined();
  });

  it('concurrent run protection: if onSync is still running, runSync should not start another', async () => {
    let resolveSync: (() => void) | null = null;
    onSync.mockImplementation(() => new Promise<void>((resolve) => {
      resolveSync = resolve;
    }));

    queue.add({ path: '/a.md', type: 'create' });
    queue.flush();

    // Give flush a chance to start the first run
    await Promise.resolve();

    // While first run is in flight, try to flush again
    queue.flush();
    await Promise.resolve();

    expect(onSync).toHaveBeenCalledTimes(1);

    // Allow the first run to complete
    resolveSync!();
    await Promise.resolve();

    // The second flush should NOT have triggered another call
    expect(onSync).toHaveBeenCalledTimes(1);
  });
});
