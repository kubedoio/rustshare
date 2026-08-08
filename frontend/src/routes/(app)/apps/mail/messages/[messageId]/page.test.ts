import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { readable } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { queryClient } from '$lib/query-client';
import MailMessagePage from './+page.svelte';

const mocks = vi.hoisted(() => ({
	goto: vi.fn(),
	getMessage: vi.fn(),
	listParts: vi.fn(),
	listAttachments: vi.fn(),
	listLinks: vi.fn(),
	listAccounts: vi.fn(),
	getSmtpSettings: vi.fn(),
	getPartContent: vi.fn(),
	getPartContentWithMeta: vi.fn(),
	createLink: vi.fn(),
	deleteLink: vi.fn(),
	replyMail: vi.fn(),
	replyAllMail: vi.fn(),
	forwardMail: vi.fn(),
	sendOutboundMail: vi.fn(),
	saveDraft: vi.fn(),
	updateDraft: vi.fn(),
	discardDraft: vi.fn()
}));

vi.mock('$app/stores', () => ({
	page: readable({ params: { messageId: 'msg-1' } })
}));
vi.mock('$app/navigation', () => ({ goto: mocks.goto }));
vi.mock('$lib/api/files', () => ({ listAllFiles: vi.fn().mockResolvedValue([]) }));
vi.mock('$lib/api/mail', () => ({
	mailApi: {
		...mocks,
		downloadSourceUrl: vi.fn(() => '/source')
	}
}));

const account = {
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

const message = {
	id: 'msg-1',
	account_id: 'acct-1',
	subject: 'Imported newsletter',
	from_address: 'bob@example.com',
	from_name: 'Bob',
	to_addresses: ['alice@example.com'],
	cc_addresses: [],
	bcc_addresses: [],
	sent_at: '2026-07-20T09:00:00Z',
	imported_at: '2026-07-20T10:00:00Z',
	size_bytes: 100,
	has_attachments: false,
	source_mode: 'imap_selected'
};

const htmlPart = {
	id: 'part-1',
	part_index: 1,
	content_type: 'text/html',
	charset: 'utf-8',
	size_bytes: 50,
	is_body: true
};

const textPart = {
	id: 'part-2',
	part_index: 0,
	content_type: 'text/plain',
	charset: 'utf-8',
	size_bytes: 20,
	is_body: true
};

describe('Imported mail message page', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		queryClient.clear();
		mocks.getMessage.mockResolvedValue(message);
		mocks.listParts.mockResolvedValue([htmlPart]);
		mocks.listAttachments.mockResolvedValue([]);
		mocks.listLinks.mockResolvedValue([]);
		mocks.listAccounts.mockResolvedValue([account]);
		mocks.getSmtpSettings.mockResolvedValue(null);
		mocks.getPartContent.mockResolvedValue('Plain body');
		mocks.getPartContentWithMeta.mockResolvedValue({
			content: '<p>Hi</p>',
			blockedRemoteImages: false
		});
	});

	it('shows a privacy notice when the backend blocked remote images', async () => {
		mocks.getPartContentWithMeta.mockResolvedValue({
			content: '<p>Hi</p><img data-rustshare-blocked-src="https://tracker.example/pixel.gif">',
			blockedRemoteImages: true
		});
		render(MailMessagePage);

		expect(await screen.findByText('Images were blocked to protect your privacy.')).toBeTruthy();
		expect(mocks.getPartContentWithMeta).toHaveBeenCalledWith('msg-1', 'part-1', {
			loadRemoteImages: false
		});
	});

	it('re-fetches with load_remote_images=true when loading remote images', async () => {
		mocks.getPartContentWithMeta
			.mockResolvedValueOnce({
				content: '<p>Hi</p><img data-rustshare-blocked-src="https://tracker.example/pixel.gif">',
				blockedRemoteImages: true
			})
			.mockResolvedValue({
				content: '<p>Hi</p><img src="https://tracker.example/pixel.gif">',
				blockedRemoteImages: false
			});
		const { container } = render(MailMessagePage);

		await fireEvent.click(await screen.findByRole('button', { name: 'Load remote images' }));

		await waitFor(() =>
			expect(mocks.getPartContentWithMeta).toHaveBeenCalledWith('msg-1', 'part-1', {
				loadRemoteImages: true
			})
		);
		await waitFor(() =>
			expect(container.querySelector('img[src="https://tracker.example/pixel.gif"]')).toBeTruthy()
		);
		expect(screen.queryByText('Images were blocked to protect your privacy.')).toBeNull();
		expect(screen.getByText('Remote images loaded for this message.')).toBeTruthy();
	});

	it('shows no notice when nothing was blocked', async () => {
		render(MailMessagePage);

		await waitFor(() =>
			expect(mocks.getPartContentWithMeta).toHaveBeenCalledWith('msg-1', 'part-1', {
				loadRemoteImages: false
			})
		);
		expect(await screen.findByText('Hi')).toBeTruthy();
		expect(screen.queryByText('Images were blocked to protect your privacy.')).toBeNull();
		expect(screen.queryByRole('button', { name: 'Load remote images' })).toBeNull();
	});

	it('shows no notice for plain-text messages', async () => {
		mocks.listParts.mockResolvedValue([textPart]);
		mocks.getPartContent.mockResolvedValue('Plain body');
		render(MailMessagePage);

		expect(await screen.findByText('Plain body')).toBeTruthy();
		expect(screen.queryByText('Images were blocked to protect your privacy.')).toBeNull();
		expect(screen.queryByRole('button', { name: 'Load remote images' })).toBeNull();
	});
});
