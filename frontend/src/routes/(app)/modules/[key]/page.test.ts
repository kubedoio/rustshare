import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import Page from './+page.svelte';
import { page } from '$app/stores';
import { currentUser } from '$lib/stores/auth';
import * as registry from '$lib/modules/registry';

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
vi.mock('$lib/modules/registry', async () => {
	const actual = await vi.importActual<any>('$lib/modules/registry');
	return {
		...actual,
		getModuleByKey: vi.fn()
	};
});

describe('Module Page Dynamic Route', () => {
	const mockUser = {
		id: 'user_1',
		email: 'test@example.com',
		display_name: 'Test User'
	};

	const mockModule: registry.ModuleDefinition = {
		id: 'mod_1',
		key: 'test-mod',
		displayName: 'Test Module',
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
				route: '/modules/test-mod',
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
		(registry.getModuleByKey as any).mockReturnValue(undefined);

		render(Page);
		expect(screen.getByText('Module Not Found')).toBeTruthy();
	});

	it('renders disabled state for disabled module', () => {
		(page.subscribe as any).mockImplementation((run: any) => {
			run({ params: { key: 'test-mod' } });
			return () => {};
		});
		(registry.getModuleByKey as any).mockReturnValue({ ...mockModule, enabled: false });

		render(Page);
		expect(screen.getByText('Module Disabled')).toBeTruthy();
	});

	it('renders page disabled state when ui.page.enabled is false', () => {
		(page.subscribe as any).mockImplementation((run: any) => {
			run({ params: { key: 'test-mod' } });
			return () => {};
		});
		(registry.getModuleByKey as any).mockReturnValue({
			...mockModule,
			ui: { ...mockModule.ui, page: { ...mockModule.ui.page, enabled: false } }
		});

		render(Page);
		expect(screen.getByText('Module Page Disabled')).toBeTruthy();
	});

	it('renders module content via ModulePageRenderer', () => {
		(page.subscribe as any).mockImplementation((run: any) => {
			run({ params: { key: 'test-mod' } });
			return () => {};
		});
		(registry.getModuleByKey as any).mockReturnValue(mockModule);

		render(Page);
		// GenericModuleView renders inside ModulePageShell with module title
		expect(screen.getByText('Test Module')).toBeTruthy();
	});

	it('falls back to GenericModuleView for unknown renderer', () => {
		(page.subscribe as any).mockImplementation((run: any) => {
			run({ params: { key: 'test-mod' } });
			return () => {};
		});
		(registry.getModuleByKey as any).mockReturnValue({
			...mockModule,
			ui: { ...mockModule.ui, page: { ...mockModule.ui.page, renderer: 'unknown-renderer' } }
		});

		render(Page);
		// GenericModuleView renders inside ModulePageShell with module title
		expect(screen.getByText('Test Module')).toBeTruthy();
	});
});
