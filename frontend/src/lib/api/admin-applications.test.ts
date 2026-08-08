import { describe, expect, it } from 'vitest';
import { normalizeApplicationConfig } from '$lib/applications/workspaceSurface';
import type { ApplicationConfig } from '$lib/api/types';

describe('application configuration normalization', () => {
	it('preserves declarative OKF configuration from an application projection', () => {
		const application: ApplicationConfig = {
			id: 'io.elembra.notes',
			application_id: 'io.elembra.notes',
			display_name: 'Notes',
			description: 'OKF notes.',
			enabled: true,
			root_path: '/Workspace/Notes',
			renderer: 'okf-note',
			default_template: 'template_default_okf_note',
			icon: 'sticky-note',
			schema_version: 'elembra.io/v1alpha1',
			permissions: {
				admin_can_configure: true,
				workspace_members_can_use: true,
				allow_public_share: false,
				allow_internal_share: true
			},
			ai_indexing: { enabled: true },
			audit: { enabled: true },
			ui_config: {
				documentFormat: 'okf-markdown',
				okf: {
					enabled: true,
					conceptType: 'Note',
					frontmatterRequired: true,
					preserveUnknownFields: true
				}
			},
			created_at: '',
			updated_at: ''
		};

		const normalized = normalizeApplicationConfig(application);
		expect(normalized.ui_config?.documentFormat).toBe('okf-markdown');
		expect(normalized.ui_config?.okf?.enabled).toBe(true);
	});
});
