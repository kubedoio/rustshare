import { beforeEach, describe, expect, it, vi } from 'vitest';
import { listAdminUsers, disableAdminUser, enableAdminUser, deleteAdminUser } from '$lib/api/admin';

vi.mock('$lib/api/client', () => ({
	apiClient: {
			postVoid: vi.fn(),
			patchVoid: vi.fn(),
			requestText: vi.fn(),
			requestVoid: vi.fn(),
		get: vi.fn(),
		post: vi.fn(),
		delete: vi.fn(),
		patch: vi.fn()
	}
}));

import { apiClient } from '$lib/api/client';

describe('UserTable admin API functions', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('lists users with no params', async () => {
		const response = { users: [], total: 0 };
		vi.mocked(apiClient.get).mockResolvedValue(response);

		const result = await listAdminUsers();

		expect(apiClient.get).toHaveBeenCalledWith('/admin/users');
		expect(result).toEqual(response);
	});

	it('lists users with search query', async () => {
		const response = {
			users: [
				{
					id: 'u1',
					username: 'alice',
					email: 'alice@example.com',
					display_name: 'Alice',
					is_admin: false,
					storage_quota_bytes: 10737418240,
					disabled_at: null,
					created_at: '2026-01-01T00:00:00Z'
				}
			],
			total: 1
		};
		vi.mocked(apiClient.get).mockResolvedValue(response);

		const result = await listAdminUsers({ search: 'alice' });

		expect(apiClient.get).toHaveBeenCalledWith('/admin/users?search=alice');
		expect(result.total).toBe(1);
		expect(result.users[0].username).toBe('alice');
	});

	it('lists users with status filter', async () => {
		const response = { users: [], total: 0 };
		vi.mocked(apiClient.get).mockResolvedValue(response);

		await listAdminUsers({ status: 'disabled' });

		expect(apiClient.get).toHaveBeenCalledWith('/admin/users?status=disabled');
	});

	it('lists users with pagination', async () => {
		const response = { users: [], total: 100 };
		vi.mocked(apiClient.get).mockResolvedValue(response);

		await listAdminUsers({ page: 3, per_page: 20 });

		expect(apiClient.get).toHaveBeenCalledWith('/admin/users?page=3&per_page=20');
	});

	it('lists users with combined search, status and pagination', async () => {
		const response = { users: [], total: 0 };
		vi.mocked(apiClient.get).mockResolvedValue(response);

		await listAdminUsers({ search: 'bob', status: 'active', page: 2, per_page: 10 });

		const call = vi.mocked(apiClient.get).mock.calls[0][0] as string;
		expect(call).toContain('search=bob');
		expect(call).toContain('status=active');
		expect(call).toContain('page=2');
		expect(call).toContain('per_page=10');
	});

	it('disables a user', async () => {
		vi.mocked(apiClient.post).mockResolvedValue(undefined);

		await disableAdminUser('u1');

		expect(apiClient.postVoid).toHaveBeenCalledWith('/admin/users/u1/disable');
	});

	it('enables a user', async () => {
		vi.mocked(apiClient.post).mockResolvedValue(undefined);

		await enableAdminUser('u1');

		expect(apiClient.postVoid).toHaveBeenCalledWith('/admin/users/u1/enable');
	});

	it('deletes a user', async () => {
		vi.mocked(apiClient.delete).mockResolvedValue(undefined);

		await deleteAdminUser('u1');

		expect(apiClient.delete).toHaveBeenCalledWith('/admin/users/u1');
	});
});
