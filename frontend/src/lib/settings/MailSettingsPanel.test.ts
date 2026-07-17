import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { queryClient } from '$lib/query-client';
import MailSettingsPanel from './MailSettingsPanel.svelte';

const mocks = vi.hoisted(() => ({
	listAccounts: vi.fn(),
	getSmtpSettings: vi.fn(),
	updateAccount: vi.fn(),
	updateSmtpSettings: vi.fn(),
	createAccount: vi.fn(),
	listFolders: vi.fn(),
	listArchiveJobs: vi.fn(),
	testAccount: vi.fn(),
	testSmtpConnection: vi.fn()
}));

vi.mock('$lib/api/mail', () => ({
	mailApi: {
		listAccounts: mocks.listAccounts,
		getSmtpSettings: mocks.getSmtpSettings,
		updateAccount: mocks.updateAccount,
		updateSmtpSettings: mocks.updateSmtpSettings,
		createAccount: mocks.createAccount,
		listFolders: mocks.listFolders,
		listArchiveJobs: mocks.listArchiveJobs,
		testAccount: mocks.testAccount,
		testSmtpConnection: mocks.testSmtpConnection
	}
}));

const testAccount = {
	id: 'acct-1',
	name: 'Work mail',
	host: 'imap.example.com',
	port: 993,
	username: 'alice@example.com',
	tls_mode: 'tls',
	is_enabled: true,
	last_connected_at: '2026-07-01T10:00:00Z',
	last_error: null,
	created_at: '2026-07-01T00:00:00Z'
};

describe('MailSettingsPanel', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		queryClient.clear();
		mocks.listAccounts.mockResolvedValue([testAccount]);
		mocks.getSmtpSettings.mockResolvedValue({
			id: 'smtp-1',
			host: 'smtp.example.com',
			port: 587,
			username: 'alice@example.com',
			tls_mode: 'starttls',
			from_address: 'alice@example.com',
			from_name: 'Alice',
			reply_to: null,
			sent_folder: 'Sent',
			is_enabled: true
		});
		mocks.listFolders.mockResolvedValue([]);
		mocks.listArchiveJobs.mockResolvedValue([]);
		mocks.updateAccount.mockResolvedValue(testAccount);
		mocks.updateSmtpSettings.mockResolvedValue({});
		mocks.createAccount.mockResolvedValue(testAccount);
	});

	it('renders the account list and selected account with saved-password state', async () => {
		render(MailSettingsPanel);

		expect(await screen.findByText('Mail accounts')).toBeTruthy();
		expect(await screen.findAllByText('Work mail')).not.toHaveLength(0);
		// Unified status + sticky footer actions
		expect(await screen.findByText('Connected')).toBeTruthy();
		expect(screen.getByText('Test incoming mail')).toBeTruthy();
		expect(screen.getByText('Test outgoing mail')).toBeTruthy();
		expect(screen.getByText('Save changes')).toBeTruthy();
		// Saved passwords are not shown as empty inputs with misleading placeholders
		expect(screen.getAllByText('Saved').length).toBeGreaterThan(0);
		expect(screen.queryByPlaceholderText(/keep unchanged/i)).toBeNull();
	});

	it('reveals a password input only after choosing Replace password', async () => {
		render(MailSettingsPanel);

		await screen.findByText('Connected');
		expect(screen.queryByPlaceholderText('Enter new password')).toBeNull();

		await fireEvent.click(screen.getAllByText('Replace password')[0]);
		expect(screen.getByPlaceholderText('Enter new password')).toBeTruthy();
	});

	it('saves IMAP and SMTP settings together with one Save changes action', async () => {
		render(MailSettingsPanel);

		await screen.findByText('Connected');
		await fireEvent.click(screen.getByText('Save changes'));

		await waitFor(() => {
			expect(mocks.updateAccount).toHaveBeenCalledWith(
				'acct-1',
				expect.objectContaining({ host: 'imap.example.com', password: undefined })
			);
			expect(mocks.updateSmtpSettings).toHaveBeenCalledWith(
				'acct-1',
				expect.objectContaining({ host: 'smtp.example.com', password: null })
			);
		});
	});

	it('prefills Gmail defaults from the provider preset', async () => {
		render(MailSettingsPanel);

		await fireEvent.click(await screen.findByText('Add account'));
		await fireEvent.change(screen.getByLabelText('Provider'), { target: { value: 'gmail' } });

		expect((screen.getByLabelText('IMAP host') as HTMLInputElement).value).toBe('imap.gmail.com');
		expect((screen.getByLabelText('Port') as HTMLInputElement).value).toBe('993');
	});
});
