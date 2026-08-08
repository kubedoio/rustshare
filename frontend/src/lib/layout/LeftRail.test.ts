import { render, screen } from '@testing-library/svelte';
import { writable } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('$app/stores', async () => {
	const { writable } = await import('svelte/store');
	return {
		page: writable({
			url: new URL('http://localhost/apps/notes')
		})
	};
});

vi.mock('$lib/query-compat', () => ({
	createQuery: vi.fn(() => {
		const entry = (id: string, name: string, icon: string, slug: string, order: number) => ({
			manifest: {
				apiVersion: 'elembra.io/v1alpha1',
				kind: 'Application' as const,
				metadata: { id: `io.elembra.${slug}`, name, version: '1.0.0', description: name },
				runtime: { kind: 'embedded' as const },
				contracts: { provides: [], requires: [] },
				resources: [],
				contributions: {
					navigation: [
						{ id: `${slug}.navigation`, label: name, icon, route: `/apps/${slug}`, order }
					],
					routes: [{ id: `${slug}.route`, route: `/apps/${slug}`, renderer: slug }],
					commands: [],
					dashboard: [],
					settings: [],
					searchProviders: [],
					renderers: [],
					admin: []
				},
				integrationEvents: { publishes: [], subscribes: [] },
				configuration: { schema: 'config.json' },
				data: { owner: `io.elembra.${slug}`, preserveOnDisable: true, exportSupported: true }
			},
			enabled: true,
			configuration: {},
			health: 'healthy' as const
		});
		return writable({
			data: [
				entry('m2', 'Standups', 'clipboard-list', 'standups', 20),
				entry('m1', 'Notes', 'sticky-note', 'notes', 10)
			],
			isLoading: false
		});
	})
}));

vi.mock('$lib/components/dashboard/ApplicationIcon.svelte', () => ({
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
		(page as any).set({ url: new URL('http://localhost/apps/notes') });
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
		expect(foldersLink.getAttribute('aria-current')).toBeNull();
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

			const workspaceLink = screen.getByRole('link', { name: 'Workspace' });
			const foldersLink = screen.getByRole('link', { name: 'Folders' });

			expect(workspaceLink.querySelector('span.text-sm.font-medium')?.textContent).toBe(
				'Workspace'
			);
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

			const workspaceLink = screen.getByRole('link', { name: 'Workspace' });
			const foldersLink = screen.getByRole('link', { name: 'Folders' });

			expect(workspaceLink.querySelector('span.text-sm.font-medium')).toBeNull();
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

		it('marks Workspace as active on /dashboard', () => {
			(page as any).set({ url: new URL('http://localhost/dashboard') });
			render(LeftRail);

			const workspaceLink = screen.getByRole('link', { name: 'Workspace' });
			expect(workspaceLink.getAttribute('aria-current')).toBe('page');
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
