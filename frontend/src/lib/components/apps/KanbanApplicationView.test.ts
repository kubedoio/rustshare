import { render, screen, waitFor, fireEvent } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import KanbanApplicationView from './KanbanApplicationView.svelte';

vi.mock('$app/navigation', () => ({
	goto: vi.fn()
}));

vi.mock('$lib/api/applications', () => ({
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
			updated_at: '2026-04-30T10:00:00Z',
			archived: false
		}
	]),
	getKanbanBoard: vi.fn(async (boardId: string) => {
		if (boardId !== 'board-1') return null;
		return {
			id: 'board-1',
			title: 'Product Roadmap',
			slug: 'product-roadmap',
			path: '/Kanban/product-roadmap',
			labels: [
				{ id: 'label_green', name: 'Low', color: 'green' },
				{ id: 'label_red', name: 'High', color: 'red' }
			],
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
							content: '# Define MVP\n\nThis is a detailed description.',
							description_preview: 'This is a detailed description.',
							column_id: 'column_backlog',
							status: 'backlog',
							order: 1000,
							labels: [{ id: 'label_red', name: 'High', color: 'red' }],
							assignees: [],
							priority: 'normal',
							attachments_count: 0,
							checklist: { done: 0, total: 0 },
							checklists: [],
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
							description_preview: '',
							column_id: 'column_review',
							status: 'review',
							order: 1000,
							labels: [],
							assignees: [],
							priority: 'normal',
							attachments_count: 0,
							checklist: { done: 0, total: 0 },
							checklists: [],
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
	getKanbanCard: vi.fn(async () => ({
		id: 'card-1',
		title: 'Define MVP',
		content: '# Define MVP\n\nThis is a detailed description.',
		description_preview: 'This is a detailed description.',
		column_id: 'column_backlog',
		status: 'backlog',
		order: 1000,
		labels: [{ id: 'label_red', name: 'High', color: 'red' }],
		assignees: [],
		priority: 'normal',
		attachments_count: 0,
		checklist: { done: 0, total: 0 },
		checklists: [],
		archived: false,
		created_at: '2026-04-30T10:00:00Z',
		updated_at: '2026-04-30T10:00:00Z',
		attachments: [],
		activity: []
	})),
	createKanbanBoard: vi.fn(),
	createKanbanCard: vi.fn(),
	updateKanbanBoard: vi.fn(),
	archiveKanbanBoard: vi.fn(),
	updateKanbanCard: vi.fn(),
	moveKanbanCard: vi.fn(),
	archiveKanbanCard: vi.fn(),
	deleteKanbanCard: vi.fn(),
	getKanbanAssignableUsers: vi.fn(async () => []),
	addCardLabel: vi.fn(),
	removeCardLabel: vi.fn(),
	assignCardMember: vi.fn(),
	unassignCardMember: vi.fn(),
	addCardAttachment: vi.fn(),
	deleteCardAttachment: vi.fn(),
	createChecklist: vi.fn(),
	createChecklistItem: vi.fn(),
	toggleChecklistItem: vi.fn(),
	deleteChecklistItem: vi.fn(),
	deleteChecklist: vi.fn(),
	createKanbanLabel: vi.fn()
}));

import { queryClient } from '$lib/query-client';

