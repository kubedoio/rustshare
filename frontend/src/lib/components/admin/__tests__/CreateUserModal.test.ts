import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
	createAdminUser,
	getAdminUser,
	updateAdminUser,
	type CreateUserRequest,
	type UpdateUserRequest
} from '$lib/api/admin';

vi.mock('$lib/api/client', () => ({
	apiClient: {
			postVoid: vi.fn(),
			patchVoid: vi.fn(),
			requestText: vi.fn(),
			requestVoid: vi.fn(),
		get: vi.fn(),
		post: vi.fn(),
		patch: vi.fn(),
		delete: vi.fn()
	}
}));

import { apiClient } from '$lib/api/client';

const mockUser = {
	id: 'u1',
	username: 'alice',
	email: 'alice@example.com',
	display_name: 'Alice Smith',
	is_admin: false,
	storage_quota_bytes: 10737418240,
	storage_used_bytes: 1073741824,
	disabled_at: null,
	created_at: '2026-01-01T00:00:00Z'
};

describe('CreateUserModal admin API functions', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('creates a user with all required fields', async () => {
		vi.mocked(apiClient.post).mockResolvedValue(mockUser);

		const request: CreateUserRequest = {
			username: 'alice',
			email: 'alice@example.com',
			password: 'securepass123'
		};

		const result = await createAdminUser(request);

		expect(apiClient.post).toHaveBeenCalledWith('/admin/users', request);
		expect(result.username).toBe('alice');
	});

	it('creates an admin user', async () => {
		const adminUser = { ...mockUser, is_admin: true };
		vi.mocked(apiClient.post).mockResolvedValue(adminUser);

		const request: CreateUserRequest = {
			username: 'admin2',
			email: 'admin2@example.com',
			password: 'strongpass!',
			is_admin: true
		};

		const result = await createAdminUser(request);

		expect(apiClient.post).toHaveBeenCalledWith('/admin/users', request);
		expect(result.is_admin).toBe(true);
	});

	it('creates a user with storage quota', async () => {
		vi.mocked(apiClient.post).mockResolvedValue(mockUser);

		const request: CreateUserRequest = {
			username: 'bob',
			email: 'bob@example.com',
			password: 'pass1234',
			storage_quota_bytes: 5368709120 // 5 GB
		};

		await createAdminUser(request);

		expect(apiClient.post).toHaveBeenCalledWith('/admin/users', request);
	});

	it('creates a user with display name', async () => {
		vi.mocked(apiClient.post).mockResolvedValue(mockUser);

		const request: CreateUserRequest = {
			username: 'carol',
			email: 'carol@example.com',
			password: 'carols-pass',
			display_name: 'Carol Jones'
		};

		await createAdminUser(request);

		expect(apiClient.post).toHaveBeenCalledWith('/admin/users', request);
	});

	it('fetches a user by ID', async () => {
		vi.mocked(apiClient.get).mockResolvedValue(mockUser);

		const result = await getAdminUser('u1');

		expect(apiClient.get).toHaveBeenCalledWith('/admin/users/u1');
		expect(result.id).toBe('u1');
		expect(result.storage_used_bytes).toBe(1073741824);
	});

	it('updates a user email', async () => {
		const updated = { ...mockUser, email: 'newalice@example.com' };
		vi.mocked(apiClient.patch).mockResolvedValue(updated);

		const request: UpdateUserRequest = { email: 'newalice@example.com' };
		const result = await updateAdminUser('u1', request);

		expect(apiClient.patch).toHaveBeenCalledWith('/admin/users/u1', request);
		expect(result.email).toBe('newalice@example.com');
	});

	it('updates user quota and admin flag', async () => {
		const updated = { ...mockUser, is_admin: true, storage_quota_bytes: 21474836480 };
		vi.mocked(apiClient.patch).mockResolvedValue(updated);

		const request: UpdateUserRequest = {
			is_admin: true,
			storage_quota_bytes: 21474836480
		};

		const result = await updateAdminUser('u1', request);

		expect(apiClient.patch).toHaveBeenCalledWith('/admin/users/u1', request);
		expect(result.is_admin).toBe(true);
	});

	it('creates user with all optional fields', async () => {
		vi.mocked(apiClient.post).mockResolvedValue(mockUser);

		const request: CreateUserRequest = {
			username: 'full',
			email: 'full@example.com',
			password: 'fullpass1',
			display_name: 'Full User',
			is_admin: false,
			storage_quota_bytes: 1073741824
		};

		await createAdminUser(request);

		expect(apiClient.post).toHaveBeenCalledWith('/admin/users', request);
	});
});
