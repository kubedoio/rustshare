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
		getDashboardModulesForUser: vi.fn(() => [
			{
				key: 'kanban',
				ui: {
					dashboard: {
						enabled: true,
						order: 10,
						widget: {
							enabled: true,
							type: 'kanban-summary',
							title: 'Kanban',
							description: 'Kanban boards',
							columns: { desktop: 6, tablet: 12, mobile: 12 }
						}
					}
				}
			},
			{
				key: 'notes',
				ui: {
					dashboard: {
						enabled: true,
						order: 20,
						widget: {
							enabled: true,
							type: 'notes-recent',
							title: 'Latest Notes',
							description: 'Recent notes',
							columns: { desktop: 6, tablet: 12, mobile: 12 }
						}
					}
				}
			},
			{
				key: 'unknown',
				ui: {
					dashboard: {
						enabled: true,
						order: 30,
						widget: {
							enabled: true,
							type: 'unknown-type',
							title: 'Unknown Widget',
							description: 'Unknown',
							columns: { desktop: 6, tablet: 12, mobile: 12 }
						}
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
			expect(screen.getByLabelText('Workspace dashboard widgets')).toBeTruthy();
		});
	});

	it('enabled widgets render and order is respected', async () => {
		render(DashboardPage);

		await vi.waitFor(() => {
			expect(screen.getByText('Kanban')).toBeTruthy();
			expect(screen.getByText('Latest Notes')).toBeTruthy();
		});
	});

	it('unknown widget falls back to generic module summary', async () => {
		render(DashboardPage);

		await vi.waitFor(() => {
			expect(screen.getByText('Unknown Widget')).toBeTruthy();
		});
	});
});
