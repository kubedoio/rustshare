import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { queryClient } from '$lib/query-client';
import MailModuleView from './MailModuleView.svelte';

const mocks = vi.hoisted(() => ({
	goto: vi.fn(),
	listMessages: vi.fn()
}));

vi.mock('$app/navigation', () => ({
	goto: mocks.goto
}));

vi.mock('$lib/api/mail', () => ({
	mailApi: {
		listMessages: mocks.listMessages
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
});
