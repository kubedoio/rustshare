import { cleanup, render, screen, waitFor } from '@testing-library/svelte';
import { createRawSnippet, tick } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { authStore } from '$lib/stores/auth';
import AppShell from './AppShell.svelte';

vi.mock('$lib/stores/auth', async () => {
	const { writable } = await import('svelte/store');
	return {
		authStore: writable({
			user: { id: 'user-a', tenant_id: 'tenant-a' },
			isAuthenticated: true,
			isLoading: false
		})
	};
});
vi.mock('$app/stores', async () => {
	const { readable } = await import('svelte/store');
	return { page: readable({ url: new URL('http://localhost/apps/chat') }) };
});
vi.mock('./LeftRail.svelte', () => ({ default: () => {} }));
vi.mock('./SidebarNav.svelte', () => ({ default: () => {} }));
vi.mock('./Topbar.svelte', () => ({ default: () => {} }));
vi.mock('$lib/components/common/KeyboardShortcuts.svelte', () => ({ default: () => {} }));
vi.mock('$lib/components/common/ToastContainer.svelte', () => ({ default: () => {} }));

import type { Writable } from 'svelte/store';
const auth = authStore as unknown as Writable<{
	user: { id: string; tenant_id: string; display_name?: string };
	isAuthenticated: boolean;
	isLoading: boolean;
}>;

beforeEach(() =>
	auth.set({
		user: { id: 'user-a', tenant_id: 'tenant-a' },
		isAuthenticated: true,
		isLoading: false
	})
);
afterEach(cleanup);

describe('private application state', () => {
	it.each([
		{ id: 'user-b', tenant_id: 'tenant-a' },
		{ id: 'user-a', tenant_id: 'tenant-b' }
	])('discards mounted private state when the Principal or tenant changes to %s', async (user) => {
		let mounted = 0;
		let destroyed = 0;
		const children = createRawSnippet(() => ({
			render: () => '<p>Private application content</p>',
			setup: () => {
				mounted += 1;
				return () => {
					destroyed += 1;
				};
			}
		}));
		render(AppShell, { children });
		await screen.findByText('Private application content');
		expect(mounted).toBe(1);
		auth.set({ user, isAuthenticated: true, isLoading: false });
		await waitFor(() => expect(destroyed).toBe(1));
		expect(mounted).toBe(2);
		auth.set({
			user: { ...user, display_name: 'Renamed' },
			isAuthenticated: true,
			isLoading: false
		});
		await tick();
		expect(mounted).toBe(2);
	});
});
