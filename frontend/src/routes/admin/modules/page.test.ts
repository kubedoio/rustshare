import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import { queryClient } from '$lib/query-client';
import AdminModulesPage from './+page.svelte';

const mocks = vi.hoisted(() => ({
	listAdminModules: vi.fn(),
	enableModule: vi.fn(),
	disableModule: vi.fn()
}));

vi.mock('$lib/api/admin-modules', () => ({
	listAdminModules: mocks.listAdminModules,
	enableModule: mocks.enableModule,
	disableModule: mocks.disableModule
}));

describe('AdminModulesPage', () => {
	beforeEach(() => {
		queryClient.clear();
		vi.clearAllMocks();
		mocks.listAdminModules.mockResolvedValue([
			{
				id: 'module_notes',
				module_key: 'notes',
				display_name: 'Notes',
				description: 'OKF notes.',
				enabled: true,
				root_path: '/Workspace/Notes',
				renderer: 'okf-note',
				default_template: 'template_default_okf_note',
				icon: 'sticky-note',
				schema_version: '1.0',
				permissions: {
					admin_can_configure: true,
					workspace_members_can_use: true,
					allow_public_share: false,
					allow_internal_share: true
				},
				ai_indexing: {
					enabled: true,
					source: 'okf-frontmatter-and-markdown',
					permission_aware: true
				},
				audit: { enabled: true },
				ui_config: {
					documentFormat: 'okf-markdown',
					okf: {
						enabled: true,
						conceptType: 'Note',
						frontmatterRequired: true,
						preserveUnknownFields: true
					}
				},
				created_at: '2026-01-01T00:00:00Z',
				updated_at: '2026-01-01T00:00:00Z'
			},
			{
				id: 'module_meetings',
				module_key: 'meetings',
				display_name: 'Meeting Notes',
				description: 'Meeting notes.',
				enabled: true,
				root_path: '/Workspace/Meetings',
				renderer: 'meetings',
				default_template: 'template_default_meeting',
				icon: 'calendar-days',
				schema_version: '1.0',
				permissions: {
					admin_can_configure: true,
					workspace_members_can_use: true,
					allow_public_share: false,
					allow_internal_share: true
				},
				ai_indexing: { enabled: true },
				audit: { enabled: true },
				ui_config: {},
				created_at: '2026-01-01T00:00:00Z',
				updated_at: '2026-01-01T00:00:00Z'
			}
		]);
	});

	it('shows an OKF badge for the notes module', async () => {
		render(AdminModulesPage);

		await screen.findByText('Notes');
		expect(screen.getByText('OKF Note')).toBeTruthy();
	});

	it('does not show an OKF badge for non-OKF modules', async () => {
		render(AdminModulesPage);

		await screen.findByText('Meeting Notes');
		expect(screen.queryByText('OKF Meeting Notes')).toBeFalsy();
	});

	it('renders the OKF document format for notes', async () => {
		render(AdminModulesPage);

		await screen.findByText('okf-markdown');
	});
});
