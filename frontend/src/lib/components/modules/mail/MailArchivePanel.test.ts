import { render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { queryClient } from '$lib/query-client';
import MailArchivePanel from './MailArchivePanel.svelte';

const mocks = vi.hoisted(() => ({
	listArchiveJobs: vi.fn(),
	createArchiveJob: vi.fn(),
	cancelArchiveJob: vi.fn()
}));

vi.mock('$lib/api/mail', () => ({ mailApi: mocks }));

const folder = (name: string) => ({
	name,
	display_name: name,
	delimiter: '/',
	role: null,
	unseen: 0,
	total: 0
});

describe('MailArchivePanel', () => {
	beforeEach(() => {
		queryClient.clear();
		vi.clearAllMocks();
		mocks.listArchiveJobs.mockResolvedValue([]);
	});

	it('selects a valid folder when the account changes', async () => {
		const { rerender } = render(MailArchivePanel, {
			accountId: 'account-a',
			folders: [folder('Inbox A')]
		});
		const select = screen.getByLabelText('Folder') as HTMLSelectElement;
		await waitFor(() => expect(select.value).toBe('Inbox A'));

		await rerender({ accountId: 'account-b', folders: [folder('Inbox B')] });

		await waitFor(() => expect(select.value).toBe('Inbox B'));
	});
});
