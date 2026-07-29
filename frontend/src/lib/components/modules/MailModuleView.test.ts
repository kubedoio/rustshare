import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { get } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { queryClient } from '$lib/query-client';
import { toastStore } from '$lib/stores/toast';
import type { ModuleDefinition } from '$lib/modules/registry';
import MailModuleView from './MailModuleView.svelte';

const mocks = vi.hoisted(() => ({
	goto: vi.fn(),
	listAccounts: vi.fn(),
	listFolders: vi.fn(),
	listAccountMessages: vi.fn(),
	getRemoteMessageBody: vi.fn(),
	markMessageRead: vi.fn(),
	markMessageUnread: vi.fn(),
	moveMessage: vi.fn(),
	archiveMessage: vi.fn(),
	deleteMessage: vi.fn(),
	starMessage: vi.fn(),
	unstarMessage: vi.fn(),
	createImportJob: vi.fn(),
	listImportJobs: vi.fn(),
	listArchiveJobs: vi.fn(),
	listMessagesPage: vi.fn(),
	listDrafts: vi.fn(),
	getDraft: vi.fn(),
	getSmtpSettings: vi.fn(),
	sendOutboundMail: vi.fn(),
	saveDraft: vi.fn(),
	updateDraft: vi.fn(),
	sendDraft: vi.fn(),
	discardDraft: vi.fn(),
	uploadMessage: vi.fn(),
	remoteAttachmentUrl: vi.fn(() => '/attachment')
}));

vi.mock('$app/navigation', () => ({ goto: mocks.goto }));
vi.mock('$lib/api/files', () => ({ listAllFiles: vi.fn().mockResolvedValue([]) }));
vi.mock('$lib/api/mail', () => ({
	mailApi: mocks
}));

const testModule = {
	key: 'mail',
	name: 'Mail',
	description: 'Mail workspace',
	icon: 'mail',
	enabled: true,
	dashboard: { enabled: false },
	page: { enabled: true, route: '/modules/mail', renderer: 'mail-list', layout: 'list-grid' },
	aiIndexing: { enabled: false },
	audit: { enabled: true }
} as unknown as ModuleDefinition;

const account = {
	id: 'acct-1',
	name: 'Work mail',
	host: 'imap.example.com',
	port: 993,
	username: 'alice@example.com',
	tls_mode: 'tls',
	is_enabled: true,
	last_connected_at: '2026-07-20T10:00:00Z',
	last_error: null,
	created_at: '2026-07-01T00:00:00Z'
};
const folders = [
	{
		name: 'INBOX',
		display_name: 'Inbox',
		delimiter: '/',
		role: null,
		unseen: 3,
		total: 10
	},
	{
		name: 'Archive',
		display_name: 'Archive',
		delimiter: '/',
		role: 'archive',
		unseen: 0,
		total: 20
	}
] as const;
const message = {
	uid: 42,
	subject: 'Quarterly update',
	from_address: 'bob@example.com',
	from_name: 'Bob',
	sent_at: '2026-07-20T09:00:00Z',
	size_bytes: 2048,
	is_seen: false,
	is_flagged: true,
	imported_message_id: 'saved-42'
};
const body = {
	...message,
	to: [{ name: 'Alice', address: 'alice@example.com' }],
	cc: [],
	date: message.sent_at,
	message_id: '<remote-42@example.com>',
	in_reply_to: null,
	html: '<p>Hello <strong>Alice</strong></p><script>bad()</script>',
	text: 'Hello Alice',
	attachments: []
};

