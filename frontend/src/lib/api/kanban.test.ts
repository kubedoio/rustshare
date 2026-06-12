import { beforeEach, describe, expect, it, vi } from 'vitest';
import { listKanbanBoards } from '$lib/api/kanban';

vi.mock('$lib/api/client', () => ({
	apiClient: {
		get: vi.fn()
	}
}));

import { apiClient } from '$lib/api/client';

describe('kanban API', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('returns a single page when a limit is provided', async () => {
		const page = Array.from({ length: 50 }, (_, index) => ({
			id: `board-${index}`,
			title: `Board ${index}`
		}));
		vi.mocked(apiClient.get).mockResolvedValueOnce(page);

		await expect(listKanbanBoards(50)).resolves.toHaveLength(50);
		expect(apiClient.get).toHaveBeenCalledTimes(1);
		expect(apiClient.get).toHaveBeenCalledWith('/modules/kanban/boards?per_page=50');
	});

	it('fetches all board pages when no limit is provided', async () => {
		const firstPage = Array.from({ length: 100 }, (_, index) => ({
			id: `board-${index}`,
			title: `Board ${index}`
		}));
		const secondPage = [{ id: 'board-100', title: 'Board 100' }];

		vi.mocked(apiClient.get).mockResolvedValueOnce(firstPage).mockResolvedValueOnce(secondPage);

		await expect(listKanbanBoards()).resolves.toHaveLength(101);
		expect(apiClient.get).toHaveBeenNthCalledWith(1, '/modules/kanban/boards?page=1&per_page=100');
		expect(apiClient.get).toHaveBeenNthCalledWith(2, '/modules/kanban/boards?page=2&per_page=100');
	});
});
