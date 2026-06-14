import { beforeEach, describe, expect, it, vi } from 'vitest';
import { standupsApi } from '$lib/api/standups';

vi.mock('$lib/api/client', () => ({
	apiClient: {
		get: vi.fn()
	}
}));

import { apiClient } from '$lib/api/client';

describe('standups API', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('returns a single page when a limit is provided', async () => {
		const page = Array.from({ length: 50 }, (_, index) => ({
			id: `standup-${index}`,
			name: `standup-${index}.md`
		}));
		vi.mocked(apiClient.get).mockResolvedValueOnce(page);

		await expect(standupsApi.list(50)).resolves.toHaveLength(50);
		expect(apiClient.get).toHaveBeenCalledTimes(1);
		expect(apiClient.get).toHaveBeenCalledWith('/standups?per_page=50');
	});

	it('fetches all pages when no limit is provided', async () => {
		const firstPage = Array.from({ length: 100 }, (_, index) => ({
			id: `standup-${index}`,
			name: `standup-${index}.md`
		}));
		const secondPage = [{ id: 'standup-100', name: 'standup-100.md' }];

		vi.mocked(apiClient.get).mockResolvedValueOnce(firstPage).mockResolvedValueOnce(secondPage);

		await expect(standupsApi.list()).resolves.toHaveLength(101);
		expect(apiClient.get).toHaveBeenNthCalledWith(1, '/standups?page=1&per_page=100');
		expect(apiClient.get).toHaveBeenNthCalledWith(2, '/standups?page=2&per_page=100');
	});
});
