import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
	createFromTemplate,
	getModule,
	getModuleSummary,
	listEnabledModules
} from '$lib/api/modules';

vi.mock('$lib/api/client', () => ({
	apiClient: {
		get: vi.fn(),
		post: vi.fn()
	}
}));

import { apiClient } from '$lib/api/client';

describe('modules API', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('unwraps enabled modules from the backend response', async () => {
		const modules = [
			{
				id: '1',
				module_key: 'notes',
				display_name: 'Notes',
				description: 'Shared notes',
				enabled: true,
				root_path: '/Notes',
				renderer: 'notes',
				default_template: null,
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
					sidebar: { enabled: true, order: 1, icon: 'sticky-note', label: 'Notes' },
					dashboard: {
						enabled: true,
						order: 1,
						cardTitle: 'Notes',
						cardDescription: 'Shared notes',
						summaryMode: 'recent-items',
						maxItems: 4
					}
				},
				created_at: '2026-04-30T00:00:00Z',
				updated_at: '2026-04-30T00:00:00Z'
			}
		];

		vi.mocked(apiClient.get).mockResolvedValue({ modules });

		await expect(listEnabledModules()).resolves.toEqual(modules);
		expect(apiClient.get).toHaveBeenCalledWith('/modules');
	});

	it('unwraps a single module detail response', async () => {
		const module = {
			id: '1',
			module_key: 'notes',
			display_name: 'Notes',
			description: 'Shared notes',
			enabled: true,
			root_path: '/Notes',
			renderer: 'notes',
			default_template: null,
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
			ui_config: {},
			created_at: '2026-04-30T00:00:00Z',
			updated_at: '2026-04-30T00:00:00Z'
		};

		vi.mocked(apiClient.get).mockResolvedValue({ module });

		await expect(getModule('notes')).resolves.toEqual(module);
		expect(apiClient.get).toHaveBeenCalledWith('/modules/notes');
	});

	it('accepts the legacy raw module detail response shape', async () => {
		const module = {
			id: 'legacy-1',
			module_key: 'kanban',
			display_name: 'Kanban Dashboard',
			description: 'Manage board cards as folders and files.',
			enabled: true,
			root_path: '/Kanban',
			renderer: 'kanban',
			default_template: 'template_default_kanban',
			icon: 'columns',
			schema_version: '1.0',
			permissions: {
				admin_can_configure: true,
				workspace_members_can_use: true,
				allow_public_share: false,
				allow_internal_share: true
			},
			ai_indexing: { enabled: true },
			audit: { enabled: true },
			ui_config: {},
			created_at: '2026-04-30T00:00:00Z',
			updated_at: '2026-04-30T00:00:00Z'
		};

		vi.mocked(apiClient.get).mockResolvedValue(module);

		await expect(getModule('kanban')).resolves.toEqual(module);
		expect(apiClient.get).toHaveBeenCalledWith('/modules/kanban');
	});

	it('unwraps module summary payloads', async () => {
		const summary = {
			module_key: 'notes',
			mode: 'recent-items',
			total_items: 2,
			recent_items: [
				{ id: 'a', name: 'Alpha', item_type: 'file' as const, updated_at: '2026-04-30T00:00:00Z' }
			]
		};

		vi.mocked(apiClient.get).mockResolvedValue({ summary });

		await expect(getModuleSummary('notes')).resolves.toEqual(summary);
		expect(apiClient.get).toHaveBeenCalledWith('/modules/notes/summary');
	});

	it('passes through create-from-template requests unchanged', async () => {
		const request = {
			template_key: 'meetings',
			name: 'Planning',
			parent_folder_id: 'folder-1'
		};
		const response = {
			object_id: 'obj-1',
			object_type: 'folder' as const,
			path: '/Notes/Planning'
		};

		vi.mocked(apiClient.post).mockResolvedValue(response);

		await expect(createFromTemplate(request)).resolves.toEqual(response);
		expect(apiClient.post).toHaveBeenCalledWith('/modules/from-template', request);
	});
});
