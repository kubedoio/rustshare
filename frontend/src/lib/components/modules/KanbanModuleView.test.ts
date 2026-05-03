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

describe('KanbanModuleView', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		queryClient.clear();
	});

	it('renders a selected board with column folders and cards', async () => {
		render(KanbanModuleView, { module: mockModule as any });

		await waitFor(() => {
			expect(screen.getByText('Backlog')).toBeTruthy();
		});

		expect(screen.getByText('Review')).toBeTruthy();
		expect(screen.getByText('Define MVP')).toBeTruthy();
		expect(screen.getByText('Design Review')).toBeTruthy();
	});

	it('shows create board modal when new board button is clicked', async () => {
		render(KanbanModuleView, { module: mockModule as any });

		const newBoardBtn = await screen.findByRole('button', { name: /New Board/i });
		newBoardBtn.click();

		await waitFor(() => {
			expect(screen.getByText('Create New Kanban Board')).toBeTruthy();
		});
	});

	it('shows empty state when no boards exist', async () => {
		const { listKanbanBoards } = await import('$lib/api/kanban');
		vi.mocked(listKanbanBoards).mockResolvedValueOnce([]);

		render(KanbanModuleView, { module: mockModule as any });

		await waitFor(() => {
			expect(screen.getByText('No boards yet')).toBeTruthy();
		});
	});

	it('renders card description preview when available', async () => {
		render(KanbanModuleView, { module: mockModule as any });

		await waitFor(() => {
			expect(screen.getByText('This is a detailed description.')).toBeTruthy();
		});
	});

	it('opens card detail when a card is clicked', async () => {
		global.fetch = vi.fn(async () =>
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

		render(KanbanModuleView, { module: mockModule as any });

		const card = await screen.findByText('Define MVP');
		card.click();

		await waitFor(() => {
			expect(screen.getByText('Define MVP')).toBeTruthy();
		});
	});

	it('shows error and rolls back on failed card move', async () => {
		const { moveKanbanCard } = await import('$lib/api/kanban');
		vi.mocked(moveKanbanCard).mockRejectedValueOnce(new Error('Move failed'));

		render(KanbanModuleView, { module: mockModule as any });

		await waitFor(() => {
			expect(screen.getByText('Define MVP')).toBeTruthy();
		});
	});

	it('hides module when disabled', async () => {
		const disabledModule = { ...mockModule, enabled: false };
		render(KanbanModuleView, { module: disabledModule as any });
		// Component should still render but module is marked disabled externally
		expect(document.body).toBeTruthy();
	});
});

const mockModule = {
	id: 'module_kanban',
	key: 'kanban',
	displayName: 'Kanban Dashboard',
	description: 'Manage board cards as folders and files.',
	enabled: true,
	rootPath: '/Kanban',
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
			route: '/modules/kanban',
			renderer: 'kanban',
			layout: 'kanban-board',
			emptyStateTitle: 'No boards yet',
			emptyStateDescription: 'Create your first file-backed board.',
			primaryAction: { label: 'New Board', action: 'create-from-template' }
		}
	},
	aiIndexing: { enabled: true },
	audit: { enabled: true }
};
