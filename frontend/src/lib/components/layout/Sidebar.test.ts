import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import { writable } from 'svelte/store';
import Sidebar from './Sidebar.svelte';
import { authStore } from '$lib/stores/auth';
import { getSidebarModulesForUser } from '$lib/modules/registry';

// Mock dependencies
vi.mock('$app/stores', () => {
	return {
		page: writable({ url: { pathname: '/dashboard' } })
	};
});

vi.mock('$lib/stores/auth', () => {
	return {
		authStore: {
			subscribe: writable({
				user: {
					id: '1',
					email: 'test@example.com',
					display_name: 'Test',
					is_admin: true
				},
				isAuthenticated: true
			}).subscribe,
			logout: vi.fn()
		}
	};
});

// Sidebar imports getSidebarModulesForUser, so we test its output matches what's rendered
describe('Sidebar Component', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('enabled modules render below My Files', () => {
		render(Sidebar);

		const navLinks = screen.getAllByRole('link');
		const linkTexts = navLinks.map((link) => link.textContent?.trim());

		// Basic navigation items
		const myFilesIndex = linkTexts.findIndex((t) => t?.includes('My Files'));
		expect(myFilesIndex).toBeGreaterThanOrEqual(0);

		// Dynamic modules from registry
		const modules = getSidebarModulesForUser({
			id: '1',
			email: 'test@example.com',
			display_name: 'Test',
			is_admin: true
		});

		// Verify modules are rendered after "My Files" (or rather, "Shared with Me")
		const sharedWithMeIndex = linkTexts.findIndex((t) => t?.includes('Shared with Me'));
		expect(sharedWithMeIndex).toBeGreaterThanOrEqual(myFilesIndex);

		const firstModuleText = modules[0].ui.sidebar.label;
		const firstModuleIndex = linkTexts.findIndex((t) => t?.includes(firstModuleText));
		expect(firstModuleIndex).toBeGreaterThan(sharedWithMeIndex);
	});

	it('left sidebar bell removed', () => {
		render(Sidebar);
		// Notifications should not be in the sidebar anymore
		const notificationsLink = screen.queryByRole('link', { name: /Notifications/i });
		expect(notificationsLink).toBeNull();
	});
});