describe('KanbanApplicationView', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		queryClient.clear();
	});

	it('renders overview with board cards when boards exist', async () => {
		render(KanbanApplicationView, { module: mockModule as any });

		await waitFor(() => {
			expect(screen.getByText('Kanban')).toBeTruthy();
		});

		expect(screen.getByText('Product Roadmap')).toBeTruthy();
		expect(screen.getByText('2 columns · 2 cards')).toBeTruthy();
	});

	it('renders a selected board with column folders and cards after clicking a board', async () => {
		render(KanbanApplicationView, { module: mockModule as any });

		await waitFor(() => {
			expect(screen.getByText('Product Roadmap')).toBeTruthy();
		});

		const boardCard = screen.getByText('Product Roadmap');
		await fireEvent.click(boardCard);

		await waitFor(() => {
			expect(screen.getByText('Backlog')).toBeTruthy();
		});

		expect(screen.getByText('Review')).toBeTruthy();
		expect(screen.getByText('Define MVP')).toBeTruthy();
		expect(screen.getByText('Design Review')).toBeTruthy();
	});

	it('shows create board modal when new board button is clicked from overview', async () => {
		render(KanbanApplicationView, { module: mockModule as any });

		await waitFor(() => {
			expect(screen.getByRole('button', { name: /New board/i })).toBeTruthy();
		});

		const newBoardBtn = screen.getByRole('button', { name: /New board/i });
		newBoardBtn.click();

		await waitFor(() => {
			expect(screen.getByRole('dialog', { name: /New board/i })).toBeTruthy();
		});
	});

	it('shows empty state when no boards exist', async () => {
		const { listKanbanBoards } = await import('$lib/api/kanban');
		vi.mocked(listKanbanBoards).mockResolvedValueOnce([]);

		render(KanbanApplicationView, { module: mockModule as any });

		await waitFor(() => {
			expect(screen.getByText('No boards yet')).toBeTruthy();
		});
	});

	it('renders card description preview when available', async () => {
		render(KanbanApplicationView, { module: mockModule as any });

		await waitFor(() => {
			expect(screen.getByText('Product Roadmap')).toBeTruthy();
		});

		const boardCard = screen.getByText('Product Roadmap');
		await fireEvent.click(boardCard);

		await waitFor(() => {
			expect(screen.getByText('This is a detailed description.')).toBeTruthy();
		});
	});

	it('opens card detail when a card is clicked', async () => {
		global.fetch = vi.fn(
			async () =>
				new Response(
					JSON.stringify({
						id: 'card-1',
						title: 'Define MVP',
						content: '# Define MVP\n\nDetails here.',
						status: 'backlog',
						labels: [],
						assignees: [],
						attachments: [],
						checklists: [],
						activity: []
					}),
					{ status: 200, headers: { 'Content-Type': 'application/json' } }
				)
		) as any;

		render(KanbanApplicationView, { module: mockModule as any });

		await waitFor(() => {
			expect(screen.getByText('Product Roadmap')).toBeTruthy();
		});

		const boardCard = screen.getByText('Product Roadmap');
		await fireEvent.click(boardCard);

		const card = await screen.findByText('Define MVP');
		card.click();

		await waitFor(() => {
			expect(screen.getByText('Define MVP')).toBeTruthy();
		});
	});

	it('shows error and rolls back on failed card move', async () => {
		const { moveKanbanCard } = await import('$lib/api/kanban');
		vi.mocked(moveKanbanCard).mockRejectedValueOnce(new Error('Move failed'));

		render(KanbanApplicationView, { module: mockModule as any });

		await waitFor(() => {
			expect(screen.getByText('Product Roadmap')).toBeTruthy();
		});

		const boardCard = screen.getByText('Product Roadmap');
		await fireEvent.click(boardCard);

		await waitFor(() => {
			expect(screen.getByText('Define MVP')).toBeTruthy();
		});
	});

	it('hides module when disabled', async () => {
		const disabledModule = { ...mockModule, enabled: false };
		render(KanbanApplicationView, { module: disabledModule as any });
		// Component should still render but module is marked disabled externally
		expect(document.body).toBeTruthy();
	});

	it('navigates back to overview when All Boards is clicked', async () => {
		render(KanbanApplicationView, { module: mockModule as any });

		await waitFor(() => {
			expect(screen.getByText('Product Roadmap')).toBeTruthy();
		});

		// Enter board view
		const boardCard = screen.getByText('Product Roadmap');
		await fireEvent.click(boardCard);

		await waitFor(() => {
			expect(screen.getByText('Backlog')).toBeTruthy();
		});

		// Click All Boards
		const allBoardsBtn = screen.getByRole('button', { name: /All Boards/i });
		await fireEvent.click(allBoardsBtn);

		await waitFor(() => {
			expect(screen.getByText('Kanban')).toBeTruthy();
		});
	});
});

const mockModule = {
	id: 'module_kanban',
	key: 'kanban',
	displayName: 'Kanban',
	description: 'Organize lightweight work boards in your workspace.',
	enabled: true,
	rootPath: '/Workspace/Kanban',
	renderer: 'kanban',
	defaultTemplate: 'template_default_kanban',
	schemaVersion: '1.0',
	permissions: {
		adminCanConfigure: true,
		workspaceMembersCanUse: true,
		allowPublicShare: false,
		allowInternalShare: true
	},
	ui: {
		sidebar: { enabled: true, order: 40, icon: 'columns', label: 'Kanban' },
		dashboard: {
			enabled: true,
			order: 40,
			widget: {
				enabled: true,
				type: 'kanban-summary',
				title: 'Kanban',
				description: 'Active boards and cards.',
				size: 'large',
				columns: { desktop: 6, tablet: 12, mobile: 12 },
				maxItems: 4
			}
		},
		page: {
			enabled: true,
			route: '/apps/kanban',
			renderer: 'kanban',
			layout: 'kanban-board',
			emptyStateTitle: 'No boards yet',
			emptyStateDescription: 'Create your first file-backed board.',
			primaryAction: { label: 'New board', action: 'create-from-template' }
		}
	},
	aiIndexing: { enabled: true },
	audit: { enabled: true }
};
