import { fireEvent, render, screen } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
	goto: vi.fn(),
	authState: {
		isLoading: false,
		isAuthenticated: true,
		user: { email: 'admin@example.com', is_admin: true } as {
			email: string;
			is_admin: boolean;
		} | null
	}
}));

vi.mock('$app/navigation', () => ({ goto: mocks.goto }));

vi.mock('$app/stores', () => ({
	page: {
		subscribe: (fn: any) => {
			fn({ url: { pathname: '/admin/users' } });
			return () => {};
		}
	}
}));

vi.mock('$lib/stores/auth', () => ({
	authStore: {
		subscribe: (fn: any) => {
			fn(mocks.authState);
			return () => {};
		}
	}
}));

import AdminLayout from '../+layout.svelte';

describe('admin layout', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mocks.authState = {
			isLoading: false,
			isAuthenticated: true,
			user: { email: 'admin@example.com', is_admin: true }
		};
	});

	it('collapses the sidebar behind a hamburger toggle below md', async () => {
		const { container } = render(AdminLayout);

		// Sidebar starts hidden (shown statically from md up via md:flex).
		const aside = container.querySelector('aside')!;
		expect(aside.className).toContain('hidden');
		expect(aside.className).toContain('md:flex');

		// Hamburger opens it as an overlay.
		await fireEvent.click(screen.getByRole('button', { name: 'Open admin navigation' }));
		expect(aside.className).toContain('fixed');
		expect(aside.className).toContain('flex');
		expect(screen.getByRole('button', { name: 'Close admin navigation' })).toBeTruthy();

		// Overlay click closes it again.
		await fireEvent.click(screen.getByRole('button', { name: 'Close admin navigation' }));
		expect(aside.className).toContain('hidden');
	});

	it('closes the mobile sidebar after navigating', async () => {
		const { container } = render(AdminLayout);
		const aside = container.querySelector('aside')!;

		await fireEvent.click(screen.getByRole('button', { name: 'Open admin navigation' }));
		expect(aside.className).toContain('fixed');

		await fireEvent.click(screen.getByRole('link', { name: /Users/ }));
		expect(aside.className).toContain('hidden');
	});

	it('redirects non-admin users to the dashboard', () => {
		mocks.authState.user = { email: 'user@example.com', is_admin: false };
		render(AdminLayout);
		expect(mocks.goto).toHaveBeenCalledWith('/dashboard');
	});
});
