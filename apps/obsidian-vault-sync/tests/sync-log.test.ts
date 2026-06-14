import { describe, it, expect, vi, beforeEach } from 'vitest';
import { SyncLog } from '../src/sync-log';

describe('SyncLog', () => {
  let log: SyncLog;

  beforeEach(() => {
    log = new SyncLog();
  });

  it('stores debug/info/warn/error messages and retrieves them via getRecent()', () => {
    log.debug('d', '/a.md');
    log.info('i', '/b.md');
    log.warn('w', '/c.md');
    log.error('e', '/d.md', 'err');

    const recent = log.getRecent();
    expect(recent).toHaveLength(4);
    expect(recent[0].level).toBe('error');
    expect(recent[0].message).toBe('e');
    expect(recent[0].path).toBe('/d.md');
    expect(recent[0].error).toBe('err');

    expect(recent[1].level).toBe('warn');
    expect(recent[1].message).toBe('w');
    expect(recent[1].path).toBe('/c.md');

    expect(recent[2].level).toBe('info');
    expect(recent[2].message).toBe('i');
    expect(recent[2].path).toBe('/b.md');

    expect(recent[3].level).toBe('debug');
    expect(recent[3].message).toBe('d');
    expect(recent[3].path).toBe('/a.md');
  });

  it('getRecent(limit) returns at most limit entries, most recent first', () => {
    log.info('first');
    log.info('second');
    log.info('third');

    const recent = log.getRecent(2);
    expect(recent).toHaveLength(2);
    expect(recent[0].message).toBe('third');
    expect(recent[1].message).toBe('second');
  });

  it('buffer wraps correctly when exceeding maxEntries', () => {
    const smallLog = new SyncLog(3);
    smallLog.info('1');
    smallLog.info('2');
    smallLog.info('3');
    smallLog.info('4');
    smallLog.info('5');

    const recent = smallLog.getRecent();
    expect(recent).toHaveLength(3);
    expect(recent[0].message).toBe('5');
    expect(recent[1].message).toBe('4');
    expect(recent[2].message).toBe('3');
  });

  it('clear() empties the buffer', () => {
    log.info('msg');
    expect(log.getRecent()).toHaveLength(1);

    log.clear();
    expect(log.getRecent()).toHaveLength(0);
  });

  it('entries include correct timestamp, level, message, path, and error fields', () => {
    const before = new Date().toISOString();
    log.error('something failed', '/notes.md', 'stack trace here');
    const after = new Date().toISOString();

    const entry = log.getRecent(1)[0];
    expect(entry.level).toBe('error');
    expect(entry.message).toBe('something failed');
    expect(entry.path).toBe('/notes.md');
    expect(entry.error).toBe('stack trace here');
    expect(entry.timestamp).toBeDefined();
    expect(entry.timestamp >= before && entry.timestamp <= after).toBe(true);
  });
});
