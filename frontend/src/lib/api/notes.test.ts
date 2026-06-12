import { beforeEach, describe, expect, it, vi } from 'vitest';
import { listNotes } from '$lib/api/notes';

vi.mock('$lib/api/client', () => ({
	apiClient: {
		get: vi.fn()
	}
}));

import { apiClient } from '$lib/api/client';

describe('notes API', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('returns a single page when a limit is provided', async () => {
		const page = Array.from({ length: 50 }, (_, index) => ({
			id: `note-${index}`,
			name: `note-${index}.md`
		}));
		vi.mocked(apiClient.get).mockResolvedValueOnce(page);

		await expect(listNotes(50)).resolves.toHaveLength(50);
		expect(apiClient.get).toHaveBeenCalledTimes(1);
		expect(apiClient.get).toHaveBeenCalledWith('/notes?per_page=50');
	});

	it('fetches all pages when no limit is provided', async () => {
		const firstPage = Array.from({ length: 100 }, (_, index) => ({
			id: `note-${index}`,
			name: `note-${index}.md`
		}));
		const secondPage = [{ id: 'note-100', name: 'note-100.md' }];

		vi.mocked(apiClient.get).mockResolvedValueOnce(firstPage).mockResolvedValueOnce(secondPage);

		await expect(listNotes()).resolves.toHaveLength(101);
		expect(apiClient.get).toHaveBeenNthCalledWith(1, '/notes?page=1&per_page=100');
		expect(apiClient.get).toHaveBeenNthCalledWith(2, '/notes?page=2&per_page=100');
	});
});
