import { render, screen } from '@testing-library/svelte';
import { readable } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import DashboardPage from './+page.svelte';

vi.mock('$app/environment', () => ({
	browser: true
}));

vi.mock('$lib/api/users', () => ({
	getDashboardConfig: vi.fn().mockResolvedValue({
		enabled_modules: ['kanban', 'notes', 'unknown'],
		module_order: ['kanban', 'notes', 'unknown']
	}),
	updateDashboardConfig: vi.fn(),
	listUserModulePreferences: vi.fn().mockResolvedValue([]),
	updateUserModulePreference: vi.fn()
}));

vi.mock('$lib/modules/registry', async (importOriginal) => {
	const mod = await importOriginal<typeof import('$lib/modules/registry')>();
	return {
		...mod,
		getEnabledModules: vi.fn(() => [
			{
				key: 'kanban',
				displayName: 'Kanban',
				ui: {
					dashboard: {
						enabled: true,
						order: 10,
						widget: { enabled: true }
					}
				}
			},
			{
				key: 'notes',
				displayName: 'Notes',
				ui: {
					dashboard: {
						enabled: true,
						order: 20,
						widget: { enabled: true }
					}
				}
			},
			{
				key: 'unknown',
				displayName: 'Unknown',
				ui: {
					dashboard: {
						enabled: true,
						order: 30,
						widget: { enabled: true }
					}
				}
			}
		])
	};
});

vi.mock('$lib/query-compat', () => ({
	createQuery: vi.fn((options: { queryKey?: unknown[] }) => {
		const key = options.queryKey?.[0];
		if (key === 'all-files') {
			return readable({
				data: [
					{ id: 'f1', size: 1200, deleted_at: null },
					{ id: 'f2', size: 3400, deleted_at: null }
				],
				isLoading: false
			});
		}
		if (key === 'shares-received') {
			return readable({ data: [], isLoading: false });
		}
		if (key === 'module-summary') {
			return readable({
				data: {
					total_items: 5,
					recent_items: [
						{ name: 'Item 1', item_type: 'file', updated_at: new Date().toISOString() },
						{ name: 'Item 2', item_type: 'folder', updated_at: new Date().toISOString() }
					]
				},
				isLoading: false
			});
		}
		if (key === 'workspace-module-summaries') {
			return readable({
				data: [
					{
						module: { key: 'kanban', displayName: 'Kanban' },
						summary: {
							total_items: 1,
							recent_items: [
								{ id: 'board-1', name: 'Kanban Board', item_type: 'folder', updated_at: new Date().toISOString() }
							]
						}
					},
					{
						module: { key: 'notes', displayName: 'Notes' },
						summary: {
							total_items: 1,
							recent_items: [
								{ id: 'note-1', name: 'Latest Note.md', item_type: 'file', updated_at: new Date().toISOString() }
							]
						}
					},
					{
						module: { key: 'unknown', displayName: 'Unknown' },
						summary: {
							total_items: 1,
							recent_items: [
								{ id: 'unknown-1', name: 'Unknown Widget.md', item_type: 'file', updated_at: new Date().toISOString() }
							]
						}
					}
				],
				isLoading: false
			});
		}
		if (key === 'kanban-boards-widget') {
			return readable({ data: [], isLoading: false });
		}
		if (key === 'kanban-board-widget') {
			return readable({ data: null, isLoading: false });
		}
		return readable({ data: null, isLoading: false });
	})
}));

vi.mock('$lib/stores/auth', () => ({
	currentUser: readable({
		id: 'user-1',
		email: 'alex@example.com',
		display_name: 'Alex Johnson',
		is_admin: true,
		storage_quota: 10_000,
		storage_used: 4_600
	})
}));

describe('Dashboard Page Workspace Surface', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('renders the dashboard widget grid', async () => {
		render(DashboardPage);

		await vi.waitFor(() => {
			expect(screen.getByLabelText('Workspace summary')).toBeTruthy();
			expect(screen.getByLabelText('Recent artifacts')).toBeTruthy();
		});
	});

	it('module summaries render as recent artifacts', async () => {
		render(DashboardPage);

		await vi.waitFor(() => {
			expect(screen.getAllByText('Kanban Board').length).toBeGreaterThan(0);
			expect(screen.getByText('Latest Note')).toBeTruthy();
		});
	});

	it('unknown module artifacts fall back to a generic file label', async () => {
		render(DashboardPage);

		await vi.waitFor(() => {
			expect(screen.getByText('Unknown Widget')).toBeTruthy();
			expect(screen.getByText('File')).toBeTruthy();
		});
	});
});
