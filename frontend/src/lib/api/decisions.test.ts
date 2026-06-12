import { beforeEach, describe, expect, it, vi } from 'vitest';
import { decisionsApi } from '$lib/api/decisions';

vi.mock('$lib/api/client', () => ({
	apiClient: {
		get: vi.fn()
	}
}));

import { apiClient } from '$lib/api/client';

describe('decisions API', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('returns a single page when a limit is provided', async () => {
		const page = Array.from({ length: 50 }, (_, index) => ({
			id: `decision-${index}`,
			name: `decision-${index}.md`
		}));
		vi.mocked(apiClient.get).mockResolvedValueOnce(page);

		await expect(decisionsApi.list(50)).resolves.toHaveLength(50);
		expect(apiClient.get).toHaveBeenCalledTimes(1);
		expect(apiClient.get).toHaveBeenCalledWith('/decisions?per_page=50');
	});

	it('fetches all pages when no limit is provided', async () => {
		const firstPage = Array.from({ length: 100 }, (_, index) => ({
			id: `decision-${index}`,
			name: `decision-${index}.md`
		}));
		const secondPage = [{ id: 'decision-100', name: 'decision-100.md' }];

		vi.mocked(apiClient.get).mockResolvedValueOnce(firstPage).mockResolvedValueOnce(secondPage);

		await expect(decisionsApi.list()).resolves.toHaveLength(101);
		expect(apiClient.get).toHaveBeenNthCalledWith(1, '/decisions?page=1&per_page=100');
		expect(apiClient.get).toHaveBeenNthCalledWith(2, '/decisions?page=2&per_page=100');
	});
});
