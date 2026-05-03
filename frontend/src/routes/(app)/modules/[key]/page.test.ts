import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import Page from './+page.svelte';
import { page } from '$app/stores';
import { currentUser } from '$lib/stores/auth';
import { modulesStore } from '$lib/modules/registry';

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

// Mock the registry store
vi.mock('$lib/modules/registry', async () => {
	const actual = await vi.importActual<any>('$lib/modules/registry');
	return {
		...actual,
		modulesStore: {
			subscribe: vi.fn(),
			set: vi.fn(),
			update: vi.fn()
		}
	};
});

describe('Module Page Dynamic Route', () => {
	const mockUser = {
		id: 'user_1',
		email: 'test@example.com',
		display_name: 'Test User'
	};

	const mockModule = {
		id: 'mod_1',
		key: 'test-mod',
		displayName: 'Test Module',
		description: 'A test module description',
		enabled: true,
		rootPath: '/Test',
		renderer: 'generic',
		defaultTemplate: null,
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
		(modulesStore.subscribe as any).mockImplementation((run: any) => {
			run([]);
			return () => {};
		});

		render(Page);
		expect(screen.getByText('Module Not Found')).toBeTruthy();
	});

	it('renders disabled state for disabled module', () => {
		(page.subscribe as any).mockImplementation((run: any) => {
			run({ params: { key: 'test-mod' } });
			return () => {};
		});
		(modulesStore.subscribe as any).mockImplementation((run: any) => {
			run([{ ...mockModule, enabled: false }]);
			return () => {};
		});

		render(Page);
		expect(screen.getByText('Module Disabled')).toBeTruthy();
	});

	it('renders page disabled state when ui.page.enabled is false', () => {
		(page.subscribe as any).mockImplementation((run: any) => {
			run({ params: { key: 'test-mod' } });
			return () => {};
		});
		(modulesStore.subscribe as any).mockImplementation((run: any) => {
			run([
				{
					...mockModule,
					ui: { ...mockModule.ui, page: { ...mockModule.ui.page, enabled: false } }
				}
			]);
			return () => {};
		});

		render(Page);
		expect(screen.getByText('Module Page Disabled')).toBeTruthy();
	});

	it('renders module header with correct config', () => {
		(page.subscribe as any).mockImplementation((run: any) => {
			run({ params: { key: 'test-mod' } });
			return () => {};
		});
		(modulesStore.subscribe as any).mockImplementation((run: any) => {
			run([mockModule]);
			return () => {};
		});

		render(Page);
		expect(screen.getByText('Test Module')).toBeTruthy();
		expect(screen.getByText('A test module description')).toBeTruthy();
		expect(screen.getByText('Do Something')).toBeTruthy();
		expect(screen.getByText('Browse Files')).toBeTruthy();
	});

	it('falls back to GenericModuleView for unknown renderer', () => {
		(page.subscribe as any).mockImplementation((run: any) => {
			run({ params: { key: 'test-mod' } });
			return () => {};
		});
		(modulesStore.subscribe as any).mockImplementation((run: any) => {
			run([
				{
					...mockModule,
					ui: { ...mockModule.ui, page: { ...mockModule.ui.page, renderer: 'unknown-renderer' } }
				}
			]);
			return () => {};
		});

		render(Page);
		// Module header still renders even with unknown renderer
		expect(screen.getByText('Test Module')).toBeTruthy();
	});
});
