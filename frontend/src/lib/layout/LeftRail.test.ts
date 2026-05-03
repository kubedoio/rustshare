import { render, screen } from '@testing-library/svelte';
import { readable } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('$app/stores', () => ({
	page: readable({
		url: new URL('http://localhost/modules/notes')
	})
}));

vi.mock('$lib/query-compat', () => ({
	createQuery: vi.fn(() =>
		readable({
			data: [
				{
					id: 'm2',
					module_key: 'standups',
					display_name: 'Standups',
					description: 'Daily updates',
					enabled: true,
					root_path: '/Workspace/Standups',
					renderer: 'standups',
					default_template: 'template_default_standup',
					icon: 'clipboard-list',
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
						sidebar: { enabled: true, order: 20, icon: 'clipboard-list', label: 'Standups' }
					},
					created_at: '2026-04-30T00:00:00Z',
					updated_at: '2026-04-30T00:00:00Z'
				},
				{
					id: 'm1',
					module_key: 'notes',
					display_name: 'Notes',
					description: 'Notes',
					enabled: true,
					root_path: '/Workspace/Notes',
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
						sidebar: { enabled: true, order: 10, icon: 'sticky-note', label: 'Notes' }
					},
					created_at: '2026-04-30T00:00:00Z',
					updated_at: '2026-04-30T00:00:00Z'
				}
			],
			isLoading: false
		})
	)
}));

vi.mock('$lib/components/dashboard/ModuleIcon.svelte', () => ({
	default: vi.fn()
}));

import LeftRail from './LeftRail.svelte';

describe('LeftRail', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('renders enabled modules below Folders in sidebar order and highlights the active module', () => {
		render(LeftRail);

		const foldersLink = screen.getByRole('link', { name: 'Folders' });
		const notesLink = screen.getByRole('link', { name: 'Notes' });
		const standupsLink = screen.getByRole('link', { name: 'Standups' });

		expect(
			foldersLink.compareDocumentPosition(notesLink) & Node.DOCUMENT_POSITION_FOLLOWING
		).toBeTruthy();
		expect(
			notesLink.compareDocumentPosition(standupsLink) & Node.DOCUMENT_POSITION_FOLLOWING
		).toBeTruthy();
		expect(notesLink.getAttribute('aria-current')).toBe('page');
	});

	it('does not render a notifications bell entry in the left rail', () => {
		render(LeftRail);

		expect(screen.queryByRole('link', { name: 'Notifications' })).toBeNull();
	});
});
