import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
	createFromTemplate,
	getApplication,
	getApplicationSummary,
	listEnabledApplications
} from '$lib/api/applications';

vi.mock('$lib/api/client', () => ({
	apiClient: {
		postVoid: vi.fn(),
		patchVoid: vi.fn(),
		requestText: vi.fn(),
		requestVoid: vi.fn(),
		get: vi.fn(),
		post: vi.fn()
	}
}));

import { apiClient } from '$lib/api/client';

describe('applications API', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('unwraps enabled applications from the backend response', async () => {
		const applications = [
			{
				id: '1',
				application_id: 'notes',
				display_name: 'Notes',
				description: 'Shared notes',
				enabled: true,
				root_path: '/Workspace/Notes',
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

		vi.mocked(apiClient.get).mockResolvedValue({ applications });

		await expect(listEnabledApplications()).resolves.toEqual([
			expect.objectContaining({
				application_id: 'notes',
				ui_config: expect.objectContaining({
					sidebar: expect.objectContaining({ label: 'Notes' }),
					dashboard: expect.objectContaining({
						summaryMode: 'recent-items',
						widget: expect.objectContaining({
							type: 'recent-items',
							title: 'Notes'
						})
					}),
					page: expect.objectContaining({
						route: '/apps/notes',
						renderer: 'notes'
					})
				})
			})
		]);
		expect(apiClient.get).toHaveBeenCalledWith('/applications');
	});

	it('unwraps a single application detail response', async () => {
		const application = {
			id: '1',
			application_id: 'notes',
			display_name: 'Notes',
			description: 'Shared notes',
			enabled: true,
			root_path: '/Workspace/Notes',
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

		vi.mocked(apiClient.get).mockResolvedValue({ application });

		await expect(getApplication('notes')).resolves.toEqual(
			expect.objectContaining({
				application_id: 'notes',
				ui_config: expect.objectContaining({
					dashboard: expect.objectContaining({
						widget: expect.objectContaining({ type: 'latest-notes' })
					}),
					page: expect.objectContaining({
						route: '/apps/notes',
						renderer: 'notes'
					})
				})
			})
		);
		expect(apiClient.get).toHaveBeenCalledWith('/applications/notes');
	});

	it('unwraps application summary payloads', async () => {
		const summary = {
			application_id: 'notes',
			mode: 'recent-items',
			total_items: 2,
			recent_items: [
				{ id: 'a', name: 'Alpha', item_type: 'file' as const, updated_at: '2026-04-30T00:00:00Z' }
			]
		};

		vi.mocked(apiClient.get).mockResolvedValue({ summary });

		await expect(getApplicationSummary('notes')).resolves.toEqual(summary);
		expect(apiClient.get).toHaveBeenCalledWith('/applications/notes/summary');
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
		expect(apiClient.post).toHaveBeenCalledWith('/applications/from-template', request);
	});
});
