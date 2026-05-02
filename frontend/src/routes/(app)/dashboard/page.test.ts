import { render, screen } from '@testing-library/svelte';
import { readable } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import DashboardPage from './+page.svelte';
import DashboardWidgetGrid from '$lib/components/dashboard/DashboardWidgetGrid.svelte';

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

	it('compact overview renders with separate stat cards and storage progress', () => {
		render(DashboardPage);

		expect(screen.getByText("Alex Johnson's Workspace Overview")).toBeTruthy();
		expect(screen.getByText('Files')).toBeTruthy();
		expect(screen.getByText('Shared')).toBeTruthy();
		expect(screen.getByText('Limit')).toBeTruthy();
		expect(screen.getByText('Storage')).toBeTruthy();

		// Check progressbar
		const progressbar = screen.getByRole('progressbar');
		expect(progressbar).toBeTruthy();
		// Used 4600 / Quota 10000 = 46%
		expect(progressbar.getAttribute('aria-valuenow')).toBe('46');
	});

	it('enabled widgets render and order is respected', () => {
		render(DashboardPage);

		expect(screen.getByText('Workspace Summary & Insights')).toBeTruthy();

		// Check kanban and notes render
		expect(screen.getByText('Kanban')).toBeTruthy();
		expect(screen.getByText('Latest Notes')).toBeTruthy();
	});

	it('unknown widget falls back to generic module summary', () => {
		render(DashboardPage);
		expect(screen.getByText('Unknown Widget')).toBeTruthy();
	});
});
