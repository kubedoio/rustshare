import { render, screen, within } from '@testing-library/svelte';
import { readable } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import DashboardPage from './+page.svelte';

vi.mock('$app/environment', () => ({
	browser: true
}));

vi.mock('$lib/api/users', () => ({
	getDashboardConfig: vi.fn().mockResolvedValue({
		enabled_modules: ['kanban', 'notes', 'unknown'],
		module_order: ['kanban', 'notes', 'unknown']
	}),
	updateDashboardConfig: vi.fn(),
	listUserModulePreferences: vi.fn().mockResolvedValue([]),
	updateUserModulePreference: vi.fn()
}));

vi.mock('$lib/modules/registry', async (importOriginal) => {
	const mod = await importOriginal<typeof import('$lib/modules/registry')>();
	return {
		...mod
	};
});

vi.mock('$lib/modules/moduleActions', () => ({
	runModulePrimaryAction: vi.fn()
}));

const mockEnabledModules = [
	{
		id: 'module-notes',
		module_key: 'notes',
		display_name: 'Notes',
		description: 'Recent notes',
		enabled: true,
		root_path: '/Workspace/Notes',
		renderer: 'notes',
		default_template: 'template_default_note',
		icon: 'sticky-note',
		schema_version: '1',
		permissions: {
			admin_can_configure: true,
			workspace_members_can_use: true,
			allow_public_share: true,
			allow_internal_share: true
		},
		ai_indexing: { enabled: true },
		audit: { enabled: true },
		ui_config: {
			dashboard: {
				enabled: true,
				order: 10,
				primaryAction: {
					label: 'New note',
					action: 'create-from-template',
					template: 'template_default_note'
				}
			}
		},
		created_at: '2026-04-30T00:00:00Z',
		updated_at: '2026-04-30T00:00:00Z'
	},
	{
		id: 'module-kanban',
		module_key: 'kanban',
		display_name: 'Kanban',
		description: 'Kanban boards',
		enabled: true,
		root_path: '/Workspace/Kanban',
		renderer: 'kanban',
		default_template: 'template_default_kanban',
		icon: 'columns',
		schema_version: '1',
		permissions: {
			admin_can_configure: true,
			workspace_members_can_use: true,
			allow_public_share: true,
			allow_internal_share: true
		},
		ai_indexing: { enabled: true },
		audit: { enabled: true },
		ui_config: {
			dashboard: {
				enabled: true,
				order: 20,
				primaryAction: {
					label: 'New Kanban board',
					action: 'create-from-template',
					template: 'template_default_kanban'
				}
			}
		},
		created_at: '2026-04-30T00:00:00Z',
		updated_at: '2026-04-30T00:00:00Z'
	},
	{
		id: 'module-decisions',
		module_key: 'decisions',
		display_name: 'Decisions',
		description: 'Decision records',
		enabled: false,
		root_path: '/Workspace/Decisions',
		renderer: 'decisions',
		default_template: 'template_default_decision',
		icon: 'path-separation',
		schema_version: '1',
		permissions: {
			admin_can_configure: true,
			workspace_members_can_use: true,
			allow_public_share: true,
			allow_internal_share: true
		},
		ai_indexing: { enabled: true },
		audit: { enabled: true },
		ui_config: {
			dashboard: {
				enabled: true,
				order: 30,
				primaryAction: {
					label: 'New decision record',
					action: 'create-from-template',
					template: 'template_default_decision'
				}
			}
		},
		created_at: '2026-04-30T00:00:00Z',
		updated_at: '2026-04-30T00:00:00Z'
	},
	{
		id: 'module-no-action',
		module_key: 'no-action',
		display_name: 'No Action Module',
		description: 'A module without a primary action',
		enabled: true,
		root_path: '/Workspace/NoAction',
		renderer: 'generic',
		default_template: null,
		icon: 'folder',
		schema_version: '1',
		permissions: {
			admin_can_configure: true,
			workspace_members_can_use: true,
			allow_public_share: true,
			allow_internal_share: true
		},
		ai_indexing: { enabled: true },
		audit: { enabled: true },
		ui_config: {
			dashboard: {
				enabled: true,
				order: 40
			}
		},
		created_at: '2026-04-30T00:00:00Z',
		updated_at: '2026-04-30T00:00:00Z'
	}
];

vi.mock('$lib/query-compat', () => ({
	createQuery: vi.fn((options: { queryKey?: unknown[] }) => {
		const key = options.queryKey?.[0];
		if (key === 'all-files') {
			return readable({
				data: [
					{
						id: 'note-123',
						name: 'New Name.md',
						size: 1200,
						deleted_at: null,
						mime_type: 'text/markdown'
					},
					{
						id: 'f2',
						name: 'Other File.txt',
						size: 3400,
						deleted_at: null,
						mime_type: 'text/plain'
					}
				],
				isLoading: false
			});
		}
		if (key === 'shares-received') {
			return readable({ data: [], isLoading: false });
		}
		if (key === 'module-summary') {
			return readable({
				data: {
					total_items: 5,
					recent_items: [
						{ name: 'Item 1', item_type: 'file', updated_at: new Date().toISOString() },
						{ name: 'Item 2', item_type: 'folder', updated_at: new Date().toISOString() }
					]
				},
				isLoading: false
			});
		}
		if (key === 'enabled-modules') {
			return readable({
				data: mockEnabledModules,
				isLoading: false
			});
		}
		if (key === 'workspace-module-summaries') {
			return readable({
				data: [
					{
						module: mockEnabledModules[0],
						summary: {
							total_items: 1,
							recent_items: [
								{
									id: 'note-1',
									name: 'Latest Note.md',
									item_type: 'file',
									updated_at: new Date().toISOString()
								}
							]
						}
					},
					{
						module: mockEnabledModules[1],
						summary: {
							total_items: 1,
							recent_items: [
								{
									id: 'board-1',
									name: 'Kanban Board',
									item_type: 'folder',
									updated_at: new Date().toISOString()
								}
							]
						}
					}
				],
				isLoading: false
			});
		}
		if (key === 'kanban-boards-widget') {
			return readable({ data: [], isLoading: false });
		}
		if (key === 'kanban-board-widget') {
			return readable({ data: null, isLoading: false });
		}
		return readable({ data: null, isLoading: false });
	})
}));

