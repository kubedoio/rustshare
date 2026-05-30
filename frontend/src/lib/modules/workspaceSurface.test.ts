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

	it('maps legacy modulePage to canonical page shape', () => {
		const module = {
			...baseModule,
			ui_config: {
				modulePage: {
					layout: 'kanban-board',
					emptyStateTitle: 'No boards yet',
					emptyStateDescription: 'Create your first board.',
					emptyStateAction: 'New Board'
				}
			}
		} satisfies ModuleConfig;

		const normalized = normalizeModuleUiConfig(module);
		expect(normalized.page?.layout).toBe('kanban-board');
		expect(normalized.page?.emptyStateTitle).toBe('No boards yet');
		expect(normalized.modulePage?.layout).toBe('kanban-board');
	});
});

describe('workspace surface contract drift guard', () => {
	it('does not drift from canonical widget types', () => {
		const module = {
			...baseModule,
			module_key: 'kanban'
		} satisfies ModuleConfig;

		const widget = getModuleDashboardWidgetConfig(module);
		expect(widget.type).toBe('kanban-summary');
	});

	it('preserves canonical page layouts and does not drift to unknown values', () => {
		const kanban = {
			...baseModule,
			module_key: 'kanban',
			renderer: 'kanban',
			ui_config: { page: { enabled: true, route: '/modules/kanban', renderer: 'kanban', layout: 'kanban-board', emptyStateTitle: '', emptyStateDescription: '', emptyStateAction: '' } }
		} satisfies ModuleConfig;
		const notes = { ...baseModule, module_key: 'notes', renderer: 'notes' } satisfies ModuleConfig;

		expect(getModulePageConfig(kanban).layout).toBe('kanban-board');
		expect(getModulePageConfig(notes).layout).toBe('list-grid');
	});

	it('does not drift from canonical module root in base config', () => {
		expect(baseModule.root_path).toBe('/Workspace/Notes');
	});

	it('does not drift from approved icon keys', () => {
		const approved = new Set([
			'layout-dashboard',
			'folder',
			'file-text',
			'sticky-note',
			'calendar-days',
			'clipboard-list',
			'columns',
			'git-branch',
			'path-separation',
			'share-2',
			'lock',
			'globe',
			'settings',
			'lightbulb',
			'activity'
		]);
		expect(approved.has(baseModule.icon)).toBe(true);
	});

	it('preserves all canonical snake_case fields through normalization', () => {
		const module = {
			...baseModule,
			ui_config: {
				sidebar: { enabled: true, order: 1, icon: 'sticky-note', label: 'Notes' },
				dashboard: {
					enabled: true,
					order: 1,
					widget: {
						enabled: true,
						type: 'latest-notes',
						title: 'Notes',
						description: 'Recent notes.',
						size: 'small',
						columns: { desktop: 3, tablet: 6, mobile: 12 },
						maxItems: 4,
						primaryAction: { label: 'New Note', action: 'create-from-template', template: 'template_default_note' }
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
					primaryAction: { label: 'New Note', action: 'create-from-template', template: 'template_default_note' }
				}
			}
		} satisfies ModuleConfig;

		const normalized = normalizeModuleUiConfig(module);
		expect(normalized.sidebar?.enabled).toBe(true);
		expect(normalized.dashboard?.widget?.type).toBe('latest-notes');
		expect(normalized.page?.route).toBe('/modules/notes');
		expect(normalized.page?.renderer).toBe('notes');
		expect(normalized.page?.layout).toBe('list-grid');
	});

	it('does not drift from canonical default widget types for all predefined modules', () => {
		// When the backend sends a minimal payload (no ui_config), the frontend
		// normalization must produce the canonical default widget types.
		const expectations: Record<string, string> = {
			notes: 'latest-notes',
			meetings: 'decisions-meetings-summary',
			standups: 'generic-module-summary',
			kanban: 'kanban-summary',
			decisions: 'generic-module-summary',
			brainstorming: 'recent-brainstorm-boards',
			shares: 'active-shares'
		};

		for (const [key, expectedWidgetType] of Object.entries(expectations)) {
			const module = { ...baseModule, module_key: key } satisfies ModuleConfig;
			const widget = getModuleDashboardWidgetConfig(module);
			expect(widget.type).toBe(expectedWidgetType);
		}
	});
});
