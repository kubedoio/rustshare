import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { queryClient } from '$lib/query-client';
import MailModuleView from './MailModuleView.svelte';

const mocks = vi.hoisted(() => ({
	goto: vi.fn(),
	listMessages: vi.fn(),
	listMessagesPage: vi.fn(),
	listAccounts: vi.fn(),
	listFolders: vi.fn(),
	listAccountMessages: vi.fn(),
	listArchiveJobs: vi.fn(),
	listImportJobs: vi.fn(),
	listDrafts: vi.fn(),
	getDraft: vi.fn(),
	createAccount: vi.fn(),
	deleteAccount: vi.fn(),
	testAccount: vi.fn(),
	createImportJob: vi.fn(),
	createArchiveJob: vi.fn(),
	cancelArchiveJob: vi.fn(),
	uploadMessage: vi.fn(),
	sendMessage: vi.fn(),
	sendOutboundMail: vi.fn(),
	getSmtpSettings: vi.fn(),
	updateSmtpSettings: vi.fn(),
	deleteSmtpSettings: vi.fn(),
	testSmtpConnection: vi.fn()
}));

vi.mock('$app/navigation', () => ({
	goto: mocks.goto
}));

vi.mock('$lib/api/files', () => ({
	listAllFiles: vi.fn().mockResolvedValue([])
}));

vi.mock('$lib/api/mail', () => ({
	mailApi: {
		listMessages: mocks.listMessages,
		listMessagesPage: mocks.listMessagesPage,
		listAccounts: mocks.listAccounts,
		listFolders: mocks.listFolders,
		listAccountMessages: mocks.listAccountMessages,
		listArchiveJobs: mocks.listArchiveJobs,
		listImportJobs: mocks.listImportJobs,
		listDrafts: mocks.listDrafts,
		getDraft: mocks.getDraft,
		createAccount: mocks.createAccount,
		deleteAccount: mocks.deleteAccount,
		testAccount: mocks.testAccount,
		createImportJob: mocks.createImportJob,
		createArchiveJob: mocks.createArchiveJob,
		cancelArchiveJob: mocks.cancelArchiveJob,
		uploadMessage: mocks.uploadMessage,
		sendMessage: mocks.sendMessage,
		sendOutboundMail: mocks.sendOutboundMail,
		getSmtpSettings: mocks.getSmtpSettings,
		updateSmtpSettings: mocks.updateSmtpSettings,
		deleteSmtpSettings: mocks.deleteSmtpSettings,
		testSmtpConnection: mocks.testSmtpConnection
	}
}));

const testModule = {
	id: 'module_mail',
	key: 'mail',
	displayName: 'Mail',
	description: 'Import, archive, and reference email.',
	enabled: true,
	rootPath: '/Workspace/Mail',
	renderer: 'mail-list',
	defaultTemplate: null,
	icon: 'mail',
	schemaVersion: '1.0',
	permissions: {
		adminCanConfigure: true,
		workspaceMembersCanUse: true,
		allowPublicShare: false,
		allowInternalShare: true
	},
	ui: {
		sidebar: { enabled: true, order: 65, icon: 'mail', label: 'Mail' },
		dashboard: {
			enabled: true,
			order: 65,
			widget: {
				enabled: true,
				type: 'mail-summary',
				title: 'Mail',
				description: 'Imported messages.',
				size: 'small' as const,
				columns: { desktop: 3, tablet: 6, mobile: 12 },
				maxItems: 0,
				primaryAction: { label: 'Import mail', action: 'generic-create' }
			}
		},
		page: {
			enabled: true,
			route: '/modules/mail',
			renderer: 'mail-list',
			layout: 'list-grid',
			emptyStateTitle: 'No imported mail yet',
			emptyStateDescription: 'No imported mail yet.',
			primaryAction: { label: 'Import mail', action: 'generic-create' },
			searchPlaceholder: 'Search messages...',
			filterLabel: 'All messages',
			sortLabel: 'Imported',
			itemSingular: 'message',
			itemPlural: 'messages'
		}
	},
	aiIndexing: { enabled: false },
	audit: { enabled: true }
};

