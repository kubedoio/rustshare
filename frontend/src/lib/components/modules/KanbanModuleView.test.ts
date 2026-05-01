import { render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import KanbanModuleView from './KanbanModuleView.svelte';

vi.mock('$app/navigation', () => ({
	goto: vi.fn()
}));

vi.mock('$lib/api/modules', () => ({
	createFromTemplate: vi.fn()
}));

vi.mock('$lib/api/kanban', () => ({
	listKanbanBoards: vi.fn(async () => [
		{
			id: 'board-1',
			title: 'Product Roadmap',
			slug: 'product-roadmap',
			path: '/Kanban/product-roadmap',
			column_count: 2,
			card_count: 2,
			created_at: '2026-04-30T10:00:00Z',
			updated_at: '2026-04-30T10:00:00Z'
		}
	]),
	getKanbanBoard: vi.fn(async (boardId: string) => {
		if (boardId !== 'board-1') return null;
		return {
			id: 'board-1',
			title: 'Product Roadmap',
			slug: 'product-roadmap',
			path: '/Kanban/product-roadmap',
			columns: [
				{
					id: 'column_backlog',
					title: 'Backlog',
					slug: '00-Backlog',
					order: 0,
					status: 'backlog',
					cards: [
						{
							id: 'card-1',
							title: 'Define MVP',
							slug: 'CARD-0001-define-mvp',
							content: '# Define MVP\n',
							column_id: 'column_backlog',
							status: 'backlog',
							order: 1000,
							assignees: [],
							tags: [],
							priority: 'normal',
							archived: false,
							created_at: '2026-04-30T10:00:00Z',
							updated_at: '2026-04-30T10:00:00Z'
						}
					]
				},
				{
					id: 'column_review',
					title: 'Review',
					slug: '03-Review',
					order: 3,
					status: 'review',
					cards: [
						{
							id: 'card-2',
							title: 'Design Review',
							slug: 'CARD-0002-design-review',
							content: '# Design Review\n',
							column_id: 'column_review',
							status: 'review',
							order: 1000,
							assignees: [],
							tags: [],
							priority: 'normal',
							archived: false,
							created_at: '2026-04-30T10:00:00Z',
							updated_at: '2026-04-30T10:00:00Z'
						}
					]
				}
			],
			created_at: '2026-04-30T10:00:00Z',
			updated_at: '2026-04-30T10:00:00Z'
		};
	}),
	createKanbanBoard: vi.fn(),
	createKanbanCard: vi.fn(),
	updateKanbanCard: vi.fn(),
	moveKanbanCard: vi.fn(),
	archiveKanbanCard: vi.fn(),
	deleteKanbanCard: vi.fn()
}));

describe('KanbanModuleView', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('renders a selected board with column folders and cards', async () => {
		render(KanbanModuleView, {
			moduleConfig: {
				module_key: 'kanban',
				display_name: 'Kanban Dashboard',
				description: 'Manage board cards as folders and files.',
				icon: 'columns',
				root_path: '/Kanban',
				default_template: 'template_default_kanban',
				ui_config: {
					modulePage: {
						emptyStateTitle: 'No boards yet',
						emptyStateDescription: 'Create your first file-backed board.',
						emptyStateAction: 'New Board'
					}
				}
			}
		});

		await waitFor(() => {
			expect(screen.getByText('Backlog')).toBeTruthy();
		});

		expect(screen.getByText('Review')).toBeTruthy();
		expect(screen.getByText('Define MVP')).toBeTruthy();
		expect(screen.getByText('Design Review')).toBeTruthy();
	});
});
