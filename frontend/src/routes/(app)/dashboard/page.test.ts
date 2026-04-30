import { fireEvent, render, screen } from '@testing-library/svelte';
import { readable } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('$lib/modules/moduleActions', () => ({
	runModulePrimaryAction: vi.fn()
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
								order: 10,
								cardTitle: 'Notes',
								cardDescription: 'Notes',
								summaryMode: 'none',
								maxItems: 4,
								primaryAction: {
									label: 'New Note',
									action: 'create-from-template',
									template: 'template_default_note'
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

		expect(screen.getByText('Workspace Summary')).toBeTruthy();
		expect(screen.getByText('Total Files')).toBeTruthy();
		expect(screen.getByText('Shared Items')).toBeTruthy();
		expect(screen.getByText('Storage Usage')).toBeTruthy();
		expect(screen.getByText('Quota')).toBeTruthy();
		expect(screen.getByText('Enabled Modules')).toBeTruthy();
		expect(screen.getByRole('button', { name: /new note/i })).toBeTruthy();
		expect(screen.getByRole('button', { name: /^new$/i })).toBeTruthy();
		expect(screen.queryByText('Workspace Overview')).toBeNull();
	});

	it('routes the summary primary action through the module action helper', async () => {
		render(DashboardPage);

		await fireEvent.click(screen.getByRole('button', { name: /new note/i }));

		expect(runModulePrimaryAction).toHaveBeenCalledWith(
			expect.objectContaining({ module_key: 'notes' }),
			expect.objectContaining({ action: 'create-from-template', label: 'New Note' })
		);
	});
});
