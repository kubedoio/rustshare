import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const api = vi.hoisted(() => ({
	listUsers: vi.fn(),
	bulkInvite: vi.fn(),
	offboard: vi.fn()
}));

vi.mock('$lib/api/organization-admin', () => ({
	listOrganizationUsers: api.listUsers,
	bulkInviteOrganizationUsers: api.bulkInvite,
	offboardOrganizationUsers: api.offboard
}));

import UsersPage from './users/+page.svelte';

describe('organization users operations', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		api.listUsers.mockResolvedValue({
			users: [
				{
					id: 'u1',
					display_name: 'Alex Example',
					username: 'alex',
					email: 'alex@example.org',
					disabled_at: null
				},
				{
					id: 'u2',
					display_name: 'Sam Sample',
					username: 'sam',
					email: 'sam@example.org',
					disabled_at: null
				},
				{
					id: 'u3',
					display_name: '',
					username: 'unnamed-user',
					email: 'unnamed@example.org',
					disabled_at: null
				}
			],
			total: 1
		});
		api.offboard.mockResolvedValue({ rows: [] });
	});

	it('shows user-list operation failures accessibly', async () => {
		api.listUsers.mockRejectedValue(new Error('request failed'));
		render(UsersPage);
		await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('request failed'));
	});

	it('requires a current dry run and shows SMTP apply failures', async () => {
		api.bulkInvite.mockResolvedValueOnce({ dry_run: true, rows: [{ row: 1, outcome: 'valid' }] });
		api.bulkInvite.mockRejectedValueOnce(new Error('SMTP unavailable'));
		render(UsersPage);
		await fireEvent.input(screen.getByLabelText('Email addresses'), {
			target: { value: 'alex@example.org' }
		});
		const apply = screen.getByRole('button', { name: 'Send invitations' }) as HTMLButtonElement;
		expect(apply.disabled).toBe(true);
		await fireEvent.click(screen.getByRole('button', { name: 'Dry run' }));
		await waitFor(() => expect(apply.disabled).toBe(false));
		await fireEvent.click(apply);
		await waitFor(() =>
			expect(screen.getByRole('alert').textContent).toContain('SMTP unavailable')
		);
	});

	it('identifies preview targets and does not present uncalculated impact as zero', async () => {
		Object.defineProperty(HTMLDialogElement.prototype, 'showModal', {
			configurable: true,
			value: function (this: HTMLDialogElement) {
				this.setAttribute('open', '');
			}
		});
		api.offboard.mockResolvedValueOnce({
			rows: [
				{
					user_id: 'u1',
					outcome: 'would_disable',
					shares_revoked: 0,
					group_memberships_removed: 0
				}
			]
		});
		render(UsersPage);
		await screen.findByText('Alex Example');
		expect(screen.getByRole('checkbox', { name: 'Select unnamed-user' })).toBeTruthy();
		await fireEvent.click(screen.getByRole('checkbox', { name: 'Select Alex Example' }));
		await fireEvent.click(screen.getByRole('button', { name: 'Preview offboarding' }));

		const dialog = screen.getByRole('dialog');
		await waitFor(() => expect(dialog.textContent).toContain('Alex Example'));
		expect(dialog.textContent).toContain('impact counts are not calculated during dry run');
		expect(dialog.textContent).not.toContain('0 shares revoked');
		await fireEvent.click(screen.getByRole('checkbox', { name: 'Select Sam Sample' }));
		await fireEvent.click(screen.getByRole('button', { name: 'Confirm disable' }));
		await waitFor(() => expect(api.offboard).toHaveBeenCalledTimes(2));
		expect(api.offboard).toHaveBeenNthCalledWith(2, ['u1'], false);
	});
});
