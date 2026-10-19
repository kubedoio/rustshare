import { beforeEach, describe, expect, it, vi } from 'vitest';

const client = vi.hoisted(() => ({
	get: vi.fn(),
	post: vi.fn(),
	postVoid: vi.fn(),
	request: vi.fn(),
	delete: vi.fn(),
	patch: vi.fn()
}));

vi.mock('./client', () => ({ apiClient: client }));

import {
	getOrganizationAdminCapability,
	listOrganizationUsers,
	resendOrganizationInvitation,
	addOrganizationGroupMember
} from './organization-admin';

describe('organization administration API client', () => {
	beforeEach(() => vi.clearAllMocks());

	it('queries only the backend capability probe, not profile role flags', async () => {
		client.get.mockResolvedValue({ allowed: true });
		await expect(getOrganizationAdminCapability()).resolves.toEqual({ allowed: true });
		expect(client.get).toHaveBeenCalledWith('/organization-admin/capability');
	});

	it('encodes search and bounds page parameters through the scoped users endpoint', async () => {
		client.get.mockResolvedValue({ users: [], total: 0, page: 1, per_page: 50 });
		await listOrganizationUsers({
			page: 1,
			per_page: 50,
			search: 'a+b@example.org',
			status: 'active'
		});
		expect(client.get).toHaveBeenCalledWith(
			'/organization-admin/users?page=1&per_page=50&search=a%2Bb%40example.org&status=active'
		);
	});

	it('sends a stable caller-provided idempotency key for resend retries', async () => {
		client.request.mockResolvedValue({ invitation_id: 'inv-1' });
		await resendOrganizationInvitation('inv/1', 'retry-key');
		expect(client.request).toHaveBeenCalledWith('/organization-admin/invitations/inv%2F1/resend', {
			method: 'POST',
			headers: { 'Idempotency-Key': 'retry-key' },
			body: '{}'
		});
	});

	it('uses the empty-response client method for successful member additions', async () => {
		await addOrganizationGroupMember('group-1', 'user-1');
		expect(client.postVoid).toHaveBeenCalledWith('/organization-admin/groups/group-1/members', {
			user_id: 'user-1'
		});
	});
});