describe('MailModuleView', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		localStorage.clear();
		queryClient.clear();
		toastStore.clear();
		sessionStorage.clear();
		mocks.listAccounts.mockResolvedValue([account]);
		mocks.listFolders.mockResolvedValue(folders);
		mocks.listAccountMessages.mockResolvedValue({
			uidvalidity: 7,
			next_cursor: null,
			messages: [message]
		});
		mocks.getRemoteMessageBody.mockResolvedValue(body);
		mocks.remoteAttachmentUrl.mockReturnValue('/attachment');
		mocks.listDrafts.mockResolvedValue([]);
		mocks.listMessagesPage.mockResolvedValue({
			messages: [],
			next_cursor_at: null,
			next_cursor_id: null
		});
		mocks.listImportJobs.mockResolvedValue([]);
		mocks.listArchiveJobs.mockResolvedValue([]);
		mocks.getSmtpSettings.mockResolvedValue({ is_enabled: true });
		mocks.createImportJob.mockResolvedValue({ id: 'job-1' });
		mocks.markMessageRead.mockResolvedValue(undefined);
		mocks.markMessageUnread.mockResolvedValue(undefined);
		mocks.moveMessage.mockResolvedValue(undefined);
		mocks.archiveMessage.mockResolvedValue(undefined);
		mocks.deleteMessage.mockResolvedValue(undefined);
		mocks.starMessage.mockResolvedValue(undefined);
		mocks.unstarMessage.mockResolvedValue(undefined);
	});

	it('renders folders with unseen counts and virtual folders', async () => {
		render(MailModuleView, { module: testModule });

		expect(await screen.findByRole('button', { name: /Inbox/ })).toBeTruthy();
		expect(screen.getByText('3')).toBeTruthy();
		expect(screen.getByRole('button', { name: /Drafts/ })).toBeTruthy();
		expect(screen.getByRole('button', { name: 'Saved to RustShare' })).toBeTruthy();
	});

	it('loads message rows with unread, flagged, and imported indicators', async () => {
		render(MailModuleView, { module: testModule });

		const row = await screen.findByRole('button', { name: /Bob.*Quarterly update/ });
		expect(row.querySelector('.font-bold')).toBeTruthy();
		expect(row.querySelector('.fill-warning')).toBeTruthy();
		expect(row.querySelector('.text-success')).toBeTruthy();
	});

	it('loads and sanitizes the selected remote body', async () => {
		const { container } = render(MailModuleView, { module: testModule });
		await fireEvent.click(await screen.findByRole('button', { name: /Bob.*Quarterly update/ }));

		await waitFor(() => expect(container.textContent).toContain('Hello Alice'));
		expect(mocks.getRemoteMessageBody).toHaveBeenCalledWith('acct-1', 42, 'INBOX', 7);
		expect(container.querySelector('script')).toBeNull();
	});

	it('blocks remote images with a privacy notice and loads them on demand', async () => {
		mocks.getRemoteMessageBody.mockResolvedValue({
			...body,
			html: '<p>Hello</p><img src="https://tracker.example/pixel.gif">'
		});
		const { container } = render(MailModuleView, { module: testModule });
		await fireEvent.click(await screen.findByRole('button', { name: /Bob.*Quarterly update/ }));

		expect(await screen.findByText('Images were blocked to protect your privacy.')).toBeTruthy();
		const blockedImg = container.querySelector('img[data-rustshare-blocked-src]');
		expect(blockedImg).toBeTruthy();
		expect(blockedImg!.getAttribute('src')).toBeNull();

		await fireEvent.click(screen.getByRole('button', { name: 'Load remote images' }));
		await waitFor(() =>
			expect(container.querySelector('img[src="https://tracker.example/pixel.gif"]')).toBeTruthy()
		);
		expect(screen.queryByText('Images were blocked to protect your privacy.')).toBeNull();
		expect(screen.getByText('Remote images loaded for this message.')).toBeTruthy();
	});

	it('resets blocked remote images when switching messages', async () => {
		const second = { ...message, uid: 43, subject: 'Second notice', is_seen: true };
		mocks.listAccountMessages.mockResolvedValue({
			uidvalidity: 7,
			next_cursor: null,
			messages: [message, second]
		});
		mocks.getRemoteMessageBody.mockImplementation((_accountId: string, uid: number) =>
			Promise.resolve({ ...body, uid, html: '<img src="https://tracker.example/pixel.gif">' })
		);
		const { container } = render(MailModuleView, { module: testModule });
		await fireEvent.click(await screen.findByRole('button', { name: /Bob.*Quarterly update/ }));
		await fireEvent.click(await screen.findByRole('button', { name: 'Load remote images' }));
		await waitFor(() =>
			expect(container.querySelector('img[src="https://tracker.example/pixel.gif"]')).toBeTruthy()
		);

		await fireEvent.click(await screen.findByRole('button', { name: /Bob.*Second notice/ }));
		expect(await screen.findByText('Images were blocked to protect your privacy.')).toBeTruthy();
		expect(container.querySelector('img[src="https://tracker.example/pixel.gif"]')).toBeNull();
	});

	it('rewrites cid: images to attachment download URLs', async () => {
		mocks.getRemoteMessageBody.mockResolvedValue({
			...body,
			html: '<p>Logo:</p><img src="cid:logo@example.com">',
			attachments: [
				{
					index: 2,
					filename: 'logo.png',
					mime_type: 'image/png',
					size_bytes: 5,
					content_id: '<logo@example.com>'
				}
			]
		});
		const { container } = render(MailModuleView, { module: testModule });
		await fireEvent.click(await screen.findByRole('button', { name: /Bob.*Quarterly update/ }));

		await waitFor(() => expect(container.querySelector('img[src="/attachment"]')).toBeTruthy());
		expect(screen.queryByText('Images were blocked to protect your privacy.')).toBeNull();
	});

	it('keeps cid: images rewritten to absolute URLs on the local API base', async () => {
		// When VITE_API_URL is absolute (the default is
		// http://localhost:8080/api/v1), rewritten attachment URLs are absolute
		// too; they must not be treated as remote images, while a truly
		// external tracker in the same message is still blocked.
		const localUrl =
			'http://localhost:8080/api/v1/mail/accounts/acc-1/messages/42/attachments/2?folder=INBOX';
		mocks.remoteAttachmentUrl.mockReturnValue(localUrl);
		mocks.getRemoteMessageBody.mockResolvedValue({
			...body,
			html: '<p>Logo:</p><img src="cid:logo@example.com"><img src="https://tracker.example/pixel.gif">',
			attachments: [
				{
					index: 2,
					filename: 'logo.png',
					mime_type: 'image/png',
					size_bytes: 5,
					content_id: '<logo@example.com>'
				}
			]
		});
		const { container } = render(MailModuleView, { module: testModule });
		await fireEvent.click(await screen.findByRole('button', { name: /Bob.*Quarterly update/ }));

		await waitFor(() => expect(container.querySelector(`img[src="${localUrl}"]`)).toBeTruthy());
		expect(container.querySelector('img[src="https://tracker.example/pixel.gif"]')).toBeNull();
		expect(await screen.findByText('Images were blocked to protect your privacy.')).toBeTruthy();
	});

	it('leaves unresolved cid: images untouched without crashing', async () => {
		mocks.getRemoteMessageBody.mockResolvedValue({
			...body,
			html: '<img src="cid:missing-part">',
			attachments: []
		});
		const { container } = render(MailModuleView, { module: testModule });
		await fireEvent.click(await screen.findByRole('button', { name: /Bob.*Quarterly update/ }));

		await waitFor(() =>
			expect(container.querySelector('img[src="cid:missing-part"]')).toBeTruthy()
		);
		expect(screen.queryByText('Images were blocked to protect your privacy.')).toBeNull();
	});

	it('shows no remote-image notice for plain-text messages', async () => {
		mocks.getRemoteMessageBody.mockResolvedValue({ ...body, html: null, text: 'Plain body' });
		const { container } = render(MailModuleView, { module: testModule });
		await fireEvent.click(await screen.findByRole('button', { name: /Bob.*Quarterly update/ }));

		await waitFor(() => expect(container.textContent).toContain('Plain body'));
		expect(screen.queryByText('Images were blocked to protect your privacy.')).toBeNull();
		expect(screen.queryByRole('button', { name: 'Load remote images' })).toBeNull();
	});

	it('toggles the selected message star', async () => {
		render(MailModuleView, { module: testModule });
		await fireEvent.click(await screen.findByRole('button', { name: /Bob.*Quarterly update/ }));
		await fireEvent.click(await screen.findByRole('button', { name: 'Remove star' }));

		await waitFor(() => expect(mocks.unstarMessage).toHaveBeenCalledWith('acct-1', 42, 'INBOX', 7));
	});

	it('moves the selected message from the move modal', async () => {
		render(MailModuleView, { module: testModule });
		await fireEvent.click(await screen.findByRole('button', { name: /Bob.*Quarterly update/ }));
		await fireEvent.click(await screen.findByRole('button', { name: 'Move message' }));
		const archiveButtons = await screen.findAllByRole('button', { name: /Archive/ });
		await fireEvent.click(archiveButtons.at(-1)!);

		await waitFor(() =>
			expect(mocks.moveMessage).toHaveBeenCalledWith('acct-1', 42, 'INBOX', 'Archive', 7)
		);
	});

	it('disables the current folder as a move destination', async () => {
		render(MailModuleView, { module: testModule });
		await fireEvent.click(await screen.findByRole('button', { name: /Bob.*Quarterly update/ }));
		await fireEvent.click(await screen.findByRole('button', { name: 'Move message' }));

		const currentOption = screen
			.getAllByRole('button', { name: /Inbox/ })
			.find((button) => button.textContent?.includes('Current'));
		expect(currentOption).toBeTruthy();
		expect((currentOption as HTMLButtonElement).disabled).toBe(true);
	});

	it('archives the selected message into the archive-role folder', async () => {
		render(MailModuleView, { module: testModule });
		await fireEvent.click(await screen.findByRole('button', { name: /Bob.*Quarterly update/ }));
		await fireEvent.click(await screen.findByRole('button', { name: 'Archive message' }));

		await waitFor(() =>
			expect(mocks.archiveMessage).toHaveBeenCalledWith('acct-1', 42, 'INBOX', 7, 'Archive')
		);
	});

	it('shows the server error message when a single archive fails', async () => {
		mocks.archiveMessage.mockRejectedValue(new Error('Folder UIDVALIDITY changed for INBOX'));
		render(MailModuleView, { module: testModule });
		await fireEvent.click(await screen.findByRole('button', { name: /Bob.*Quarterly update/ }));
		await fireEvent.click(await screen.findByRole('button', { name: 'Archive message' }));

		await waitFor(() =>
			expect(
				get(toastStore).some(
					(toast) =>
						toast.type === 'error' &&
						toast.message === 'Archive failed: Folder UIDVALIDITY changed for INBOX'
				)
			).toBe(true)
		);
	});

	it('keeps the failure summary for bulk archive failures', async () => {
		const secondMessage = {
			...message,
			uid: 43,
			subject: 'Second message',
			is_seen: true,
			is_flagged: false,
			imported_message_id: null
		};
		mocks.listAccountMessages.mockResolvedValue({
			uidvalidity: 7,
			next_cursor: null,
			messages: [message, secondMessage]
		});
		mocks.archiveMessage.mockRejectedValue(new Error('boom'));
		render(MailModuleView, { module: testModule });
		await fireEvent.click(await screen.findByRole('checkbox', { name: 'Select all messages' }));
		const bulkBar = screen.getByText('2 selected').closest('div')!;
		await fireEvent.click(within(bulkBar).getByRole('button', { name: 'Archive' }));

		await waitFor(() =>
			expect(
				get(toastStore).some(
					(toast) =>
						toast.type === 'error' && toast.message === 'Archived 0 of 2 messages; 2 failed'
				)
			).toBe(true)
		);
		expect(get(toastStore).some((toast) => toast.message.startsWith('Archive failed:'))).toBe(
			false
		);
	});

	it('shows an error toast and skips the API call when no archive folder exists', async () => {
		mocks.listFolders.mockResolvedValue([folders[0]]);
		render(MailModuleView, { module: testModule });
		await fireEvent.click(await screen.findByRole('button', { name: /Bob.*Quarterly update/ }));
		await fireEvent.click(await screen.findByRole('button', { name: 'Archive message' }));

		expect(mocks.archiveMessage).not.toHaveBeenCalled();
		expect(
			get(toastStore).some(
				(toast) =>
					toast.type === 'error' && toast.message === 'No archive folder found on this account'
			)
		).toBe(true);
	});

	it('bulk archives selected messages with a success summary toast', async () => {
		render(MailModuleView, { module: testModule });
		await fireEvent.click(
			await screen.findByRole('checkbox', { name: 'Select message Quarterly update' })
		);
		const bulkBar = screen.getByText('1 selected').closest('div')!;
		await fireEvent.click(within(bulkBar).getByRole('button', { name: 'Archive' }));

		await waitFor(() =>
			expect(mocks.archiveMessage).toHaveBeenCalledWith('acct-1', 42, 'INBOX', 7, 'Archive')
		);
		await waitFor(() =>
			expect(
				get(toastStore).some(
					(toast) => toast.type === 'success' && toast.message === 'Messages archived'
				)
			).toBe(true)
		);
	});

	it('reports partial bulk archive failures and keeps failed UIDs selected', async () => {
		const secondMessage = {
			...message,
			uid: 43,
			subject: 'Second message',
			is_seen: true,
			is_flagged: false,
			imported_message_id: null
		};
		mocks.listAccountMessages.mockResolvedValue({
			uidvalidity: 7,
			next_cursor: null,
			messages: [message, secondMessage]
		});
		mocks.archiveMessage.mockImplementation((_accountId: string, uid: number) =>
			uid === 43 ? Promise.reject(new Error('boom')) : Promise.resolve(undefined)
		);
		render(MailModuleView, { module: testModule });
		await fireEvent.click(await screen.findByRole('checkbox', { name: 'Select all messages' }));
		const bulkBar = screen.getByText('2 selected').closest('div')!;
		await fireEvent.click(within(bulkBar).getByRole('button', { name: 'Archive' }));

		await waitFor(() =>
			expect(
				get(toastStore).some(
					(toast) =>
						toast.type === 'error' && toast.message === 'Archived 1 of 2 messages; 1 failed'
				)
			).toBe(true)
		);
		expect(mocks.archiveMessage).toHaveBeenCalledTimes(2);
		expect(screen.getByText('1 selected')).toBeTruthy();
		expect(
			(screen.getByRole('checkbox', { name: 'Select message Second message' }) as HTMLInputElement)
				.checked
		).toBe(true);
		expect(
			(
				screen.getByRole('checkbox', {
					name: 'Select message Quarterly update'
				}) as HTMLInputElement
			).checked
		).toBe(false);
	});

	it('ignores repeated archive clicks while a request is in flight', async () => {
		let resolveArchive: (() => void) | undefined;
		mocks.archiveMessage.mockImplementation(
			() =>
				new Promise<void>((resolve) => {
					resolveArchive = resolve;
				})
		);
		render(MailModuleView, { module: testModule });
		await fireEvent.click(await screen.findByRole('button', { name: /Bob.*Quarterly update/ }));
		const archiveButton = await screen.findByRole('button', { name: 'Archive message' });
		await fireEvent.click(archiveButton);
		await fireEvent.click(archiveButton);
		resolveArchive!();

		await waitFor(() => expect(mocks.archiveMessage).toHaveBeenCalledTimes(1));
	});

	it('shows bulk actions and applies them to selected UIDs', async () => {
		render(MailModuleView, { module: testModule });
		await fireEvent.click(
			await screen.findByRole('checkbox', { name: 'Select message Quarterly update' })
		);

		expect(screen.getByText('1 selected')).toBeTruthy();
		await fireEvent.click(screen.getByRole('button', { name: 'Star' }));
		await waitFor(() => expect(mocks.starMessage).toHaveBeenCalledWith('acct-1', 42, 'INBOX', 7));
	});

	it('wraps the bulk-action bar so it fits narrow viewports', async () => {
		render(MailModuleView, { module: testModule });
		await fireEvent.click(
			await screen.findByRole('checkbox', { name: 'Select message Quarterly update' })
		);

		const bar = screen.getByText('1 selected').parentElement!;
		expect(bar.className).toContain('flex-wrap');
		expect(bar.className).toContain('max-w-[calc(100vw-2rem)]');
		// All bulk actions stay in the reachable (wrapped) bar.
		for (const label of ['Read', 'Unread', 'Star', 'Save', 'Move', 'Delete']) {
			expect(bar.querySelector(`button`) && screen.getByRole('button', { name: label })).toBeTruthy();
		}
	});

	it('saves selected UIDs to RustShare', async () => {
		render(MailModuleView, { module: testModule });
		await fireEvent.click(
			await screen.findByRole('checkbox', { name: 'Select message Quarterly update' })
		);
		await fireEvent.click(screen.getByRole('button', { name: 'Save' }));
		await fireEvent.click(screen.getByRole('button', { name: 'Save to RustShare' }));

		await waitFor(() =>
			expect(mocks.createImportJob).toHaveBeenCalledWith('acct-1', {
				folder_name: 'INBOX',
				source_uidvalidity: 7,
				selected_uids: [42]
			})
		);
	});

	it('prefills replies with subject and raw threading header', async () => {
		render(MailModuleView, { module: testModule });
		await fireEvent.click(await screen.findByRole('button', { name: /Bob.*Quarterly update/ }));
		await waitFor(() =>
			expect((screen.getByRole('button', { name: 'Reply' }) as HTMLButtonElement).disabled).toBe(
				false
			)
		);
		await fireEvent.click(screen.getByRole('button', { name: 'Reply' }));

		expect(screen.getByDisplayValue('Re: Quarterly update')).toBeTruthy();
		await fireEvent.click(screen.getByRole('button', { name: 'Send' }));
		await waitFor(() =>
			expect(mocks.sendOutboundMail).toHaveBeenCalledWith(
				'acct-1',
				expect.objectContaining({ in_reply_to: '<remote-42@example.com>' })
			)
		);
	});

	it('opens a draft in compose', async () => {
		const draft = {
			id: 'draft-1',
			subject: 'Finish me',
			to_addresses: ['bob@example.com'],
			cc_addresses: [],
			bcc_addresses: [],
			from_address: account.username,
			from_name: 'Alice',
			sent_at: null,
			imported_at: '2026-07-20T09:00:00Z',
			size_bytes: 0,
			has_attachments: false,
			source_mode: 'draft'
		};
		mocks.listDrafts.mockResolvedValue([draft]);
		mocks.getDraft.mockResolvedValue({ message: draft, body: 'Draft body', attachments: [] });
		render(MailModuleView, { module: testModule });
		await fireEvent.click(await screen.findByRole('button', { name: /Drafts/ }));
		await fireEvent.click(await screen.findByRole('button', { name: /Finish me/ }));

		expect(await screen.findByDisplayValue('Finish me')).toBeTruthy();
		expect(screen.getByRole('button', { name: 'Save changes' })).toBeTruthy();
	});

	it('navigates from Saved to RustShare to the imported message', async () => {
		mocks.listMessagesPage.mockResolvedValue({
			messages: [
				{
					id: 'saved-1',
					subject: 'Saved mail',
					from_name: 'Bob',
					from_address: 'bob@example.com',
					to_addresses: [],
					cc_addresses: [],
					bcc_addresses: [],
					sent_at: null,
					imported_at: '2026-07-20T09:00:00Z',
					size_bytes: 1,
					has_attachments: false,
					source_mode: 'imap_selected'
				}
			],
			next_cursor_at: null,
			next_cursor_id: null
		});
		render(MailModuleView, { module: testModule });
		await fireEvent.click(await screen.findByRole('button', { name: 'Saved to RustShare' }));
		await fireEvent.click(await screen.findByRole('button', { name: /Saved mail/ }));

		expect(mocks.goto).toHaveBeenCalledWith('/modules/mail/messages/saved-1');
	});

	it('shows the saved mailbox when no IMAP account is configured', async () => {
		mocks.listAccounts.mockResolvedValue([]);
		mocks.listMessagesPage.mockResolvedValue({
			messages: [
				{
					id: 'saved-1',
					subject: 'Imported report',
					from_name: 'Bob',
					from_address: 'bob@example.com',
					to_addresses: [],
					cc_addresses: [],
					bcc_addresses: [],
					sent_at: null,
					imported_at: '2026-07-20T09:00:00Z',
					size_bytes: 1,
					has_attachments: false,
					source_mode: 'upload'
				}
			],
			next_cursor_at: null,
			next_cursor_id: null
		});
		render(MailModuleView, { module: testModule });

		// Imported mail stays reachable without an IMAP account…
		expect(await screen.findByRole('button', { name: /Imported report/ })).toBeTruthy();
		expect(screen.getByRole('button', { name: 'Saved to RustShare' })).toBeTruthy();
		// …and the account-setup prompt is a secondary hint, not a hard gate.
		expect(screen.getByText(/No mail account configured/)).toBeTruthy();
		expect(screen.getByRole('link', { name: 'Open Mail settings' })).toBeTruthy();
		// Account-dependent chrome is hidden.
		expect(screen.queryByLabelText('Mail account')).toBeNull();
		expect(screen.queryByRole('button', { name: /Drafts/ })).toBeNull();
	});

	it('keeps the three-pane remote view when an account exists', async () => {
		render(MailModuleView, { module: testModule });

		expect(await screen.findByLabelText('Mail account')).toBeTruthy();
		expect(screen.getByRole('button', { name: /Inbox/ })).toBeTruthy();
		expect(screen.queryByText(/No mail account configured/)).toBeNull();
	});

	it('persists the list context to sessionStorage', async () => {
		render(MailModuleView, { module: testModule });
		await fireEvent.click(await screen.findByRole('button', { name: 'Saved to RustShare' }));

		await waitFor(() => {
			const raw = sessionStorage.getItem('rustshare:mail-module:list-state');
			expect(raw).toBeTruthy();
			const saved = JSON.parse(raw!);
			expect(saved.mailboxView).toBe('saved');
			expect(saved.selectedAccountId).toBe('acct-1');
		});
	});

	it('restores the list context from sessionStorage on mount', async () => {
		sessionStorage.setItem(
			'rustshare:mail-module:list-state',
			JSON.stringify({
				mailboxView: 'saved',
				selectedAccountId: 'acct-1',
				selectedFolder: 'Archive',
				search: 'quarterly'
			})
		);
		mocks.listMessagesPage.mockResolvedValue({
			messages: [
				{
					id: 'saved-1',
					subject: 'Saved mail',
					from_name: 'Bob',
					from_address: 'bob@example.com',
					to_addresses: [],
					cc_addresses: [],
					bcc_addresses: [],
					sent_at: null,
					imported_at: '2026-07-20T09:00:00Z',
					size_bytes: 1,
					has_attachments: false,
					source_mode: 'imap_selected'
				}
			],
			next_cursor_at: null,
			next_cursor_id: null
		});
		render(MailModuleView, { module: testModule });

		// The saved mailbox is selected with the previous search applied.
		expect(await screen.findByRole('button', { name: /Saved mail/ })).toBeTruthy();
		await waitFor(() => expect(mocks.listMessagesPage).toHaveBeenCalledWith('quarterly'));
		expect(
			screen.getByRole('button', { name: 'Saved to RustShare' }).getAttribute('aria-current')
		).toBe('page');
		expect((screen.getByLabelText('Search mail') as HTMLInputElement).value).toBe('quarterly');
	});

	it('renders a folder error with inline retry', async () => {
		mocks.listFolders.mockRejectedValue(new Error('offline'));
		render(MailModuleView, { module: testModule });

		expect(
			await screen.findByText('Folders could not be synchronized.', {}, { timeout: 4_000 })
		).toBeTruthy();
		expect(screen.getByRole('button', { name: 'Retry' })).toBeTruthy();
	});

	it('requests the remote list newest-first by default', async () => {
		render(MailModuleView, { module: testModule });

		await waitFor(() =>
			expect(mocks.listAccountMessages).toHaveBeenCalledWith(
				'acct-1',
				'INBOX',
				100,
				null,
				'',
				'date_desc'
			)
		);
		expect(screen.getByRole('button', { name: 'Sort: newest first' })).toBeTruthy();
	});

	it('re-fetches oldest-first when the sort control is toggled', async () => {
		render(MailModuleView, { module: testModule });
		await screen.findByRole('button', { name: /Bob.*Quarterly update/ });

		await fireEvent.click(screen.getByRole('button', { name: 'Sort: newest first' }));

		await waitFor(() =>
			expect(mocks.listAccountMessages).toHaveBeenCalledWith(
				'acct-1',
				'INBOX',
				100,
				null,
				'',
				'date_asc'
			)
		);
		expect(screen.getByRole('button', { name: 'Sort: oldest first' })).toBeTruthy();
		expect(localStorage.getItem('mail-sort-order')).toBe('date_asc');
	});

	it('reads the persisted sort preference on mount', async () => {
		localStorage.setItem('mail-sort-order', 'date_asc');
		render(MailModuleView, { module: testModule });

		expect(await screen.findByRole('button', { name: 'Sort: oldest first' })).toBeTruthy();
		await waitFor(() =>
			expect(mocks.listAccountMessages).toHaveBeenCalledWith(
				'acct-1',
				'INBOX',
				100,
				null,
				'',
				'date_asc'
			)
		);
	});

	it('keeps the sort order across a search submit', async () => {
		render(MailModuleView, { module: testModule });
		await screen.findByRole('button', { name: /Bob.*Quarterly update/ });
		await fireEvent.click(screen.getByRole('button', { name: 'Sort: newest first' }));
		await waitFor(() =>
			expect(mocks.listAccountMessages).toHaveBeenCalledWith(
				'acct-1',
				'INBOX',
				100,
				null,
				'',
				'date_asc'
			)
		);

		const input = screen.getByRole('textbox', { name: 'Search mail' });
		await fireEvent.input(input, { target: { value: 'quarterly' } });
		await fireEvent.submit(input.closest('form')!);

		await waitFor(() =>
			expect(mocks.listAccountMessages).toHaveBeenCalledWith(
				'acct-1',
				'INBOX',
				100,
				null,
				'quarterly',
				'date_asc'
			)
		);
	});

	it('passes the sort order to the Saved view', async () => {
		localStorage.setItem('mail-sort-order', 'date_asc');
		render(MailModuleView, { module: testModule });
		await fireEvent.click(await screen.findByRole('button', { name: 'Saved to RustShare' }));

		await waitFor(() =>
			expect(mocks.listMessagesPage).toHaveBeenCalledWith('', null, null, 'date_asc')
		);
	});

	it('renders the sort control for an empty mailbox', async () => {
		mocks.listAccountMessages.mockResolvedValue({
			uidvalidity: 7,
			next_cursor: null,
			messages: []
		});
		render(MailModuleView, { module: testModule });

		expect(await screen.findByText('No messages in this folder.')).toBeTruthy();
		expect(screen.getByRole('button', { name: 'Sort: newest first' })).toBeTruthy();
	});
});
