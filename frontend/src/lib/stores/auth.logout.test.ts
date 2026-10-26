import { get } from 'svelte/store';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { UserProfile } from '../api/users';

const client = vi.hoisted(() => ({
	get: vi.fn<() => Promise<UserProfile>>(() => new Promise<UserProfile>(() => {})),
	postVoid: vi.fn()
}));

vi.mock('../api/client', () => ({ apiClient: client }));
vi.mock('./replication', () => ({ replicationStore: { reset: vi.fn() } }));
vi.mock('./theme', () => ({ themeStore: { loadFromBackend: vi.fn() } }));
vi.mock('../websocket/manager', () => ({
	initializeWebSocket: vi.fn(),
	cleanupWebSocket: vi.fn()
}));
vi.mock('../query-client', () => ({
	queryClient: { clear: vi.fn(), invalidateQueries: vi.fn() }
}));
vi.mock('../chat/keys', () => ({ setChatKeyUser: vi.fn() }));
vi.mock('../chat/session', () => ({ clear: vi.fn() }));

import { clear as clearChatSession } from '../chat/session';
import { queryClient } from '../query-client';
import { cleanupWebSocket } from '../websocket/manager';
import { replicationStore } from './replication';
import { authStore } from './auth';

afterEach(() => vi.restoreAllMocks());

describe('auth logout lifecycle', () => {
	it('defers non-Chat teardown on failure and completes it on success', async () => {
		const profile: UserProfile = {
			id: 'user-a',
			tenant_id: 'tenant-a',
			username: 'user-a',
			display_name: 'User A',
			email: 'a@example.com',
			is_admin: false,
			storage_quota: 0,
			storage_used: 0,
			theme: 'system',
			created_at: new Date().toISOString(),
			updated_at: new Date().toISOString()
		};
		client.get.mockResolvedValue(profile);
		await authStore.refreshSession();
		vi.clearAllMocks();
		const removeItem = vi.spyOn(window.sessionStorage, 'removeItem');
		const failure = new Error('offline');
		client.postVoid.mockRejectedValueOnce(failure);

		await expect(authStore.logout()).rejects.toBe(failure);

		expect(client.postVoid).toHaveBeenCalledWith('/auth/logout', null);
		expect(get(authStore).isAuthenticated).toBe(true);
		expect(get(authStore).user?.id).toBe(profile.id);
		expect(clearChatSession).toHaveBeenCalledOnce();
		expect(cleanupWebSocket).not.toHaveBeenCalled();
		expect(replicationStore.reset).not.toHaveBeenCalled();
		expect(removeItem).not.toHaveBeenCalledWith('rustshare.websocket_token');
		expect(queryClient.clear).not.toHaveBeenCalled();

		client.postVoid.mockResolvedValueOnce(undefined);
		await authStore.logout();

		expect(cleanupWebSocket).toHaveBeenCalledOnce();
		expect(replicationStore.reset).toHaveBeenCalledOnce();
		expect(removeItem).toHaveBeenCalledWith('rustshare.websocket_token');
		expect(get(authStore).isAuthenticated).toBe(false);
		expect(queryClient.clear).toHaveBeenCalledOnce();
	});
});
