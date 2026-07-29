import { render, screen } from '@testing-library/svelte';
import { readable } from 'svelte/store';
import { describe, expect, it, vi } from 'vitest';
import MailMessagePage from './+page.svelte';

const fixtures = vi.hoisted(() => ({
	message: {
		id: 'msg-1',
		account_id: 'acct-1',
		subject: 'Quarterly report',
		from_name: 'Bob',
		from_address: 'bob@example.com',
		to_addresses: ['alice@example.com'],
		cc_addresses: [],
		bcc_addresses: [],
		sent_at: '2026-07-20T09:00:00Z',
		imported_at: '2026-07-20T10:00:00Z',
		size_bytes: 4096,
		has_attachments: true,
		source_mode: 'imap_selected'
	},
	parts: [
		{
			id: 'part-1',
			part_index: 0,
			content_type: 'text/plain',
			charset: 'utf-8',
			size_bytes: 10,
			is_body: true
		}
	],
	attachments: [
		{
			id: 'att-1',
			file_id: 'file-1',
			filename: 'report.pdf',
			mime_type: 'application/pdf',
			size_bytes: 2048
		},
		{
			id: 'att-2',
			file_id: null,
			filename: 'notes.txt',
			mime_type: 'text/plain',
			size_bytes: 0
		}
	]
}));

vi.mock('$app/stores', () => ({
	page: readable({ params: { messageId: 'msg-1' } })
}));

vi.mock('$app/navigation', () => ({ goto: vi.fn() }));

vi.mock('$lib/modules/registry', () => ({
	getModuleByKey: vi.fn(() => ({ enabled: true }))
}));

vi.mock('$lib/api/files', () => ({ listAllFiles: vi.fn().mockResolvedValue([]) }));

vi.mock('$lib/api/mail', () => ({
	mailApi: {
		getMessage: vi.fn(),
		listParts: vi.fn(),
		listAttachments: vi.fn(),
		listLinks: vi.fn().mockResolvedValue([]),
		getPartContent: vi.fn().mockResolvedValue('Hello Alice'),
		listAccounts: vi.fn().mockResolvedValue([]),
		getSmtpSettings: vi.fn().mockResolvedValue(null),
		downloadSourceUrl: vi.fn((messageId: string) => `/api/v1/mail/messages/${messageId}/source`),
		attachmentDownloadUrl: vi.fn(
			(messageId: string, attachmentId: string) =>
				`/api/v1/mail/messages/${messageId}/attachments/${attachmentId}`
		),
		createLink: vi.fn(),
		deleteLink: vi.fn(),
		replyMail: vi.fn(),
		replyAllMail: vi.fn(),
		forwardMail: vi.fn(),
		sendOutboundMail: vi.fn(),
		saveDraft: vi.fn(),
		updateDraft: vi.fn(),
		discardDraft: vi.fn()
	}
}));

vi.mock('$lib/query-compat', () => ({
	createQuery: vi.fn((options: { queryKey?: unknown[] }) => {
		const key = options.queryKey?.[0];
		const data =
			key === 'mail-message'
				? fixtures.message
				: key === 'mail-message-parts'
					? fixtures.parts
					: key === 'mail-message-attachments'
						? fixtures.attachments
						: [];
		const store = readable({ data, isLoading: false, isError: false, refetch: vi.fn() });
		return { subscribe: store.subscribe, setOptions: vi.fn() };
	}),
	createMutation: vi.fn(() => {
		const store = readable({ mutate: vi.fn(), isPending: false });
		return { subscribe: store.subscribe };
	})
}));

describe('Mail message detail page', () => {
	it('links each attachment to the mail attachment download endpoint', async () => {
		render(MailMessagePage);

		const links = await screen.findAllByRole('link', { name: 'Download' });
		expect(links).toHaveLength(2);
		expect(links[0].getAttribute('href')).toBe('/api/v1/mail/messages/msg-1/attachments/att-1');
		expect(links[1].getAttribute('href')).toBe('/api/v1/mail/messages/msg-1/attachments/att-2');
		expect(links[0].getAttribute('download')).not.toBeNull();
	});

	it('shows attachment filename, type, and size', async () => {
		render(MailMessagePage);

		expect(await screen.findByText('report.pdf')).toBeTruthy();
		expect(screen.getByText('notes.txt')).toBeTruthy();
		expect(screen.getByText(/application\/pdf/)).toBeTruthy();
		expect(screen.getByText(/2,048 bytes/)).toBeTruthy();
	});

	it('keeps the mail-only badge for attachments without a linked file', async () => {
		render(MailMessagePage);

		expect(await screen.findByText('mail-only')).toBeTruthy();
		expect(screen.getByRole('button', { name: 'Open file' })).toBeTruthy();
	});

	it('keeps the separate Download .eml action pointing at the source endpoint', async () => {
		render(MailMessagePage);

		const emlLink = await screen.findByRole('link', { name: 'Download .eml' });
		expect(emlLink.getAttribute('href')).toBe('/api/v1/mail/messages/msg-1/source');
	});
});
