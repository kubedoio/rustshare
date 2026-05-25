import { beforeEach, describe, expect, it, vi } from 'vitest';
import { listAuditLog } from '$lib/api/admin';

vi.mock('$lib/api/client', () => ({
	apiClient: {
		postVoid: vi.fn(),
		patchVoid: vi.fn(),
		requestText: vi.fn(),
		requestVoid: vi.fn(),
		get: vi.fn()
	}
}));

import { apiClient } from '$lib/api/client';

describe('AuditTable admin API functions', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('lists audit log with no filters', async () => {
		const response = {
			entries: [],
			total: 0
		};
		vi.mocked(apiClient.get).mockResolvedValue(response);

		const result = await listAuditLog();

		expect(apiClient.get).toHaveBeenCalledWith('/admin/audit');
		expect(result).toEqual(response);
	});

	it('lists audit log with type filter', async () => {
		const response = {
			entries: [
				{
					id: 'e1',
					occurred_at: '2026-03-22T10:00:00Z',
					type: 'user',
					actor_label: 'admin@example.com',
					action_type: 'create',
					target_label: 'alice',
					detail: { email: 'alice@example.com' }
				}
			],
			total: 1
		};
		vi.mocked(apiClient.get).mockResolvedValue(response);

		const result = await listAuditLog({ type: 'user' });

		expect(apiClient.get).toHaveBeenCalledWith('/admin/audit?type=user');
		expect(result.total).toBe(1);
		expect(result.entries[0].type).toBe('user');
	});

	it('lists audit log with user_id filter', async () => {
		const response = { entries: [], total: 0 };
		vi.mocked(apiClient.get).mockResolvedValue(response);

		await listAuditLog({ user_id: 'u-abc' });

		expect(apiClient.get).toHaveBeenCalledWith('/admin/audit?user_id=u-abc');
	});

	it('lists audit log with date range', async () => {
		const response = { entries: [], total: 0 };
		vi.mocked(apiClient.get).mockResolvedValue(response);

		await listAuditLog({ from: '2026-01-01', to: '2026-03-22' });

		const call = vi.mocked(apiClient.get).mock.calls[0][0] as string;
		expect(call).toContain('from=2026-01-01');
		expect(call).toContain('to=2026-03-22');
	});

	it('lists audit log with pagination', async () => {
		const response = { entries: [], total: 200 };
		vi.mocked(apiClient.get).mockResolvedValue(response);

		await listAuditLog({ page: 2, per_page: 50 });

		const call = vi.mocked(apiClient.get).mock.calls[0][0] as string;
		expect(call).toContain('page=2');
		expect(call).toContain('per_page=50');
	});

	it('lists audit log with all filters combined', async () => {
		const response = { entries: [], total: 5 };
		vi.mocked(apiClient.get).mockResolvedValue(response);

		await listAuditLog({
			type: 'file',
			user_id: 'u1',
			from: '2026-01-01',
			to: '2026-03-22',
			page: 1,
			per_page: 25
		});

		const call = vi.mocked(apiClient.get).mock.calls[0][0] as string;
		expect(call).toContain('type=file');
		expect(call).toContain('user_id=u1');
		expect(call).toContain('from=2026-01-01');
		expect(call).toContain('to=2026-03-22');
		expect(call).toContain('page=1');
		expect(call).toContain('per_page=25');
	});

	it('does not include undefined params in query string', async () => {
		const response = { entries: [], total: 0 };
		vi.mocked(apiClient.get).mockResolvedValue(response);

		await listAuditLog({ type: 'auth', user_id: undefined });

		const call = vi.mocked(apiClient.get).mock.calls[0][0] as string;
		expect(call).toContain('type=auth');
		expect(call).not.toContain('user_id');
	});
});
