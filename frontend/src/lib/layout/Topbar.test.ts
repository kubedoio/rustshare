import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { readable } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
	goto: vi.fn(),
	logout: vi.fn(),
	toast: vi.fn()
}));

vi.mock('$app/navigation', () => ({ goto: mocks.goto }));
vi.mock('$lib/stores/toast', () => ({ toastStore: { show: mocks.toast } }));

vi.mock('$app/stores', () => ({
	page: readable({
		url: new URL('http://localhost/dashboard'),
		route: { id: '/(app)/dashboard' }
	})
}));

vi.mock('$lib/stores/auth', () => ({
	currentUser: readable({
		id: 'user-1',
		email: 'alex@example.com',
		display_name: 'Alex',
		is_admin: true
	}),
	authStore: {
		logout: mocks.logout
	}
}));

vi.mock('$lib/stores/search', () => ({
	searchQuery: readable('')
}));

vi.mock('$lib/query-compat', () => ({
	createQuery: vi.fn((options: { queryKey?: unknown[] }) => {
		const key = options.queryKey?.[0];
		let result: { data: unknown; isLoading: boolean } = { data: null, isLoading: false };
		if (key === 'notifications-unread-count') {
			result = { data: { count: 2 }, isLoading: false };
		} else if (key === 'all-files') {
			result = { data: [], isLoading: false };
		} else if (key === 'folder-tree') {
			result = { data: undefined, isLoading: false };
		}
		const store = readable(result);
		return {
			subscribe: store.subscribe,
			setOptions: vi.fn(),
			refetch: vi.fn(),
			remove: vi.fn()
		};
	})
}));

vi.mock('$lib/api/notifications', () => ({
	getUnreadNotificationCount: vi.fn()
}));

vi.mock('$lib/api/files', () => ({
	listAllFiles: vi.fn()
}));

vi.mock('$lib/api/folders', () => ({
	getFolderTree: vi.fn()
}));

vi.mock('$lib/api/features', () => ({
	getFeatures: vi.fn().mockResolvedValue({ invite_enabled: false })
}));

vi.mock('./topbar/GlobalSearch.svelte', () => ({
	default: vi.fn()
}));

vi.mock('./topbar/NewMenuDropdown.svelte', () => ({
	default: vi.fn()
}));

vi.mock('$lib/api/organization-admin', () => ({
	getOrganizationAdminCapability: vi.fn().mockResolvedValue({ allowed: false })
}));

vi.mock('./topbar/InvitePopover.svelte', () => ({
	default: vi.fn()
}));

vi.mock('$lib/components/common/ThemeToggle.svelte', () => ({
	default: vi.fn()
}));

import Topbar from './Topbar.svelte';

describe('Topbar', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mocks.logout.mockReset().mockResolvedValue(undefined);
	});

	it('keeps the notification bell in the top header', () => {
		render(Topbar);

		const notificationsLink = screen.getByRole('link', { name: 'Notifications' });
		expect(notificationsLink).toBeTruthy();
		expect(notificationsLink.getAttribute('href')).toBe('/notifications');
	});

	it('warns and does not redirect when server logout fails', async () => {
		mocks.logout.mockRejectedValueOnce(new Error('offline'));
		render(Topbar);
		await fireEvent.click(screen.getByRole('button', { name: 'A Alex' }));
		await fireEvent.click(screen.getByRole('menuitem', { name: 'Sign out' }));

		await waitFor(() =>
			expect(mocks.toast).toHaveBeenCalledWith(
				'Sign out failed. Your browser session may still be active. Try again.',
				'error'
			)
		);
		expect(mocks.goto).not.toHaveBeenCalled();
	});
});