vi.mock('$lib/stores/activity', () => ({
	activityStore: readable([
		{
			id: 'act-1',
			type: 'note_created',
			fileName: 'Old Name.md',
			timestamp: new Date().toISOString(),
			artifactId: 'note-123',
			moduleKey: 'notes'
		}
	]),
	serverActivityStore: {
		subscribe: readable({
			items: [
				{
					id: 'act-1',
					type: 'note_created',
					fileName: 'New Name.md',
					timestamp: new Date().toISOString(),
					artifactId: 'note-123',
					moduleKey: 'notes',
					accessible: true
				}
			],
			loading: false,
			error: null,
			hasMore: false,
			cursor: null
		}).subscribe,
		fetch: vi.fn(),
		loadMore: vi.fn(),
		reset: vi.fn()
	},
	getActivityDisplay: vi.fn(() => ({
		icon: '📝',
		title: 'Note created',
		description: '',
		color: '#ea580c'
	})),
	getRelativeTime: vi.fn(() => 'Just now'),
	getActivityHref: vi.fn(() => '/modules/notes/note-123')
}));

vi.mock('$lib/stores/auth', () => ({
	currentUser: readable({
		id: 'user-1',
		email: 'alex@example.com',
		display_name: 'Alex Johnson',
		is_admin: true,
		storage_quota: 10_000,
		storage_used: 4_600
	})
}));

describe('Dashboard Page Workspace Surface', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('renders the approved overview surfaces without module cards', async () => {
		render(DashboardPage);

		await vi.waitFor(() => {
			expect(screen.getByRole('heading', { name: 'Workspace Overview' })).toBeTruthy();
			expect(screen.getByLabelText('Workspace summary')).toBeTruthy();
			expect(screen.getByLabelText('Recent activity')).toBeTruthy();
			expect(screen.getByLabelText('Quick actions')).toBeTruthy();
		});

		expect(screen.queryByText('Kanban Dashboard')).toBeNull();
		expect(screen.queryByText('/Workspace/Kanban')).toBeNull();
	});

	it('renders three metric summary cards', async () => {
		render(DashboardPage);

		await vi.waitFor(() => {
			expect(screen.getByText('Recent Artifacts')).toBeTruthy();
			expect(screen.getByText('Updated This Week')).toBeTruthy();
			expect(screen.getByText('Shared Items')).toBeTruthy();
		});
	});

	it('does not render pinned folders widget', async () => {
		render(DashboardPage);

		await vi.waitFor(() => {
			expect(screen.queryByLabelText('Pinned folders')).toBeNull();
		});
	});

	it('renders metric values correctly', async () => {
		render(DashboardPage);

		await vi.waitFor(() => {
			expect(screen.getAllByText('2').length).toBeGreaterThanOrEqual(1);
			expect(screen.getAllByText('0').length).toBe(2);
		});
	});

	it('renders New note quick action button', async () => {
		render(DashboardPage);

		await vi.waitFor(() => {
			const newNoteMatches = screen.getAllByText('New note');
			expect(newNoteMatches.length).toBeGreaterThan(0);
		});
	});

	it('renders server-sourced recent activity', async () => {
		render(DashboardPage);

		await vi.waitFor(() => {
			expect(screen.getByText('New Name.md')).toBeTruthy();
		});
	});
});

describe('Dashboard Page Registry-Driven Quick Actions', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('renders quick actions only for enabled modules with primary actions', async () => {
		render(DashboardPage);

		await vi.waitFor(() => {
			const quickActionsSection = screen.getByLabelText('Quick actions');
			expect(within(quickActionsSection).getByText('New note')).toBeTruthy();
			expect(within(quickActionsSection).getByText('New Kanban board')).toBeTruthy();
		});

		// Decisions is disabled → no quick action
		const quickActionsSection = screen.getByLabelText('Quick actions');
		expect(within(quickActionsSection).queryByText('New decision record')).toBeNull();
	});

	it('hides disabled modules from quick actions', async () => {
		render(DashboardPage);

		await vi.waitFor(() => {
			const quickActionsSection = screen.getByLabelText('Quick actions');
			expect(within(quickActionsSection).queryByText('New decision record')).toBeNull();
		});
	});

	it('hides modules without primary actions from quick actions', async () => {
		render(DashboardPage);

		await vi.waitFor(() => {
			expect(screen.queryByText('New No Action Module')).toBeNull();
		});
	});
});
