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
		api.offboard
			.mockResolvedValueOnce({
				rows: [
					{
						user_id: 'u1',
						outcome: 'would_disable',
						shares_revoked: 0,
						group_memberships_removed: 0
					}
				]
			})
			.mockResolvedValueOnce({
				rows: [
					{
						user_id: 'u1',
						outcome: 'disabled',
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
		const applied = screen.getByRole('button', { name: 'Applied' }) as HTMLButtonElement;
		expect(applied.disabled).toBe(true);
		await fireEvent.click(applied);
		expect(api.offboard).toHaveBeenCalledTimes(2);
	});

	it('returns to page one after offboarding shrinks the active list', async () => {
		Object.defineProperty(HTMLDialogElement.prototype, 'showModal', {
			configurable: true,
			value: function (this: HTMLDialogElement) {
				this.setAttribute('open', '');
			}
		});
		api.listUsers.mockImplementation(({ page }: { page: number }) =>
			Promise.resolve(
				page === 2
					? {
							users: [
								{
									id: 'last',
									display_name: 'Last Member',
									username: 'last',
									email: 'last@example.org',
									disabled_at: null
								}
							],
							total: 51
						}
					: { users: [], total: api.offboard.mock.calls.length >= 2 ? 50 : 51 }
			)
		);
		api.offboard
			.mockResolvedValueOnce({ rows: [{ user_id: 'last', outcome: 'would_disable' }] })
			.mockResolvedValueOnce({ rows: [{ user_id: 'last', outcome: 'disabled' }] });
		render(UsersPage);
		await screen.findByText('51 users · Page 1 of 2');
		await fireEvent.click(screen.getByRole('button', { name: 'Next' }));
		await screen.findByText('Last Member');
		await fireEvent.click(screen.getByRole('checkbox', { name: 'Select Last Member' }));
		await fireEvent.click(screen.getByRole('button', { name: 'Preview offboarding' }));
		await screen.findByRole('dialog');
		await fireEvent.click(screen.getByRole('button', { name: 'Confirm disable' }));
		await waitFor(() =>
			expect(api.listUsers).toHaveBeenLastCalledWith(
				expect.objectContaining({ page: 1, status: 'all' })
			)
		);
	});

	it('resets status pagination and ignores a stale result from the previous filter', async () => {
		let finishPageTwo!: (result: { users: unknown[]; total: number }) => void;
		api.listUsers.mockImplementation(({ page, status }: { page: number; status: string }) => {
			if (status === 'all' && page === 2) {
				return new Promise((resolve) => {
					finishPageTwo = resolve;
				});
			}
			return Promise.resolve({
				users:
					status === 'disabled'
						? [
								{
									id: 'disabled',
									display_name: 'Disabled Member',
									username: 'disabled',
									email: 'disabled@example.org',
									disabled_at: '2026-01-01T00:00:00Z'
								}
							]
						: [],
				total: status === 'disabled' ? 1 : 100
			});
		});
		render(UsersPage);
		await screen.findByText('100 users · Page 1 of 2');
		await fireEvent.click(screen.getByRole('button', { name: 'Next' }));
		await waitFor(() =>
			expect(api.listUsers).toHaveBeenCalledWith(expect.objectContaining({ page: 2 }))
		);
		await fireEvent.change(screen.getByLabelText('User status'), { target: { value: 'disabled' } });
		await screen.findByText('Disabled Member');
		expect(api.listUsers).toHaveBeenLastCalledWith(
			expect.objectContaining({ page: 1, status: 'disabled' })
		);

		finishPageTwo({
			users: [
				{
					id: 'stale',
					display_name: 'Stale Member',
					username: 'stale',
					email: 'stale@example.org',
					disabled_at: null
				}
			],
			total: 100
		});
		await Promise.resolve();
		expect(screen.queryByText('Stale Member')).toBeNull();
		expect(screen.getByText('Disabled Member')).toBeTruthy();
	});
});
