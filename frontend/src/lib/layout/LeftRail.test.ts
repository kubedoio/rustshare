import { render, screen } from '@testing-library/svelte';
import { writable } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('$app/stores', async () => {
	const { writable } = await import('svelte/store');
	return {
		page: writable({
			url: new URL('http://localhost/modules/notes')
		})
	};
});

vi.mock('$lib/query-compat', () => ({
	createQuery: vi.fn(() =>
		writable({
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

vi.mock('$lib/stores/sidebarExpanded', async () => {
	const { writable } = await import('svelte/store');
	return {
		sidebarExpanded: writable(false)
	};
});

import { page } from '$app/stores';
import { sidebarExpanded } from '$lib/stores/sidebarExpanded';
import LeftRail from './LeftRail.svelte';

describe('LeftRail', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		(page as any).set({ url: new URL('http://localhost/modules/notes') });
		(sidebarExpanded as any).set(false);
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

	describe('expanded state', () => {
		beforeEach(() => {
			(sidebarExpanded as any).set(true);
		});

		it('renders visible text labels for primary items', () => {
			render(LeftRail);

			const homeLink = screen.getByRole('link', { name: 'Home' });
			const foldersLink = screen.getByRole('link', { name: 'Folders' });

			expect(homeLink.querySelector('span.text-sm.font-medium')?.textContent).toBe('Home');
			expect(foldersLink.querySelector('span.text-sm.font-medium')?.textContent).toBe('Folders');
		});

		it('renders visible text labels for secondary items', () => {
			render(LeftRail);

			const settingsLink = screen.getByRole('link', { name: 'Settings' });

			expect(settingsLink.querySelector('span.text-sm.font-medium')?.textContent).toBe('Settings');
		});

		it('renders visible text labels for modules', () => {
			render(LeftRail);

			const notesLink = screen.getByRole('link', { name: 'Notes' });
			const standupsLink = screen.getByRole('link', { name: 'Standups' });

			expect(notesLink.querySelector('span.text-sm.font-medium')?.textContent).toBe('Notes');
			expect(standupsLink.querySelector('span.text-sm.font-medium')?.textContent).toBe('Standups');
		});

		it('has a toggle collapse button', () => {
			render(LeftRail);

			expect(screen.getByRole('button', { name: 'Collapse sidebar' })).toBeTruthy();
		});

		it('sets aria-expanded=true on the aside', () => {
			const { container } = render(LeftRail);
			const aside = container.querySelector('aside');

			expect(aside?.getAttribute('aria-expanded')).toBe('true');
		});
	});

	describe('collapsed state', () => {
		beforeEach(() => {
			(sidebarExpanded as any).set(false);
		});

		it('hides visible text labels for primary items', () => {
			render(LeftRail);

			const homeLink = screen.getByRole('link', { name: 'Home' });
			const foldersLink = screen.getByRole('link', { name: 'Folders' });

			expect(homeLink.querySelector('span.text-sm.font-medium')).toBeNull();
			expect(foldersLink.querySelector('span.text-sm.font-medium')).toBeNull();
		});

		it('hides visible text labels for secondary items', () => {
			render(LeftRail);

			const settingsLink = screen.getByRole('link', { name: 'Settings' });

			expect(settingsLink.querySelector('span.text-sm.font-medium')).toBeNull();
		});

		it('hides visible text labels for modules', () => {
			render(LeftRail);

			const notesLink = screen.getByRole('link', { name: 'Notes' });
			const standupsLink = screen.getByRole('link', { name: 'Standups' });

			expect(notesLink.querySelector('span.text-sm.font-medium')).toBeNull();
			expect(standupsLink.querySelector('span.text-sm.font-medium')).toBeNull();
		});

		it('has a toggle expand button', () => {
			render(LeftRail);

			expect(screen.getByRole('button', { name: 'Expand sidebar' })).toBeTruthy();
		});

		it('sets aria-expanded=false on the aside', () => {
			const { container } = render(LeftRail);
			const aside = container.querySelector('aside');

			expect(aside?.getAttribute('aria-expanded')).toBe('false');
		});
	});

	describe('active states for primary items', () => {
		beforeEach(() => {
			(sidebarExpanded as any).set(true);
		});

		it('marks Home as active on /dashboard', () => {
			(page as any).set({ url: new URL('http://localhost/dashboard') });
			render(LeftRail);

			const homeLink = screen.getByRole('link', { name: 'Home' });
			expect(homeLink.getAttribute('aria-current')).toBe('page');
		});

		it('marks Folders as active on /files', () => {
			(page as any).set({ url: new URL('http://localhost/files') });
			render(LeftRail);

			const foldersLink = screen.getByRole('link', { name: 'Folders' });
			expect(foldersLink.getAttribute('aria-current')).toBe('page');
		});

		it('marks Settings as active on /settings', () => {
			(page as any).set({ url: new URL('http://localhost/settings') });
			render(LeftRail);

			const settingsLink = screen.getByRole('link', { name: 'Settings' });
			expect(settingsLink.getAttribute('aria-current')).toBe('page');
		});
	});
});
