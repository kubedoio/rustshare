import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiClient } from './client';
import { unifiedSearch } from './search';

describe('unifiedSearch', () => {
	beforeEach(() => vi.restoreAllMocks());

	it('uses the permission-aware Files search and forwards cancellation', async () => {
		const request = vi.spyOn(apiClient, 'request').mockResolvedValue({ results: [], total: 0 });
		const controller = new AbortController();

		await unifiedSearch('budget review', 12, controller.signal);

		expect(request).toHaveBeenCalledWith('/api/v1/search', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ query: 'budget review', limit: 12, sources: ['files'] }),
			signal: controller.signal
		});
	});
});
