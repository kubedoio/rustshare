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
			expect(screen.getByText(/No Kanban boards yet/)).toBeTruthy();
		});
	});

	it('renders real board data with card counts and timestamps', async () => {
		const { listKanbanBoards } = await import('$lib/api/kanban');
		vi.mocked(listKanbanBoards).mockResolvedValueOnce([
			{
				id: 'board-real',
				title: 'Sprint 42',
				slug: 'sprint-42',
				path: '/Kanban/sprint-42',
				column_count: 3,
				card_count: 12,
				created_at: '2026-04-30T10:00:00Z',
				updated_at: '2026-05-01T14:30:00Z',
				archived: false
			}
		]);

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
			expect(screen.getByText('Sprint 42')).toBeTruthy();
		});
		expect(screen.getByText('12 active cards')).toBeTruthy();
	});

	it('filters archived boards from summary', async () => {
		const { listKanbanBoards } = await import('$lib/api/kanban');
		vi.mocked(listKanbanBoards).mockResolvedValueOnce([
			{
				id: 'board-active',
				title: 'Active Board',
				slug: 'active-board',
				path: '/Kanban/active-board',
				column_count: 2,
				card_count: 5,
				created_at: '2026-04-30T10:00:00Z',
				updated_at: '2026-05-01T14:30:00Z',
				archived: false
			},
			{
				id: 'board-archived',
				title: 'Old Board',
				slug: 'old-board',
				path: '/Kanban/old-board',
				column_count: 1,
				card_count: 0,
				created_at: '2026-04-30T10:00:00Z',
				updated_at: '2026-04-30T10:00:00Z',
				archived: true
			}
		]);

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
			expect(screen.getByText('Active Board')).toBeTruthy();
		});
		expect(screen.queryByText('Old Board')).toBeNull();
	});
});
