import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const api = vi.hoisted(() => ({
	listGroups: vi.fn(),
	createGroup: vi.fn(),
	deleteGroup: vi.fn()
}));

vi.mock('$lib/api/organization-admin', () => ({
	listOrganizationGroups: api.listGroups,
	createOrganizationGroup: api.createGroup,
	deleteOrganizationGroup: api.deleteGroup
}));

import GroupsPage from './+page.svelte';

const group = (id: string) => ({
	id,
	name: `Group ${id}`,
	description: null,
	member_count: 0,
	created_at: '2026-01-01T00:00:00Z'
});

describe('organization group list', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		api.listGroups.mockImplementation(({ page }: { page: number }) => {
			const total = api.deleteGroup.mock.calls.length ? 50 : 51;
			return Promise.resolve({
				groups:
					page === 2
						? total === 51
							? [group('last')]
							: []
						: Array.from({ length: 50 }, (_, i) => group(String(i))),
				total
			});
		});
		api.deleteGroup.mockResolvedValue(undefined);
		vi.stubGlobal(
			'confirm',
			vi.fn(() => true)
		);
	});

	it('returns to the last populated page after deleting its only group', async () => {
		render(GroupsPage);
		await screen.findByText('51 groups · Page 1 of 2');
		await fireEvent.click(screen.getByRole('button', { name: 'Next' }));
		await screen.findByText('Group last');
		await fireEvent.click(screen.getByRole('button', { name: 'Delete' }));

		await waitFor(() => expect(api.deleteGroup).toHaveBeenCalledWith('last'));
		await waitFor(() => expect(api.listGroups).toHaveBeenLastCalledWith({ page: 1, per_page: 50 }));
		await screen.findByText('50 groups · Page 1 of 1');
	});
});
