import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { queryClient } from '$lib/query-client';
import MailModuleView from './MailModuleView.svelte';

const mocks = vi.hoisted(() => ({
	goto: vi.fn(),
	listMessagesPage: vi.fn(),
	listAccounts: vi.fn(),
	listFolders: vi.fn(),
	listAccountMessages: vi.fn(),
	listArchiveJobs: vi.fn(),
	listImportJobs: vi.fn(),
	listDrafts: vi.fn(),
	getDraft: vi.fn(),
	getMessage: vi.fn(),
	listParts: vi.fn(),
	getPartContent: vi.fn(),
	listAttachments: vi.fn(),
	listLinks: vi.fn(),
	createImportJob: vi.fn(),
	uploadMessage: vi.fn(),
	sendOutboundMail: vi.fn(),
	updateDraft: vi.fn(),
	sendDraft: vi.fn(),
	getSmtpSettings: vi.fn(),
	listAllFiles: vi.fn()
}));

vi.mock('$app/navigation', () => ({
	goto: mocks.goto
}));

vi.mock('$lib/api/files', () => ({
	listAllFiles: mocks.listAllFiles,
	previewFile: vi.fn(),
	downloadFile: vi.fn(),
	getFileContent: vi.fn()
}));

vi.mock('$lib/api/mail', () => ({
	mailApi: {
		listMessagesPage: mocks.listMessagesPage,
		listAccounts: mocks.listAccounts,
		listFolders: mocks.listFolders,
		listAccountMessages: mocks.listAccountMessages,
		listArchiveJobs: mocks.listArchiveJobs,
		listImportJobs: mocks.listImportJobs,
		listDrafts: mocks.listDrafts,
		getDraft: mocks.getDraft,
		getMessage: mocks.getMessage,
		listParts: mocks.listParts,
		getPartContent: mocks.getPartContent,
		listAttachments: mocks.listAttachments,
		listLinks: mocks.listLinks,
		createImportJob: mocks.createImportJob,
		uploadMessage: mocks.uploadMessage,
		sendOutboundMail: mocks.sendOutboundMail,
		updateDraft: mocks.updateDraft,
		sendDraft: mocks.sendDraft,
		getSmtpSettings: mocks.getSmtpSettings,
		downloadSourceUrl: (id: string) => `/api/mail/messages/${id}/source`
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

const testAccount = {
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
};

const importedMessage = {
	id: 'msg-1',
	account_id: 'acct-1',
	subject: 'Quarterly update',
	from_address: 'alice@example.com',
	from_name: 'Alice',
	to_addresses: ['bob@example.com'],
	cc_addresses: [],
	bcc_addresses: [],
	sent_at: '2026-07-01T10:00:00Z',
	imported_at: '2026-07-01T12:00:00Z',
	size_bytes: 1024,
	has_attachments: false,
	is_seen: true,
	source_mode: 'imap_selected'
};

describe('MailModuleView workspace', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		queryClient.clear();
		mocks.listAccounts.mockResolvedValue([testAccount]);
		mocks.listFolders.mockResolvedValue([]);
		mocks.listAccountMessages.mockResolvedValue({ uidvalidity: null, messages: [] });
		mocks.listArchiveJobs.mockResolvedValue([]);
		mocks.listImportJobs.mockResolvedValue([]);
		mocks.listDrafts.mockResolvedValue([]);
		mocks.listMessagesPage.mockResolvedValue({
			messages: [],
			next_cursor_at: null,
			next_cursor_id: null
		});
		mocks.getMessage.mockResolvedValue(importedMessage);
		mocks.listParts.mockResolvedValue([
			{
				id: 'part-1',
				part_index: 1,
				content_type: 'text/plain',
				charset: 'utf-8',
				size_bytes: 12,
				is_body: true
			}
		]);
		mocks.getPartContent.mockResolvedValue('Hello from the body');
		mocks.listAttachments.mockResolvedValue([]);
		mocks.listLinks.mockResolvedValue([]);
		mocks.listAllFiles.mockResolvedValue([]);
		mocks.uploadMessage.mockResolvedValue(importedMessage);
		mocks.sendOutboundMail.mockResolvedValue({
			message_id: 'outbound-1',
			stored: true,
			append_failed: false
		});
		mocks.updateDraft.mockResolvedValue({ ...importedMessage, id: 'draft-2' });
		mocks.sendDraft.mockResolvedValue({
			message_id: 'outbound-2',
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
		mocks.getDraft.mockResolvedValue({
			message: {
				...importedMessage,
				id: 'draft-1',
				subject: 'Draft subject',
				source_mode: 'draft'
			},
			body: 'Draft body',
			attachments: []
		});
	});

	it('renders toolbar, folder pane, and imported messages; opens a message in the viewer', async () => {
		mocks.listMessagesPage.mockResolvedValue({
			messages: [importedMessage],
			next_cursor_at: null,
			next_cursor_id: null
		});

		render(MailModuleView, { module: testModule });

		// Toolbar
		expect(await screen.findByText('Work mail')).toBeTruthy();
		expect(screen.getByText('Compose')).toBeTruthy();
		// Folder pane shows local folders
		expect(screen.getByRole('option', { name: /Imported/ })).toBeTruthy();
		expect(screen.getByRole('option', { name: /Drafts/ })).toBeTruthy();
		// Message row
		const row = await screen.findByText('Quarterly update');
		await fireEvent.click(row.closest('button')!);

		// Viewer opens in place (no navigation) and loads the body
		await waitFor(() => {
			expect(mocks.getMessage).toHaveBeenCalledWith('msg-1');
		});
		expect(await screen.findByText('Hello from the body')).toBeTruthy();
		expect(screen.getByText('Reply')).toBeTruthy();
		expect(mocks.goto).not.toHaveBeenCalled();
	});

	it('shows an inline folder error and keeps imported mail usable', async () => {
		mocks.listFolders.mockRejectedValue(new Error('IMAP unreachable'));
		mocks.listMessagesPage.mockResolvedValue({
			messages: [importedMessage],
			next_cursor_at: null,
			next_cursor_id: null
		});

		render(MailModuleView, { module: testModule });

		expect(
			await screen.findByText('Folders could not be refreshed.', {}, { timeout: 8000 })
		).toBeTruthy();
		// Imported messages still render despite the folder failure
		expect(await screen.findByText('Quarterly update')).toBeTruthy();
	});

	it('lists drafts as a folder and opens a draft into compose', async () => {
		mocks.listDrafts.mockResolvedValue([
			{
				...importedMessage,
				id: 'draft-1',
				subject: 'Draft subject',
				source_mode: 'draft'
			}
		]);

		render(MailModuleView, { module: testModule });

		await fireEvent.click(await screen.findByRole('option', { name: /Drafts/ }));
		await fireEvent.click(await screen.findByText('Draft subject'));

		await waitFor(() => {
			expect(mocks.getDraft).toHaveBeenCalledWith('acct-1', 'draft-1');
		});
		const toInput = await screen.findByPlaceholderText('To');
		expect((toInput as HTMLInputElement).value).toBe('bob@example.com');
	});

	it('lists remote IMAP messages after selecting a folder', async () => {
		mocks.listFolders.mockResolvedValue([
			{ name: 'INBOX', display_name: 'INBOX', delimiter: '/', role: null }
		]);
		mocks.listAccountMessages.mockResolvedValue({
			uidvalidity: 7,
			next_cursor: null,
			messages: [
				{
					uid: 12,
					subject: 'Server alert',
					from_address: 'ops@example.com',
					from_name: 'Ops',
					sent_at: '2026-07-01T10:00:00Z',
					size_bytes: 2048,
					is_seen: false
				}
			]
		});

		render(MailModuleView, { module: testModule });

		await fireEvent.click(await screen.findByRole('option', { name: 'INBOX' }));
		const row = await screen.findByText('Server alert');
		expect(row).toBeTruthy();

		await fireEvent.click(row.closest('button')!);
		expect(await screen.findByText('Save to workspace')).toBeTruthy();
	});

	it('uploads .eml files through the mail import endpoint', async () => {
		render(MailModuleView, { module: testModule });
		await screen.findByRole('option', { name: /Imported/ });

		const input = document.querySelector('input[type="file"]') as HTMLInputElement;
		const file = new File(['From: alice@example.com'], 'message.eml', {
			type: 'message/rfc822'
		});
		await fireEvent.change(input, { target: { files: [file] } });

		await waitFor(() => {
			expect(mocks.uploadMessage).toHaveBeenCalledWith(file);
		});
	});

	it('opens compose from the toolbar and sends outbound mail', async () => {
		render(MailModuleView, { module: testModule });

		await fireEvent.click(await screen.findByText('Compose'));
		await fireEvent.input(screen.getByPlaceholderText('To'), {
			target: { value: 'bob@example.com' }
		});
		await fireEvent.input(screen.getByPlaceholderText('Subject'), {
			target: { value: 'Hello' }
		});
		// The body is a rich-text (ProseMirror) editor; seed its DOM and let the
		// editor's mutation observer sync it back to the draft.
		const editorEl = document.querySelector('.ProseMirror') as HTMLElement;
		editorEl.innerHTML = '<p>Hi Bob</p>';
		await new Promise((resolve) => setTimeout(resolve, 50));
		await fireEvent.submit(document.querySelector('.mail-body-editor')!.closest('form')!);

		await waitFor(() => {
			expect(mocks.sendOutboundMail.mock.calls[0][1]).toEqual(
				expect.objectContaining({
					to: ['bob@example.com'],
					subject: 'Hello',
					body: 'Hi Bob',
					body_html: '<p>Hi Bob</p>'
				})
			);
		});
	});

	it('sends an edited draft via the draft id returned by the save', async () => {
		// The backend replaces the draft row on update, so the send must use
		// the id returned by updateDraft ('draft-2'), not the opened id.
		mocks.listDrafts.mockResolvedValue([
			{
				...importedMessage,
				id: 'draft-1',
				subject: 'Draft subject',
				source_mode: 'draft'
			}
		]);

		render(MailModuleView, { module: testModule });

		await fireEvent.click(await screen.findByRole('option', { name: /Drafts/ }));
		await fireEvent.click(await screen.findByText('Draft subject'));
		await screen.findByDisplayValue('Draft subject');

		await fireEvent.submit(screen.getByRole('button', { name: 'Send' }).closest('form')!);

		await waitFor(() => {
			expect(mocks.updateDraft).toHaveBeenCalledWith(
				'acct-1',
				'draft-1',
				expect.objectContaining({ subject: 'Draft subject' })
			);
			expect(mocks.sendDraft).toHaveBeenCalledWith('acct-1', 'draft-2');
		});
	});
});
