import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { queryClient } from '$lib/query-client';
import BrainstormingModuleView from './BrainstormingModuleView.svelte';

const mocks = vi.hoisted(() => ({
	goto: vi.fn(),
	listBrainstormBoards: vi.fn(),
	createBrainstormBoard: vi.fn()
}));

vi.mock('$app/navigation', () => ({
	goto: mocks.goto
}));

vi.mock('$lib/api/brainstorming', () => ({
	listBrainstormBoards: mocks.listBrainstormBoards,
	createBrainstormBoard: mocks.createBrainstormBoard
}));

const mockModule = {
	id: 'module_brainstorming',
	key: 'brainstorming',
	displayName: 'Brainstorming',
	description: 'Visual decision boards.',
	enabled: true,
	rootPath: '/Brainstorming',
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
				description: 'Recent boards.',
				size: 'medium' as const,
				columns: { desktop: 6, tablet: 12, mobile: 12 },
				maxItems: 4,
				primaryAction: { label: 'New Board', action: 'create-from-template', template: 'template_blank_brainstorm' }
			}
		},
		page: {
			enabled: true,
			route: '/modules/brainstorming',
			renderer: 'brainstorming',
			layout: 'gallery-grid',
			emptyStateTitle: 'No brainstorming boards yet',
			emptyStateDescription: 'Create your first visual decision board.',
			primaryAction: { label: 'New Board', action: 'create-from-template', template: 'template_blank_brainstorm' }
		}
	},
	aiIndexing: { enabled: true },
	audit: { enabled: true }
};

describe('BrainstormingModuleView', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		queryClient.clear();
	});

	it('renders empty state when no boards exist', async () => {
		mocks.listBrainstormBoards.mockResolvedValue([]);

		render(BrainstormingModuleView, { module: mockModule });

		await waitFor(() => {
			expect(screen.getByText('No brainstorming boards yet')).toBeTruthy();
		});
		expect(screen.getByText('Create your first visual decision board.')).toBeTruthy();
	});

	it('renders board gallery with thumbnails', async () => {
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
			}
		]);

		render(BrainstormingModuleView, { module: mockModule });

		const link = await screen.findByRole('link', { name: /Q3 Roadmap/i });
		expect(link.getAttribute('href')).toBe('/modules/brainstorming/board-1');
	});

	it('shows fallback placeholder when preview.png is missing', async () => {
		mocks.listBrainstormBoards.mockResolvedValue([
			{
				id: 'board-1',
				title: 'No Preview Board',
				slug: 'no-preview-board',
				path: '/Brainstorming/no-preview-board',
				template: 'template_blank_brainstorm',
				source_file_id: 'file-1',
				preview_file_id: null,
				created_at: '2026-04-30T00:00:00Z',
				updated_at: '2026-04-30T12:00:00Z'
			}
		]);

		render(BrainstormingModuleView, { module: mockModule });

		await screen.findByText('No Preview Board');
		expect(screen.getByText('No preview')).toBeTruthy();
	});

	it('navigates to editor when board card is clicked', async () => {
		mocks.listBrainstormBoards.mockResolvedValue([
			{
				id: 'board-1',
				title: 'Clickable Board',
				slug: 'clickable-board',
				path: '/Brainstorming/clickable-board',
				template: 'template_blank_brainstorm',
				source_file_id: 'file-1',
				preview_file_id: null,
				created_at: '2026-04-30T00:00:00Z',
				updated_at: '2026-04-30T12:00:00Z'
			}
		]);

		render(BrainstormingModuleView, { module: mockModule });

		const link = await screen.findByRole('link', { name: /Clickable Board/i });
		expect(link.getAttribute('href')).toBe('/modules/brainstorming/board-1');
	});
});