describe('MailModuleView', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		queryClient.clear();
		mocks.listMessages.mockResolvedValue([]);
		mocks.listMessagesPage.mockResolvedValue({
			messages: [],
			next_cursor_at: null,
			next_cursor_id: null
		});
		mocks.listAccounts.mockResolvedValue([
			{
				id: 'acct-1',
				name: 'Work mail',
				host: 'imap.example.com',
				port: 993,
				username: 'alice@example.com',
				tls_mode: 'tls',
				is_enabled: true,
				last_connected_at: null,
				last_error: null,
				created_at: '2026-07-01T00:00:00Z'
			}
		]);
		mocks.listFolders.mockResolvedValue([]);
		mocks.listAccountMessages.mockResolvedValue({ uidvalidity: null, messages: [] });
		mocks.listArchiveJobs.mockResolvedValue([]);
		mocks.listDrafts.mockResolvedValue([]);
		mocks.getDraft.mockResolvedValue({
			message: {
				id: 'draft-1',
				account_id: 'acct-1',
				subject: 'Draft subject',
				from_address: 'alice@example.com',
				from_name: 'Alice',
				to_addresses: ['bob@example.com'],
				cc_addresses: [],
				bcc_addresses: [],
				sent_at: null,
				imported_at: '2026-07-01T12:00:00Z',
				size_bytes: 0,
				has_attachments: false,
				source_mode: 'draft'
			},
			body: 'Draft body',
			attachments: []
		});
		mocks.uploadMessage.mockResolvedValue({ id: 'msg-uploaded' });
		mocks.sendMessage.mockResolvedValue({ ok: true });
		mocks.listImportJobs.mockResolvedValue([]);
		mocks.sendOutboundMail.mockResolvedValue({
			message_id: 'outbound-1',
			stored: true,
			append_failed: false
		});
		mocks.getSmtpSettings.mockResolvedValue({
			id: 'smtp-1',
			host: 'smtp.example.com',
			port: 587,
			username: 'alice@example.com',
			tls_mode: 'tls',
			from_address: 'alice@example.com',
			is_enabled: true
		});
	});

	it('renders message subject rows and navigates on click', async () => {
		mocks.listMessagesPage.mockResolvedValueOnce({
			messages: [
				{
					id: 'msg-1',
					subject: 'Quarterly update',
					from_address: 'alice@example.com',
					from_name: 'Alice',
					to_addresses: ['bob@example.com'],
					cc_addresses: [],
					bcc_addresses: [],
					sent_at: '2026-07-01T10:00:00Z',
					imported_at: '2026-07-01T12:00:00Z',
					size_bytes: 1024,
					has_attachments: false
				}
			],
			next_cursor_at: null,
			next_cursor_id: null
		});

		render(MailModuleView, { module: testModule });

		await fireEvent.click(await screen.findByRole('tab', { name: 'Imported' }));
		const row = await screen.findByText('Quarterly update');
		expect(row).toBeTruthy();

		await fireEvent.click(row.closest('button')!);

		await waitFor(() => {
			expect(mocks.goto).toHaveBeenCalledWith('/modules/mail/messages/msg-1');
		});
	});

	it('renders empty state when no messages exist', async () => {
		mocks.listMessages.mockResolvedValueOnce([]);

		render(MailModuleView, { module: testModule });

		await fireEvent.click(await screen.findByRole('tab', { name: 'Imported' }));
		const emptyTitle = await screen.findByText('No imported mail yet');
		expect(emptyTitle).toBeTruthy();
	});

	it('renders empty drafts state', async () => {
		mocks.listMessages.mockResolvedValueOnce([]);
		mocks.listDrafts.mockResolvedValueOnce([]);

		render(MailModuleView, { module: testModule });

		await fireEvent.click(await screen.findByRole('tab', { name: /Drafts/ }));
		expect(await screen.findByText('No drafts')).toBeTruthy();
	});

	it('renders accounts, folders, and mailbox messages', async () => {
		mocks.listAccounts.mockResolvedValueOnce([
			{
				id: 'acct-1',
				name: 'Work mail',
				host: 'imap.example.com',
				port: 993,
				username: 'alice@example.com',
				tls_mode: 'tls',
				is_enabled: true,
				last_connected_at: null,
				last_error: null,
				created_at: '2026-07-01T00:00:00Z'
			}
		]);
		mocks.listFolders.mockResolvedValue([
			{ name: 'INBOX', display_name: 'INBOX', delimiter: '/', role: null }
		]);
		mocks.listAccountMessages.mockResolvedValue({
			uidvalidity: 7,
			messages: [
				{
					uid: 12,
					subject: 'Server alert',
					from_address: 'ops@example.com',
					from_name: 'Ops',
					sent_at: '2026-07-01T10:00:00Z',
					size_bytes: 2048
				}
			]
		});
		mocks.listMessages.mockResolvedValueOnce([]);

		render(MailModuleView, { module: testModule });

		expect(await screen.findByText('Work mail')).toBeTruthy();
		expect(await screen.findByText('INBOX')).toBeTruthy();
		expect(await screen.findByText('Server alert')).toBeTruthy();
	});

	it('gates archive and trash actions on their own folders', async () => {
		mocks.listFolders.mockResolvedValue([
			{ name: 'INBOX', display_name: 'INBOX', delimiter: '/', role: null },
			{ name: 'Archive', display_name: 'Archive', delimiter: '/', role: 'archive' }
		]);
		mocks.listAccountMessages.mockResolvedValue({
			uidvalidity: 7,
			messages: [
				{
					uid: 12,
					subject: 'Server alert',
					from_address: 'ops@example.com',
					from_name: 'Ops',
					sent_at: '2026-07-01T10:00:00Z',
					size_bytes: 2048
				}
			]
		});

		render(MailModuleView, { module: testModule });

		const archiveButton = (await screen.findByTitle('Archive')) as HTMLButtonElement;
		expect(archiveButton.disabled).toBe(false);
		const trashButton = (await screen.findByTitle(
			'No trash folder is configured'
		)) as HTMLButtonElement;
		expect(trashButton.disabled).toBe(true);
	});

	it('disables archive and enables trash when only a trash folder exists', async () => {
		mocks.listFolders.mockResolvedValue([
			{ name: 'INBOX', display_name: 'INBOX', delimiter: '/', role: null },
			{ name: 'Trash', display_name: 'Trash', delimiter: '/', role: 'trash' }
		]);
		mocks.listAccountMessages.mockResolvedValue({
			uidvalidity: 7,
			messages: [
				{
					uid: 12,
					subject: 'Server alert',
					from_address: 'ops@example.com',
					from_name: 'Ops',
					sent_at: '2026-07-01T10:00:00Z',
					size_bytes: 2048
				}
			]
		});

		render(MailModuleView, { module: testModule });

		const archiveButton = (await screen.findByTitle(
			'No archive folder is configured'
		)) as HTMLButtonElement;
		expect(archiveButton.disabled).toBe(true);
		const trashButton = (await screen.findByTitle('Move to trash')) as HTMLButtonElement;
		expect(trashButton.disabled).toBe(false);
	});

	it('uploads .eml files through the mail import endpoint', async () => {
		mocks.listMessages.mockResolvedValueOnce([]);

		render(MailModuleView, { module: testModule });

		await fireEvent.click(await screen.findByRole('tab', { name: 'Imported' }));
		await screen.findByText('No imported mail yet');

		const input = document.querySelector('input[type="file"]') as HTMLInputElement;
		const file = new File(['From: alice@example.com'], 'message.eml', { type: 'message/rfc822' });

		await fireEvent.change(input, { target: { files: [file] } });

		await waitFor(() => {
			expect(mocks.uploadMessage).toHaveBeenCalledWith(file);
			expect(mocks.listMessagesPage).toHaveBeenCalledTimes(2);
		});
	});

	it('opens compose and sends outbound mail', async () => {
		const testAccount = {
			id: 'acct-1',
			tenant_id: 'tenant-1',
			owner_id: 'user-1',
			name: 'Test Account',
			host: 'imap.example.com',
			port: 993,
			username: 'testuser',
			tls_mode: 'tls',
			created_at: '',
			updated_at: ''
		};
		mocks.listAccounts.mockResolvedValueOnce([testAccount]);
		mocks.listMessages.mockResolvedValueOnce([]);
		mocks.getSmtpSettings.mockResolvedValueOnce({
			id: 'smtp-1',
			host: 'smtp.example.com',
			port: 587,
			username: 'testuser',
			tls_mode: 'tls',
			from_address: 'test@example.com',
			is_enabled: true
		});

		render(MailModuleView, { module: testModule });

		// Wait for accounts to load
		await waitFor(() => {
			expect(screen.queryAllByText('Test Account').length).toBeGreaterThan(0);
		});

		await fireEvent.click(await screen.findByText('Compose'));
		await fireEvent.input(screen.getByPlaceholderText('To'), {
			target: { value: 'bob@example.com' }
		});
		await fireEvent.input(screen.getByPlaceholderText('Subject'), {
			target: { value: 'Hello' }
		});
		const editor = document.querySelector('.ProseMirror') as HTMLElement;
		editor.innerHTML = '<p>Hi Bob</p>';
		await new Promise((resolve) => setTimeout(resolve, 50));
		await fireEvent.submit(document.querySelector('.mail-body-editor')!.closest('form')!);

		await waitFor(() => {
			expect(mocks.sendOutboundMail.mock.calls[0][1]).toEqual(
				expect.objectContaining({
					to: ['bob@example.com'],
					cc: [],
					bcc: [],
					subject: 'Hello',
					body: 'Hi Bob',
					attachments: [],
					in_reply_to_msg_id: null
				})
			);
			expect(mocks.sendOutboundMail.mock.calls[0][1].idempotency_key).toMatch(/^[0-9a-f-]{36}$/);
			expect(mocks.listMessagesPage).toHaveBeenCalledTimes(2);
		});
	});

	it('opens an existing draft into compose', async () => {
		mocks.listMessages.mockResolvedValueOnce([]);
		mocks.listDrafts.mockResolvedValueOnce([
			{
				id: 'draft-1',
				account_id: 'acct-1',
				subject: 'Draft subject',
				from_address: 'alice@example.com',
				from_name: 'Alice',
				to_addresses: ['bob@example.com'],
				cc_addresses: [],
				bcc_addresses: [],
				sent_at: null,
				imported_at: '2026-07-01T12:00:00Z',
				size_bytes: 0,
				has_attachments: false,
				source_mode: 'draft'
			}
		]);

		render(MailModuleView, { module: testModule });

		await fireEvent.click(await screen.findByRole('tab', { name: /Drafts/ }));
		await fireEvent.click(await screen.findByText('Draft subject'));

		await waitFor(() => {
			expect(mocks.getDraft).toHaveBeenCalledWith('acct-1', 'draft-1');
		});
		const toInput = await screen.findByPlaceholderText('To');
		const subjectInput = await screen.findByPlaceholderText('Subject');
		expect((toInput as HTMLInputElement).value).toBe('bob@example.com');
		expect((subjectInput as HTMLInputElement).value).toBe('Draft subject');
	});
});
