import { describe, expect, it } from 'vitest';
import { normalizeApplicationConfig } from '$lib/applications/workspaceSurface';
import { PREDEFINED_MODULES, getApplicationByKey } from '$lib/applications/registry';
import type { ApplicationConfig } from '$lib/api/types';

describe('admin-applications config normalization', () => {
	it('preserves OKF documentFormat and okf block from server ui_config', () => {
		const serverModule: ApplicationConfig = {
			id: '00000000-0000-0000-0000-000000000000',
			application_id: 'notes',
			display_name: 'Notes',
			description: 'OKF notes.',
			enabled: true,
			root_path: '/Workspace/Notes',
			renderer: 'okf-note',
			default_template: 'template_default_okf_note',
			icon: 'sticky-note',
			schema_version: '1.0',
			permissions: {
				admin_can_configure: true,
				workspace_members_can_use: true,
				allow_public_share: false,
				allow_internal_share: true
			},
			ai_indexing: {
				enabled: true,
				source: 'okf-frontmatter-and-markdown',
				permission_aware: true
			},
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
			created_at: '2026-01-01T00:00:00Z',
			updated_at: '2026-01-01T00:00:00Z'
		};

		const normalized = normalizeApplicationConfig(serverModule);
		expect(normalized.ui_config?.documentFormat).toBe('okf-markdown');
		expect(normalized.ui_config?.okf?.enabled).toBe(true);
		expect(normalized.ui_config?.okf?.conceptType).toBe('Note');
		expect(normalized.ui_config?.okf?.frontmatterRequired).toBe(true);
	});
});

describe('predefined module registry', () => {
	it('defines Notes as an OKF-native module', () => {
		const notes = getApplicationByKey('notes');
		expect(notes).toBeDefined();
		expect(notes!.renderer).toBe('okf-note');
		expect(notes!.documentFormat).toBe('okf-markdown');
		expect(notes!.defaultTemplate).toBe('template_default_okf_note');
		expect(notes!.okf).toEqual({
			enabled: true,
			conceptType: 'Note',
			frontmatterRequired: true,
			preserveUnknownFields: true
		});
	});

	it('keeps non-notes modules off the OKF-native renderer', () => {
		for (const module of PREDEFINED_MODULES) {
			if (module.key === 'notes') continue;
			expect(module.renderer).not.toBe('okf-note');
			expect(module.defaultTemplate).not.toBe('template_default_okf_note');
		}
	});
});
