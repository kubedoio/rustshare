import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import Page from './+page.svelte';
import { page } from '$app/stores';
import { currentUser } from '$lib/stores/auth';
import * as registry from '$lib/applications/registry';

// Mock SvelteKit stores
vi.mock('$app/stores', () => ({
	page: {
		subscribe: vi.fn()
	}
}));

vi.mock('$lib/stores/auth', () => ({
	currentUser: {
		subscribe: vi.fn()
	}
}));

// Mock the registry
vi.mock('$lib/applications/registry', async () => {
	const actual = await vi.importActual<any>('$lib/applications/registry');
	return {
		...actual,
		getApplicationByRouteSlug: vi.fn()
	};
});

// MailApplicationView fires mail API queries on mount; stub them so the
// renderer-routing test below exercises the real component.
vi.mock('$app/navigation', () => ({ goto: vi.fn() }));
vi.mock('$lib/api/files', () => ({ listAllFiles: vi.fn().mockResolvedValue([]) }));
vi.mock('$lib/api/mail', () => ({
	mailApi: {
		listAccounts: vi.fn().mockResolvedValue([]),
		listFolders: vi.fn().mockResolvedValue([]),
		listAccountMessages: vi
			.fn()
			.mockResolvedValue({ uidvalidity: null, next_cursor: null, messages: [] }),
		getRemoteMessageBody: vi.fn(),
		markMessageRead: vi.fn(),
		markMessageUnread: vi.fn(),
		moveMessage: vi.fn(),
		archiveMessage: vi.fn(),
		deleteMessage: vi.fn(),
		starMessage: vi.fn(),
		unstarMessage: vi.fn(),
		createImportJob: vi.fn(),
		listImportJobs: vi.fn().mockResolvedValue([]),
		listArchiveJobs: vi.fn().mockResolvedValue([]),
		listMessagesPage: vi.fn().mockResolvedValue({
			messages: [],
			next_cursor_at: null,
			next_cursor_id: null
		}),
		listDrafts: vi.fn().mockResolvedValue([]),
		getDraft: vi.fn(),
		getSmtpSettings: vi.fn().mockResolvedValue(null),
		sendOutboundMail: vi.fn(),
		saveDraft: vi.fn(),
		updateDraft: vi.fn(),
		sendDraft: vi.fn(),
		discardDraft: vi.fn(),
		uploadMessage: vi.fn(),
		remoteAttachmentUrl: vi.fn(() => '/attachment'),
		remoteSourceUrl: vi.fn(() => '/message.eml')
	}
}));

describe('Application Page Dynamic Route', () => {
	const mockUser = {
		id: 'user_1',
		email: 'test@example.com',
		display_name: 'Test User'
	};

	const mockModule: registry.ApplicationDefinition = {
		id: 'mod_1',
		key: 'test-mod',
		displayName: 'Test Application',
		description: 'A test module description',
		enabled: true,
		rootPath: '/Test',
		renderer: 'generic',
		defaultTemplate: null,
		icon: 'folder',
		schemaVersion: '1.0',
		permissions: {
			adminCanConfigure: true,
			workspaceMembersCanUse: true,
			allowPublicShare: true,
			allowInternalShare: true
		},
		ui: {
			sidebar: { enabled: true, order: 1, icon: 'folder', label: 'Test' },
			dashboard: {
				enabled: true,
				order: 1,
				widget: {
					enabled: true,
					type: 'generic',
					title: 'Test',
					description: 'Test',
					size: 'medium',
					columns: { desktop: 6, tablet: 12, mobile: 12 },
					maxItems: 4
				}
			},
			page: {
				enabled: true,
				route: '/apps/test-mod',
				renderer: 'generic',
				layout: 'default',
				emptyStateTitle: 'Empty',
				emptyStateDescription: 'Nothing here',
				primaryAction: { label: 'Do Something', action: 'test' }
			}
		},
		aiIndexing: { enabled: false },
		audit: { enabled: false }
	};

	beforeEach(() => {
		vi.clearAllMocks();
		(currentUser.subscribe as any).mockImplementation((run: any) => {
			run(mockUser);
			return () => {};
		});
	});

	it('renders 404 for unknown module', () => {
		(page.subscribe as any).mockImplementation((run: any) => {
			run({ params: { key: 'unknown' } });
			return () => {};
		});
		(registry.getApplicationByRouteSlug as any).mockReturnValue(undefined);

		render(Page);
		expect(screen.getByText('Application Not Found')).toBeTruthy();
	});

	it('renders disabled state for disabled module', () => {
		(page.subscribe as any).mockImplementation((run: any) => {
			run({ params: { key: 'test-mod' } });
			return () => {};
		});
		(registry.getApplicationByRouteSlug as any).mockReturnValue({ ...mockModule, enabled: false });

		render(Page);
		expect(screen.getByText('Application Disabled')).toBeTruthy();
	});

	it('renders page disabled state when ui.page.enabled is false', () => {
		(page.subscribe as any).mockImplementation((run: any) => {
			run({ params: { key: 'test-mod' } });
			return () => {};
		});
		(registry.getApplicationByRouteSlug as any).mockReturnValue({
			...mockModule,
			ui: { ...mockModule.ui, page: { ...mockModule.ui.page, enabled: false } }
		});

		render(Page);
		expect(screen.getByText('Application Page Disabled')).toBeTruthy();
	});

	it('renders module content via ApplicationPageRenderer', () => {
		(page.subscribe as any).mockImplementation((run: any) => {
			run({ params: { key: 'test-mod' } });
			return () => {};
		});
		(registry.getApplicationByRouteSlug as any).mockReturnValue(mockModule);

		render(Page);
		// GenericApplicationView renders inside ApplicationPageShell with module title
		expect(screen.getByText('Test Application')).toBeTruthy();
	});

	it('falls back to GenericApplicationView for unknown renderer', () => {
		(page.subscribe as any).mockImplementation((run: any) => {
			run({ params: { key: 'test-mod' } });
			return () => {};
		});
		(registry.getApplicationByRouteSlug as any).mockReturnValue({
			...mockModule,
			ui: { ...mockModule.ui, page: { ...mockModule.ui.page, renderer: 'unknown-renderer' } }
		});

		render(Page);
		// GenericApplicationView renders inside ApplicationPageShell with module title
		expect(screen.getByText('Test Application')).toBeTruthy();
	});

	it('routes the first-party "mail" renderer to the mail mailbox view', async () => {
		(page.subscribe as any).mockImplementation((run: any) => {
			run({ params: { key: 'mail' } });
			return () => {};
		});
		(registry.getApplicationByRouteSlug as any).mockReturnValue({
			...mockModule,
			key: 'mail',
			ui: { ...mockModule.ui, page: { ...mockModule.ui.page, renderer: 'mail' } }
		});

		render(Page);
		// MailApplicationView's zero-account banner — GenericApplicationView
		// would render the generic empty state instead.
		expect(await screen.findByText(/No mail account configured/)).toBeTruthy();
	});
});
