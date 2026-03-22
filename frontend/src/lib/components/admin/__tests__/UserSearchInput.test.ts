// @vitest-environment happy-dom
import { render, fireEvent, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest';
import UserSearchInput from '$lib/components/common/UserSearchInput.svelte';

vi.mock('$lib/api/admin', () => ({
	listAdminUsers: vi.fn()
}));

import { listAdminUsers } from '$lib/api/admin';

const alice = {
	id: 'u1',
	username: 'alice',
	email: 'alice@example.com',
	display_name: 'Alice',
	is_admin: false,
	storage_quota_bytes: 10737418240,
	disabled_at: null,
	created_at: '2026-01-01T00:00:00Z',
	updated_at: '2026-01-01T00:00:00Z'
};

const bob = {
	id: 'u2',
	username: 'bob',
	email: 'bob@example.com',
	display_name: 'Bob',
	is_admin: false,
	storage_quota_bytes: 10737418240,
	disabled_at: null,
	created_at: '2026-01-01T00:00:00Z',
	updated_at: '2026-01-01T00:00:00Z'
};

describe('UserSearchInput', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		vi.useFakeTimers();
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it('does not call API when input is empty', async () => {
		render(UserSearchInput);
		const input = screen.getByRole('textbox');
		await fireEvent.input(input, { target: { value: '' } });
		await vi.runAllTimersAsync();
		expect(listAdminUsers).not.toHaveBeenCalled();
	});

	it('does not call API before 300ms debounce delay', async () => {
		vi.mocked(listAdminUsers).mockResolvedValue({ users: [alice], total: 1 });
		render(UserSearchInput);
		const input = screen.getByRole('textbox');
		await fireEvent.input(input, { target: { value: 'ali' } });
		vi.advanceTimersByTime(299);
		expect(listAdminUsers).not.toHaveBeenCalled();
	});

	it('calls API with search term after 300ms delay', async () => {
		vi.mocked(listAdminUsers).mockResolvedValue({ users: [alice], total: 1 });
		render(UserSearchInput);
		const input = screen.getByRole('textbox');
		await fireEvent.input(input, { target: { value: 'ali' } });
		await vi.runAllTimersAsync();
		expect(listAdminUsers).toHaveBeenCalledWith({ search: 'ali', per_page: 10 });
	});

	it('renders search results in dropdown', async () => {
		vi.mocked(listAdminUsers).mockResolvedValue({ users: [alice, bob], total: 2 });
		render(UserSearchInput);
		const input = screen.getByRole('textbox');
		await fireEvent.input(input, { target: { value: 'user' } });
		await vi.runAllTimersAsync();
		await waitFor(() => {
			expect(screen.getByText('alice')).toBeDefined();
			expect(screen.getByText('alice@example.com')).toBeDefined();
			expect(screen.getByText('bob')).toBeDefined();
		});
	});

	it('filters out excluded user IDs from results', async () => {
		vi.mocked(listAdminUsers).mockResolvedValue({ users: [alice, bob], total: 2 });
		render(UserSearchInput, { excludeIds: ['u1'] });
		const input = screen.getByRole('textbox');
		await fireEvent.input(input, { target: { value: 'user' } });
		await vi.runAllTimersAsync();
		await waitFor(() => {
			expect(screen.queryByText('alice')).toBeNull();
			expect(screen.getByText('bob')).toBeDefined();
		});
	});

	it('calls onselect with the chosen user', async () => {
		vi.mocked(listAdminUsers).mockResolvedValue({ users: [alice], total: 1 });
		const selectHandler = vi.fn();
		render(UserSearchInput, { onselect: selectHandler });

		const input = screen.getByRole('textbox');
		await fireEvent.input(input, { target: { value: 'ali' } });
		await vi.runAllTimersAsync();

		await waitFor(() => screen.getByText('alice'));
		await fireEvent.mouseDown(screen.getByRole('button', { name: /alice/i }));

		expect(selectHandler).toHaveBeenCalledWith(alice);
	});

	it('clears input and closes dropdown after selection', async () => {
		vi.mocked(listAdminUsers).mockResolvedValue({ users: [alice], total: 1 });
		render(UserSearchInput);
		const input = screen.getByRole('textbox') as HTMLInputElement;
		await fireEvent.input(input, { target: { value: 'ali' } });
		await vi.runAllTimersAsync();

		await waitFor(() => screen.getByText('alice'));
		await fireEvent.mouseDown(screen.getByRole('button', { name: /alice/i }));

		expect(screen.queryByText('alice')).toBeNull();
		expect(input.value).toBe('');
	});

	it('only fires one API call for rapid successive inputs (debounce reset)', async () => {
		vi.mocked(listAdminUsers).mockResolvedValue({ users: [alice], total: 1 });
		render(UserSearchInput);
		const input = screen.getByRole('textbox');

		await fireEvent.input(input, { target: { value: 'a' } });
		vi.advanceTimersByTime(100);
		await fireEvent.input(input, { target: { value: 'al' } });
		vi.advanceTimersByTime(100);
		await fireEvent.input(input, { target: { value: 'ali' } });
		await vi.runAllTimersAsync();

		expect(listAdminUsers).toHaveBeenCalledTimes(1);
		expect(listAdminUsers).toHaveBeenCalledWith({ search: 'ali', per_page: 10 });
	});

	it('accepts a custom placeholder prop', () => {
		render(UserSearchInput, { placeholder: 'Find a member...' });
		expect(screen.getByPlaceholderText('Find a member...')).toBeDefined();
	});
});
