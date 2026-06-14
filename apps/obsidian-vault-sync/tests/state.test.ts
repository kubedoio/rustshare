import { describe, it, expect } from 'vitest';
import { createEmptySyncState } from '../src/state';

describe('createEmptySyncState', () => {
  it('returns state with correct defaults and empty files/tombstones', () => {
    const state = createEmptySyncState('vault-123', 'device-456', 'My Laptop');

    expect(state.vault_id).toBe('vault-123');
    expect(state.device_id).toBe('device-456');
    expect(state.device_name).toBe('My Laptop');
    expect(state.last_server_rev).toBe(0);
    expect(state.files).toEqual({});
    expect(state.tombstones).toEqual({});
  });
});
