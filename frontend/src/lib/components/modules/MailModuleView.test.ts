import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { queryClient } from '$lib/query-client';
import MailModuleView from './MailModuleView.svelte';

const mocks = vi.hoisted(() => ({
	goto: vi.fn(),
	listMessages: vi.fn(),
	listAccounts: vi.fn(),
	listFolders: vi.fn(),
	listAccountMessages: vi.fn(),
	listArchiveJobs: vi.fn(),
	createAccount: vi.fn(),
	deleteAccount: vi.fn(),
	testAccount: vi.fn(),
	createImportJob: vi.fn(),
	createArchiveJob: vi.fn(),
	cancelArchiveJob: vi.fn(),
	uploadMessage: vi.fn()
}));

vi.mock('$app/navigation', () => ({
	goto: mocks.goto
}));

vi.mock('$lib/api/mail', () => ({
	mailApi: {
		listMessages: mocks.listMessages,
		listAccounts: mocks.listAccounts,
		listFolders: mocks.listFolders,
		listAccountMessages: mocks.listAccountMessages,
		listArchiveJobs: mocks.listArchiveJobs,
		createAccount: mocks.createAccount,
		deleteAccount: mocks.deleteAccount,
		testAccount: mocks.testAccount,
		createImportJob: mocks.createImportJob,
		createArchiveJob: mocks.createArchiveJob,
		cancelArchiveJob: mocks.cancelArchiveJob,
		uploadMessage: mocks.uploadMessage
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
		mocks.listAccounts.mockResolvedValue([]);
		mocks.listFolders.mockResolvedValue([]);
		mocks.listAccountMessages.mockResolvedValue({ uidvalidity: null, messages: [] });
		mocks.listArchiveJobs.mockResolvedValue([]);
		mocks.uploadMessage.mockResolvedValue({ id: 'msg-uploaded' });
	});

	it('renders message subject rows and navigates on click', async () => {
		mocks.listMessages.mockResolvedValueOnce([
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
		]);

		render(MailModuleView, { module: testModule });

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

		const emptyTitle = await screen.findByText('No imported mail yet');
		expect(emptyTitle).toBeTruthy();
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
		mocks.listFolders.mockResolvedValue([{ name: 'INBOX', delimiter: '/' }]);
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

	it('uploads .eml files through the mail import endpoint', async () => {
		mocks.listMessages.mockResolvedValueOnce([]);

		render(MailModuleView, { module: testModule });

		const input = document.querySelector('input[type="file"]') as HTMLInputElement;
		const file = new File(['From: alice@example.com'], 'message.eml', { type: 'message/rfc822' });

		await fireEvent.change(input, { target: { files: [file] } });

		await waitFor(() => {
			expect(mocks.uploadMessage).toHaveBeenCalledWith(file);
			expect(mocks.listMessages).toHaveBeenCalledTimes(2);
		});
	});
});
