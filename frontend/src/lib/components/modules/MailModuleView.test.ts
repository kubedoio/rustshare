import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { queryClient } from '$lib/query-client';
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
	uploadMessage: vi.fn()
}));

vi.mock('$app/navigation', () => ({ goto: mocks.goto }));
vi.mock('$lib/api/files', () => ({ listAllFiles: vi.fn().mockResolvedValue([]) }));
vi.mock('$lib/api/mail', () => ({
	mailApi: {
		...mocks,
		remoteAttachmentUrl: vi.fn(() => '/attachment')
	}
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
		queryClient.clear();
		mocks.listAccounts.mockResolvedValue([account]);
		mocks.listFolders.mockResolvedValue(folders);
		mocks.listAccountMessages.mockResolvedValue({
			uidvalidity: 7,
			next_cursor: null,
			messages: [message]
		});
		mocks.getRemoteMessageBody.mockResolvedValue(body);
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

	it('shows bulk actions and applies them to selected UIDs', async () => {
		render(MailModuleView, { module: testModule });
		await fireEvent.click(
			await screen.findByRole('checkbox', { name: 'Select message Quarterly update' })
		);

		expect(screen.getByText('1 selected')).toBeTruthy();
		await fireEvent.click(screen.getByRole('button', { name: 'Star' }));
		await waitFor(() => expect(mocks.starMessage).toHaveBeenCalledWith('acct-1', 42, 'INBOX', 7));
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

	it('renders a folder error with inline retry', async () => {
		mocks.listFolders.mockRejectedValue(new Error('offline'));
		render(MailModuleView, { module: testModule });

		expect(
			await screen.findByText('Folders could not be synchronized.', {}, { timeout: 4_000 })
		).toBeTruthy();
		expect(screen.getByRole('button', { name: 'Retry' })).toBeTruthy();
	});
});
