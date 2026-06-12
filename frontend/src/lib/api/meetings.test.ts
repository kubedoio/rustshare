import { beforeEach, describe, expect, it, vi } from 'vitest';
import { meetingsApi } from '$lib/api/meetings';

vi.mock('$lib/api/client', () => ({
	apiClient: {
		get: vi.fn()
	}
}));

import { apiClient } from '$lib/api/client';

describe('meetings API', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('returns a single page when a limit is provided', async () => {
		const page = Array.from({ length: 50 }, (_, index) => ({
			id: `meeting-${index}`,
			name: `meeting-${index}.md`
		}));
		vi.mocked(apiClient.get).mockResolvedValueOnce(page);

		await expect(meetingsApi.list(50)).resolves.toHaveLength(50);
		expect(apiClient.get).toHaveBeenCalledTimes(1);
		expect(apiClient.get).toHaveBeenCalledWith('/meetings?per_page=50');
	});

	it('fetches all pages when no limit is provided', async () => {
		const firstPage = Array.from({ length: 100 }, (_, index) => ({
			id: `meeting-${index}`,
			name: `meeting-${index}.md`
		}));
		const secondPage = [{ id: 'meeting-100', name: 'meeting-100.md' }];

		vi.mocked(apiClient.get).mockResolvedValueOnce(firstPage).mockResolvedValueOnce(secondPage);

		await expect(meetingsApi.list()).resolves.toHaveLength(101);
		expect(apiClient.get).toHaveBeenNthCalledWith(1, '/meetings?page=1&per_page=100');
		expect(apiClient.get).toHaveBeenNthCalledWith(2, '/meetings?page=2&per_page=100');
	});
});
