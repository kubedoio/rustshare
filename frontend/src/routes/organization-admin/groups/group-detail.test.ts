import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
	route: null as any,
	getGroup: vi.fn(),
	listUsers: vi.fn(),
	updateGroup: vi.fn(),
	addMember: vi.fn(),
	removeMember: vi.fn()
}));

vi.mock('$app/stores', async () => {
	const { writable } = await import('svelte/store');
	mocks.route = writable({ params: { id: 'group-a' } });
	return { page: mocks.route };
});

vi.mock('$lib/api/organization-admin', () => ({
	getOrganizationGroup: mocks.getGroup,
	listOrganizationUsers: mocks.listUsers,
	updateOrganizationGroup: mocks.updateGroup,
	addOrganizationGroupMember: mocks.addMember,
	removeOrganizationGroupMember: mocks.removeMember
}));

import GroupDetailPage from './[id]/+page.svelte';

describe('organization group detail', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mocks.route.set({ params: { id: 'group-a' } });
	});

	it('ignores a stale group response after navigation and edits the displayed group', async () => {
		let finishGroupA!: (value: unknown) => void;
		mocks.getGroup.mockImplementation((id: string) => {
			if (id === 'group-a') {
				return new Promise((resolve) => {
					finishGroupA = resolve;
				});
			}
			return Promise.resolve({
				id: 'group-b',
				name: 'Group B',
				description: null,
				members: [],
				total: 0,
				page: 1,
				per_page: 50
			});
		});
		render(GroupDetailPage);
		await waitFor(() => expect(mocks.getGroup).toHaveBeenCalledWith('group-a', expect.anything()));

		mocks.route.set({ params: { id: 'group-b' } });
		await screen.findByRole('heading', { name: 'Manage group: Group B' });
		finishGroupA({
			id: 'group-a',
			name: 'Group A',
			description: null,
			members: [],
			total: 0,
			page: 1,
			per_page: 50
		});
		await Promise.resolve();
		expect(screen.queryByRole('heading', { name: 'Manage group: Group A' })).toBeNull();

		await fireEvent.input(screen.getByLabelText('Name'), { target: { value: 'Updated B' } });
		await fireEvent.click(screen.getByRole('button', { name: 'Save group' }));
		await waitFor(() => expect(mocks.updateGroup).toHaveBeenCalledWith('group-b', 'Updated B', ''));
	});

	it('starts at page one after navigating from a larger group', async () => {
		mocks.getGroup.mockImplementation((id: string, params: { page: number }) =>
			Promise.resolve({
				id,
				name: id === 'group-a' ? 'Group A' : 'Group B',
				description: null,
				members: [],
				total: id === 'group-a' ? 60 : 1,
				page: params.page,
				per_page: 50
			})
		);
		render(GroupDetailPage);
		await screen.findByText('Page 1 of 2');
		await fireEvent.click(screen.getByRole('button', { name: 'Next' }));
		await screen.findByText('Page 2 of 2');

		mocks.route.set({ params: { id: 'group-b' } });
		await screen.findByRole('heading', { name: 'Manage group: Group B' });
		await waitFor(() =>
			expect(mocks.getGroup).toHaveBeenLastCalledWith('group-b', { page: 1, per_page: 50 })
		);
	});
});
