import { render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import KanbanSummaryWidget from './KanbanSummaryWidget.svelte';

vi.mock('$lib/api/modules', () => ({
	getModuleSummary: vi.fn(async () => ({
		extra: {
			boards: [{ id: 'board-1', name: 'Test Board' }]
		}
	}))
}));

vi.mock('$lib/api/kanban', () => ({
	listKanbanBoards: vi.fn(async () => [{ id: 'board-1', title: 'Test Board' }]),
	getKanbanBoard: vi.fn(async () => ({
		id: 'board-1',
		title: 'Test Board',
		columns: [
			{
				id: 'col-1',
				title: 'Backlog',
				cards: [{ id: 'card-1', title: 'First Card' }]
			}
		]
	}))
}));

import { queryClient } from '$lib/query-client';

describe('KanbanSummaryWidget', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		queryClient.clear();
	});

	it('renders board summary with columns and cards', async () => {
		render(KanbanSummaryWidget, {
			module: {
				key: 'kanban',
				ui: {
					dashboard: {
						widget: {
							title: 'Kanban',
							description: 'Active boards',
							size: 'large',
							maxItems: 4
						}
					}
				}
			} as any
		});

		await waitFor(() => {
			expect(screen.getAllByText('Backlog').length).toBeGreaterThan(0);
			expect(screen.getByText('First Card')).toBeTruthy();
		});
	});

	it('renders empty state when no boards exist', async () => {
		const { listKanbanBoards } = await import('$lib/api/kanban');
		vi.mocked(listKanbanBoards).mockResolvedValueOnce([]);

		render(KanbanSummaryWidget, {
			module: {
				key: 'kanban',
				ui: {
					dashboard: {
						widget: {
							title: 'Kanban',
							description: 'Active boards',
							size: 'large',
							maxItems: 4
						}
					}
				}
			} as any
		});

		await waitFor(() => {
			expect(screen.getByText(/No boards yet/)).toBeTruthy();
		});
	});
});
