import { describe, it, expect, vi, beforeEach } from 'vitest';
import { get } from 'svelte/store';

// Mock dependencies before importing the store
vi.mock('../api/auth', () => ({
	login: vi.fn(),
	logout: vi.fn()
}));

vi.mock('../api/users', () => ({
	getUserProfile: vi.fn()
}));

vi.mock('./replication', () => ({
	replicationStore: { reset: vi.fn() }
}));

vi.mock('./theme', () => ({
	themeStore: { loadFromBackend: vi.fn() }
}));

vi.mock('../websocket/manager', () => ({
	initializeWebSocket: vi.fn(),
	cleanupWebSocket: vi.fn()
}));

vi.mock('../query-client', () => ({
	queryClient: {
		clear: vi.fn(),
		invalidateQueries: vi.fn()
	}
}));

import { login as loginRequest, logout as logoutRequest } from '../api/auth';
import { getUserProfile } from '../api/users';
import { authStore, currentUser } from './auth';

describe('Auth Store Race Condition', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		// Reset store to initial state
		authStore.setLoading(true);
		authStore.logout();
	});

	it('should ignore stale bootstrapSession results after a successful login', async () => {
		const mockLoginRequest = vi.mocked(loginRequest);
		const mockGetUserProfile = vi.mocked(getUserProfile);
		const mockLogoutRequest = vi.mocked(logoutRequest);

		// Simulate logout succeeding
		mockLogoutRequest.mockResolvedValue(undefined);

		// Set up getUserProfile to simulate a slow bootstrap request
		// that returns the OLD user after login completes
		let bootstrapResolve: (value: unknown) => void;
		const bootstrapPromise = new Promise((resolve) => {
			bootstrapResolve = resolve;
		});

		mockGetUserProfile.mockImplementation(() => bootstrapPromise as Promise<never>);

		// Trigger bootstrapSession by calling refreshSession
		// (since bootstrapSession already ran on module load in the real app)
		authStore.refreshSession();

		// Now simulate a successful login as a NEW user
		mockLoginRequest.mockResolvedValue({
			user: {
				id: 'user-b',
				email: 'user-b@example.com',
				display_name: 'User B',
				is_admin: false
			}
		});

		// Also set up getUserProfile for the login flow to return User B
		mockGetUserProfile.mockResolvedValueOnce({
			id: 'user-b',
			email: 'user-b@example.com',
			display_name: 'User B',
			is_admin: false,
			storage_quota: 0,
			storage_used: 0,
			theme: 'system',
			username: 'userb',
			created_at: new Date().toISOString(),
			updated_at: new Date().toISOString()
		});

		// Perform login
		await authStore.login('user-b@example.com', 'password');

		// Verify login set User B
		expect(get(currentUser)?.display_name).toBe('User B');

		// Now the stale bootstrapSession resolves with User A
		bootstrapResolve!({
			id: 'user-a',
			email: 'user-a@example.com',
			display_name: 'User A',
			is_admin: false,
			storage_quota: 0,
			storage_used: 0,
			theme: 'system',
			username: 'usera',
			created_at: new Date().toISOString(),
			updated_at: new Date().toISOString()
		});

		// Give the async bootstrap a tick to finish
		await new Promise((resolve) => setTimeout(resolve, 10));

		// The store should STILL have User B, not User A
		expect(get(currentUser)?.display_name).toBe('User B');
	});

	it('should ignore stale login results after logout', async () => {
		const mockLoginRequest = vi.mocked(loginRequest);
		const mockLogoutRequest = vi.mocked(logoutRequest);
		const mockGetUserProfile = vi.mocked(getUserProfile);

		mockLogoutRequest.mockResolvedValue(undefined);

		// Start a login that will resolve slowly
		let loginResolve: ((value: Awaited<ReturnType<typeof loginRequest>>) => void) | undefined;
		mockLoginRequest.mockImplementation(
			() => new Promise((resolve) => {
				loginResolve = resolve as (value: Awaited<ReturnType<typeof loginRequest>>) => void;
			})
		);

		// Fire login but don't await it
		const loginPromise = authStore.login('user@example.com', 'password');

		// Immediately logout before login finishes
		await authStore.logout();

		// Verify store is logged out
		expect(get(authStore).isAuthenticated).toBe(false);
		expect(get(authStore).user).toBeNull();

		// Now let the original login resolve
		loginResolve?.({
			user: {
				id: 'user-x',
				email: 'user@example.com',
				display_name: 'User X',
				is_admin: false
			}
		} as Awaited<ReturnType<typeof loginRequest>>);

		// Also need getUserProfile to resolve for the login flow
		mockGetUserProfile.mockResolvedValue({
			id: 'user-x',
			email: 'user@example.com',
			display_name: 'User X',
			is_admin: false,
			storage_quota: 0,
			storage_used: 0,
			theme: 'system',
			username: 'userx',
			created_at: new Date().toISOString(),
			updated_at: new Date().toISOString()
		});

		// Wait for the login promise to finish (it should be a no-op due to generation mismatch)
		await loginPromise.catch(() => {});

		// Give async operations a tick
		await new Promise((resolve) => setTimeout(resolve, 10));

		// Store should still be logged out
		expect(get(authStore).isAuthenticated).toBe(false);
		expect(get(authStore).user).toBeNull();
	});
});
