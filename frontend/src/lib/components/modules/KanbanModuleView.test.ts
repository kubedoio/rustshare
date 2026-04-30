import { render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import KanbanModuleView from './KanbanModuleView.svelte';

vi.mock('$app/navigation', () => ({
	goto: vi.fn()
}));

vi.mock('$lib/api/modules', () => ({
	createFromTemplate: vi.fn()
}));

vi.mock('$lib/api/folders', () => ({
	getFolderContents: vi.fn(async (folderId: string | null) => {
		if (folderId === 'kanban-root') {
			return {
				folders: [
					{ id: 'board-1', name: 'Product Roadmap', updated_at: '2026-04-30T10:00:00Z' },
					{ id: 'not-a-board', name: 'Untitled Note', updated_at: '2026-04-29T10:00:00Z' }
				],
				files: []
			};
		}

		if (folderId === 'board-1') {
			return {
				folders: [
					{ id: 'col-1', name: '00-Backlog', updated_at: '2026-04-30T10:00:00Z' },
					{ id: 'col-2', name: '03-Review', updated_at: '2026-04-30T10:00:00Z' }
				],
				files: [
					{
						id: 'board-metadata',
						name: '.rustshare-module.json',
						modified_at: '2026-04-30T10:00:00Z'
					}
				]
			};
		}

		if (folderId === 'not-a-board') {
			return {
				folders: [],
				files: [
					{
						id: 'note-file',
						name: 'Untitled Note.md',
						modified_at: '2026-04-29T10:00:00Z'
					}
				]
			};
		}

		if (folderId === 'col-1') {
			return {
				folders: [],
				files: [
					{
						id: 'file-1',
						name: 'Define MVP.md',
						modified_at: '2026-04-30T10:00:00Z'
					}
				]
			};
		}

		if (folderId === 'col-2') {
			return {
				folders: [{ id: 'card-folder', name: 'Design Review', updated_at: '2026-04-29T10:00:00Z' }],
				files: []
			};
		}

		return { folders: [], files: [] };
	})
}));

describe('KanbanModuleView', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		global.fetch = vi.fn(async () => ({
			ok: true,
			json: async () => ({
				folders: [{ id: 'kanban-root', name: 'Kanban' }],
				files: []
			})
		})) as unknown as typeof fetch;
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
		expect(screen.queryByRole('button', { name: 'Untitled Note' })).toBeNull();
	});
});
