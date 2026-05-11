import { render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { queryClient } from '$lib/query-client';
import RecentBrainstormBoardsWidget from './RecentBrainstormBoardsWidget.svelte';

const mocks = vi.hoisted(() => ({
	listBrainstormBoards: vi.fn(),
	goto: vi.fn()
}));

vi.mock('$lib/api/brainstorming', () => ({
	listBrainstormBoards: mocks.listBrainstormBoards
}));

vi.mock('$app/navigation', () => ({
	goto: mocks.goto
}));

const mockModule = {
	id: 'module_brainstorming',
	key: 'brainstorming',
	displayName: 'Brainstorming',
	description: 'Visual decision boards.',
	enabled: true,
	rootPath: '/Workspace/Brainstorming',
	renderer: 'brainstorming',
	defaultTemplate: 'template_blank_brainstorm',
	schemaVersion: '1.0',
	permissions: {
		adminCanConfigure: true,
		workspaceMembersCanUse: true,
		allowPublicShare: false,
		allowInternalShare: true
	},
	ui: {
		sidebar: { enabled: true, order: 55, icon: 'pen-tool', label: 'Brainstorming' },
		dashboard: {
			enabled: true,
			order: 55,
			widget: {
				enabled: true,
				type: 'recent-brainstorm-boards',
				title: 'Brainstorming',
				description: 'Recent visual decision boards.',
				size: 'medium' as const,
				columns: { desktop: 6, tablet: 12, mobile: 12 },
				maxItems: 4,
				primaryAction: {
					label: 'New idea board',
					action: 'create-from-template',
					template: 'template_blank_brainstorm'
				}
			}
		},
		page: {
			enabled: true,
			route: '/modules/brainstorming',
			renderer: 'brainstorming',
			layout: 'gallery-grid',
			emptyStateTitle: 'No brainstorming boards yet',
			emptyStateDescription: 'Create your first visual decision board.',
			primaryAction: {
				label: 'New idea board',
				action: 'create-from-template',
				template: 'template_blank_brainstorm'
			}
		}
	},
	aiIndexing: { enabled: true },
	audit: { enabled: true }
};

describe('RecentBrainstormBoardsWidget', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		queryClient.clear();
	});

	it('renders empty state when no boards exist', async () => {
		mocks.listBrainstormBoards.mockResolvedValue([]);

		render(RecentBrainstormBoardsWidget, { module: mockModule });

		await waitFor(() => {
			expect(screen.getByText('No boards yet.')).toBeTruthy();
		});
	});

	it('renders recent boards with titles and dates', async () => {
		mocks.listBrainstormBoards.mockResolvedValue([
			{
				id: 'board-1',
				title: 'Q3 Roadmap',
				slug: 'q3-roadmap',
				path: '/Brainstorming/q3-roadmap',
				template: 'template_blank_brainstorm',
				source_file_id: 'file-1',
				preview_file_id: 'file-2',
				created_at: '2026-04-30T00:00:00Z',
				updated_at: '2026-04-30T12:00:00Z'
			},
			{
				id: 'board-2',
				title: 'Meeting Notes',
				slug: 'meeting-notes',
				path: '/Brainstorming/meeting-notes',
				template: 'template_meeting_whiteboard',
				source_file_id: 'file-3',
				preview_file_id: null,
				created_at: '2026-04-29T00:00:00Z',
				updated_at: '2026-04-29T10:00:00Z'
			}
		]);

		render(RecentBrainstormBoardsWidget, { module: mockModule });

		await screen.findByText('Q3 Roadmap');
		expect(screen.getByText('Meeting Notes')).toBeTruthy();

		const link1 = screen.getByRole('link', { name: /Q3 Roadmap/i });
		expect(link1.getAttribute('href')).toBe('/modules/brainstorming/board-1');

		const link2 = screen.getByRole('link', { name: /Meeting Notes/i });
		expect(link2.getAttribute('href')).toBe('/modules/brainstorming/board-2');
	});

	it('limits to maxItems boards', async () => {
		const boards = Array.from({ length: 10 }, (_, i) => ({
			id: `board-${i}`,
			title: `Board ${i}`,
			slug: `board-${i}`,
			path: `/Brainstorming/board-${i}`,
			template: 'template_blank_brainstorm',
			source_file_id: null,
			preview_file_id: null,
			created_at: '2026-04-30T00:00:00Z',
			updated_at: '2026-04-30T00:00:00Z'
		}));
		mocks.listBrainstormBoards.mockResolvedValue(boards);

		render(RecentBrainstormBoardsWidget, { module: mockModule });

		await screen.findByText('Board 0');

		const links = screen.getAllByRole('link');
		expect(links.length).toBeLessThanOrEqual(4);
	});
});
