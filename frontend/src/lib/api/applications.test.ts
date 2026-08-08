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

	const shellEntry = {
		manifest: {
			apiVersion: 'elembra.io/v1alpha1',
			kind: 'Application' as const,
			metadata: {
				id: 'io.elembra.notes',
				name: 'Notes',
				version: '1.0.0',
				description: 'Shared notes'
			},
			runtime: { kind: 'embedded' as const },
			contracts: { provides: [], requires: [] },
			resources: [],
			contributions: {
				navigation: [{ id: 'notes.navigation', label: 'Notes', route: '/apps/notes', order: 1 }],
				routes: [{ id: 'notes.route', route: '/apps/notes', renderer: 'okf-note' }],
				commands: [],
				dashboard: [{ id: 'notes.dashboard', renderer: 'latest-notes', order: 1 }],
				settings: [],
				searchProviders: [],
				renderers: [],
				admin: []
			},
			integrationEvents: { publishes: [], subscribes: [] },
			configuration: { schema: 'config.json' },
			data: { owner: 'io.elembra.notes', preserveOnDisable: true, exportSupported: true }
		},
		enabled: true,
		configuration: {},
		health: 'healthy' as const
	};

	it('unwraps enabled applications from the backend response', async () => {
		vi.mocked(apiClient.get).mockResolvedValue({ applications: [shellEntry] });

		await expect(listEnabledApplications()).resolves.toEqual([shellEntry]);
		expect(apiClient.get).toHaveBeenCalledWith('/applications');
	});

	it('unwraps a single application detail response', async () => {
		vi.mocked(apiClient.get).mockResolvedValue({ application: shellEntry });

		await expect(getApplication('notes')).resolves.toEqual(shellEntry);
		expect(apiClient.get).toHaveBeenCalledWith('/applications/notes');
	});

	it('unwraps application summary payloads', async () => {
		const summary = {
			application_id: 'io.elembra.notes',
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
