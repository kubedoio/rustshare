import { describe, expect, it } from 'vitest';
import type { ModuleConfig } from '$lib/api/types';
import {
	getModuleDashboardWidgetConfig,
	getModulePageConfig,
	getModuleSidebarConfig,
	normalizeModuleUiConfig
} from './workspaceSurface';

const baseModule: ModuleConfig = {
	id: 'module-notes',
	module_key: 'notes',
	display_name: 'Notes',
	description: 'Recent notes.',
	enabled: true,
	root_path: '/Workspace/Notes',
	renderer: 'notes',
	default_template: 'template_default_note',
	icon: 'sticky-note',
	schema_version: '1.0',
	permissions: {
		admin_can_configure: true,
		workspace_members_can_use: true,
		allow_public_share: false,
		allow_internal_share: true
	},
	ai_indexing: { enabled: true },
	audit: { enabled: true },
	created_at: '2026-04-30T00:00:00Z',
	updated_at: '2026-04-30T00:00:00Z'
};

describe('workspace surface module normalization', () => {
	it('normalizes legacy dashboard and modulePage config into widget and page contracts', () => {
		const module = {
			...baseModule,
			ui_config: {
				sidebar: {
					enabled: true,
					order: 30,
					icon: 'sticky-note',
					label: 'Notes'
				},
				dashboard: {
					enabled: true,
					order: 10,
					cardTitle: 'Latest Notes',
					cardDescription: 'Recent file-backed notes.',
					summaryMode: 'latest-notes',
					maxItems: 4,
					primaryAction: {
						label: 'New Note',
						action: 'create-from-template',
						template: 'template_default_note'
					}
				},
				modulePage: {
					layout: 'list-grid',
					emptyStateTitle: 'No notes yet',
					emptyStateDescription: 'Create your first note.',
					emptyStateAction: 'New Note'
				}
			}
		} satisfies ModuleConfig;

		const normalized = normalizeModuleUiConfig(module);

		expect(normalized.dashboard?.widget?.enabled).toBe(true);
		expect(normalized.dashboard?.widget?.type).toBe('latest-notes');
		expect(normalized.dashboard?.widget?.title).toBe('Latest Notes');
		expect(normalized.dashboard?.widget?.columns.desktop).toBe(3);
		expect(normalized.page?.enabled).toBe(true);
		expect(normalized.page?.route).toBe('/modules/notes');
		expect(normalized.page?.renderer).toBe('notes');
		expect(normalized.page?.primaryAction?.label).toBe('New Note');
	});

	it('preserves explicit workspace widget and page config when already present', () => {
		const module = {
			...baseModule,
			ui_config: {
				sidebar: {
					enabled: true,
					order: 30,
					icon: 'sticky-note',
					label: 'Notes'
				},
				dashboard: {
					enabled: true,
					order: 10,
					widget: {
						enabled: true,
						type: 'latest-notes',
						title: 'Latest Notes',
						description: 'Recent file-backed notes.',
						size: 'small',
						columns: { desktop: 3, tablet: 6, mobile: 12 },
						maxItems: 4,
						primaryAction: {
							label: 'New Note',
							action: 'create-from-template',
							template: 'template_default_note'
						}
					}
				},
				page: {
					enabled: true,
					route: '/modules/notes',
					renderer: 'notes',
					layout: 'list-grid',
					emptyStateTitle: 'No notes yet',
					emptyStateDescription: 'Create your first note.',
					emptyStateAction: 'New Note',
					primaryAction: {
						label: 'New Note',
						action: 'create-from-template',
						template: 'template_default_note'
					}
				}
			}
		} satisfies ModuleConfig;

		expect(getModuleSidebarConfig(module).label).toBe('Notes');
		expect(getModuleDashboardWidgetConfig(module).type).toBe('latest-notes');
		expect(getModulePageConfig(module).renderer).toBe('notes');
	});
});
