import { render, screen } from '@testing-library/svelte';
import { readable } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('$lib/stores/auth', () => ({
	currentUser: readable({
		id: 'user-1',
		email: 'alex@example.com',
		display_name: 'Alex',
		is_admin: true
	}),
	authStore: {
		logout: vi.fn()
	}
}));

vi.mock('$lib/stores/search', () => ({
	searchQuery: readable('')
}));

vi.mock('$lib/query-compat', () => ({
	createQuery: vi.fn((options: { queryKey?: unknown[] }) => {
		const key = options.queryKey?.[0];
		if (key === 'notifications-unread-count') {
			return readable({ data: { count: 2 }, isLoading: false });
		}

		if (key === 'all-files') {
			return readable({ data: [], isLoading: false });
		}

		if (key === 'folder-tree') {
			return readable({ data: undefined, isLoading: false });
		}

		return readable({ data: null, isLoading: false });
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

vi.mock('./topbar/UserMenuDropdown.svelte', () => ({
	default: vi.fn()
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
	});

	it('keeps the notification bell in the top header', () => {
		render(Topbar);

		const notificationsLink = screen.getByRole('link', { name: 'Notifications' });
		expect(notificationsLink).toBeTruthy();
		expect(notificationsLink.getAttribute('href')).toBe('/notifications');
	});
});
