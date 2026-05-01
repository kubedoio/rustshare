import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import Header from './Header.svelte';

// Mock dependencies
vi.mock('$app/stores', () => {
	const { writable } = require('svelte/store');
	return {
		page: writable({ url: { pathname: '/dashboard' } })
	};
});

vi.mock('$app/navigation', () => {
	return {
		goto: vi.fn()
	};
});

vi.mock('$lib/stores/auth', () => {
	const { writable } = require('svelte/store');
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
		},
		currentUser: writable({
			id: '1',
			email: 'test@example.com',
			display_name: 'Test',
			is_admin: true
		})
	};
});

vi.mock('$lib/query-compat', () => {
	const { writable } = require('svelte/store');
	return {
		createQuery: vi.fn().mockReturnValue(
			writable({
				data: { count: 3 },
				isLoading: false,
				error: null
			})
		)
	};
});

describe('Header Component', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('top bell remains', () => {
		render(Header);
		const notificationsLink = screen.getByTitle('Notifications');
		expect(notificationsLink).not.toBeNull();
		// Badge should show 3 from mock
		expect(notificationsLink.textContent).toContain('3');
	});
});
