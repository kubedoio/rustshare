import { beforeEach, describe, expect, it, vi } from 'vitest';
import { listAllUserShares } from '$lib/api/shares';

vi.mock('$lib/api/client', () => ({
	apiClient: {
		get: vi.fn()
	}
}));

import { apiClient } from '$lib/api/client';

describe('shares API', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('fetches all user share pages before returning aggregate lists', async () => {
		const firstPage = Array.from({ length: 100 }, (_, index) => ({
			id: `share-${index}`,
			name: `share-${index}`
		}));
		const secondPage = [{ id: 'share-100', name: 'share-100' }];

		vi.mocked(apiClient.get).mockResolvedValueOnce(firstPage).mockResolvedValueOnce(secondPage);

		await expect(listAllUserShares()).resolves.toHaveLength(101);
		expect(apiClient.get).toHaveBeenNthCalledWith(1, '/shares?page=1&per_page=100');
		expect(apiClient.get).toHaveBeenNthCalledWith(2, '/shares?page=2&per_page=100');
	});
});
