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
		getApplicationByKey: vi.fn()
	};
});

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
		(registry.getApplicationByKey as any).mockReturnValue(undefined);

		render(Page);
		expect(screen.getByText('Application Not Found')).toBeTruthy();
	});

	it('renders disabled state for disabled module', () => {
		(page.subscribe as any).mockImplementation((run: any) => {
			run({ params: { key: 'test-mod' } });
			return () => {};
		});
		(registry.getApplicationByKey as any).mockReturnValue({ ...mockModule, enabled: false });

		render(Page);
		expect(screen.getByText('Application Disabled')).toBeTruthy();
	});

	it('renders page disabled state when ui.page.enabled is false', () => {
		(page.subscribe as any).mockImplementation((run: any) => {
			run({ params: { key: 'test-mod' } });
			return () => {};
		});
		(registry.getApplicationByKey as any).mockReturnValue({
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
		(registry.getApplicationByKey as any).mockReturnValue(mockModule);

		render(Page);
		// GenericApplicationView renders inside ApplicationPageShell with module title
		expect(screen.getByText('Test Application')).toBeTruthy();
	});

	it('falls back to GenericApplicationView for unknown renderer', () => {
		(page.subscribe as any).mockImplementation((run: any) => {
			run({ params: { key: 'test-mod' } });
			return () => {};
		});
		(registry.getApplicationByKey as any).mockReturnValue({
			...mockModule,
			ui: { ...mockModule.ui, page: { ...mockModule.ui.page, renderer: 'unknown-renderer' } }
		});

		render(Page);
		// GenericApplicationView renders inside ApplicationPageShell with module title
		expect(screen.getByText('Test Application')).toBeTruthy();
	});
});
