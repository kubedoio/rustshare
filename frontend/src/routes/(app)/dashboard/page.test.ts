import { fireEvent, render, screen } from '@testing-library/svelte';
import { readable } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('$lib/modules/moduleActions', () => ({
	runModulePrimaryAction: vi.fn()
}));

vi.mock('$lib/api/workspace-surface', () => ({
	getWorkspaceSurface: vi.fn(async () => ({
		id: 'workspace_dashboard_default',
		key: 'default-workspace-dashboard',
		name: 'Default Workspace Dashboard',
		version: '1.0',
		enabled: true,
		layout: {
			type: 'responsive-grid',
			columns: 12,
			gap: 24,
			compactOverview: true
		},
		sections: [
			{
				key: 'workspace-overview',
				type: 'workspace-summary',
				enabled: true,
				order: 10,
				renderer: 'compact-workspace-overview'
			},
			{
				key: 'summary-insights',
				type: 'dashboard-widgets',
				enabled: true,
				order: 20,
				title: 'Workspace Summary & Insights',
				renderer: 'workspace-widget-grid'
			}
		]
	}))
}));

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
			return readable({
				data: [
					{
						resource_id: 'folder-1',
						resource_name: 'Designs',
						shared_by_name: 'Ada',
						resource_type: 'folder'
					}
				],
				isLoading: false
			});
		}

		if (key === 'enabled-modules') {
			return readable({
				data: [
					{
						id: 'm1',
						module_key: 'kanban',
						display_name: 'Kanban Dashboard',
						description: 'Boards',
						enabled: true,
						root_path: '/Kanban',
						renderer: 'kanban',
						default_template: 'template_default_kanban',
						icon: 'columns',
						schema_version: '1',
						permissions: {
							admin_can_configure: true,
							workspace_members_can_use: true,
							allow_public_share: true,
							allow_internal_share: true
						},
						ai_indexing: { enabled: true },
						audit: { enabled: true },
						ui_config: {
							dashboard: {
								enabled: true,
								order: 10,
								widget: {
									enabled: true,
									type: 'kanban-summary',
									title: 'Kanban Dashboard',
									description: 'Boards',
									size: 'large',
									columns: { desktop: 6, tablet: 12, mobile: 12 },
									maxItems: 4,
									primaryAction: {
										label: 'New Board',
										action: 'create-from-template',
										template: 'template_default_kanban'
									}
								},
								primaryAction: {
									label: 'New Board',
									action: 'create-from-template',
									template: 'template_default_kanban'
								}
							},
							page: {
								enabled: true,
								route: '/modules/kanban',
								renderer: 'kanban',
								layout: 'board',
								emptyStateTitle: 'No boards yet',
								emptyStateDescription: 'Create your first file-backed board.',
								emptyStateAction: 'New Board'
							}
						},
						created_at: '2026-04-30T00:00:00Z',
						updated_at: '2026-04-30T00:00:00Z'
					},
					{
						id: 'm2',
						module_key: 'notes',
						display_name: 'Notes',
						description: 'Notes',
						enabled: true,
						root_path: '/Notes',
						renderer: 'notes',
						default_template: 'template_default_note',
						icon: 'sticky-note',
						schema_version: '1',
						permissions: {
							admin_can_configure: true,
							workspace_members_can_use: true,
							allow_public_share: true,
							allow_internal_share: true
						},
						ai_indexing: { enabled: true },
						audit: { enabled: true },
						ui_config: {
							dashboard: {
								enabled: true,
								order: 30,
								widget: {
									enabled: true,
									type: 'latest-notes',
									title: 'Latest Notes',
									description: 'Recent notes',
									size: 'small',
									columns: { desktop: 3, tablet: 6, mobile: 12 },
									maxItems: 4,
									primaryAction: {
										label: 'New Note',
										action: 'create-from-template',
										template: 'template_default_note'
									}
								}
							}
						},
						created_at: '2026-04-30T00:00:00Z',
						updated_at: '2026-04-30T00:00:00Z'
					}
				],
				isLoading: false
			});
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

vi.mock('$lib/components/dashboard/WorkspaceModules.svelte', () => ({
	default: vi.fn()
}));

import DashboardPage from './+page.svelte';
import { runModulePrimaryAction } from '$lib/modules/moduleActions';

describe('dashboard page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('renders a compact workspace summary with the expected metrics', () => {
		render(DashboardPage);

		expect(screen.getByText("Alex Johnson's Workspace Overview")).toBeTruthy();
		expect(screen.getByText('Files')).toBeTruthy();
		expect(screen.getByText('Shared')).toBeTruthy();
		expect(screen.getByText('Limit')).toBeTruthy();
		expect(screen.getByText('Storage')).toBeTruthy();
		expect(screen.getByText('Workspace Summary & Insights')).toBeTruthy();
		expect(screen.getByText('Kanban Dashboard')).toBeTruthy();
		expect(screen.getByText('Latest Notes')).toBeTruthy();
		expect(screen.getByRole('button', { name: /new board/i })).toBeTruthy();
		expect(screen.getByRole('button', { name: /^new$/i })).toBeTruthy();
	});

	it('routes the summary primary action through the module action helper', async () => {
		render(DashboardPage);

		await fireEvent.click(screen.getByRole('button', { name: /new board/i }));

		expect(runModulePrimaryAction).toHaveBeenCalledWith(
			expect.objectContaining({ module_key: 'kanban' }),
			expect.objectContaining({ action: 'create-from-template', label: 'New Board' })
		);
	});
});
