import { describe, expect, it } from 'vitest';
import type { ApplicationConfig } from '$lib/api/types';
import {
	getApplicationDashboardWidgetConfig,
	getApplicationPageConfig,
	getApplicationSidebarConfig,
	getEnabledSidebarModules,
	getEnabledDashboardModules,
	normalizeApplicationUiConfig
} from './workspaceSurface';

const baseModule: ApplicationConfig = {
	id: 'module-notes',
	application_id: 'notes',
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
		} satisfies ApplicationConfig;

		const normalized = normalizeApplicationUiConfig(module);

		expect(normalized.dashboard?.widget?.enabled).toBe(true);
		expect(normalized.dashboard?.widget?.type).toBe('latest-notes');
		expect(normalized.dashboard?.widget?.title).toBe('Latest Notes');
		expect(normalized.dashboard?.widget?.columns.desktop).toBe(3);
		expect(normalized.page?.enabled).toBe(true);
		expect(normalized.page?.route).toBe('/apps/notes');
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
					route: '/apps/notes',
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
		} satisfies ApplicationConfig;

		expect(getApplicationSidebarConfig(module).label).toBe('Notes');
		expect(getApplicationDashboardWidgetConfig(module).type).toBe('latest-notes');
		expect(getApplicationPageConfig(module).renderer).toBe('notes');
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
		} satisfies ApplicationConfig;

		const normalized = normalizeApplicationUiConfig(module);
		expect(normalized.page?.layout).toBe('kanban-board');
		expect(normalized.page?.emptyStateTitle).toBe('No boards yet');
		expect(normalized.modulePage?.layout).toBe('kanban-board');
	});
});

describe('workspace surface contract drift guard', () => {
	it('does not drift from canonical widget types', () => {
		const module = {
			...baseModule,
			application_id: 'kanban'
		} satisfies ApplicationConfig;

		const widget = getApplicationDashboardWidgetConfig(module);
		expect(widget.type).toBe('kanban-summary');
	});

	it('preserves canonical page layouts and does not drift to unknown values', () => {
		const kanban = {
			...baseModule,
			application_id: 'kanban',
			renderer: 'kanban',
			ui_config: {
				page: {
					enabled: true,
					route: '/apps/kanban',
					renderer: 'kanban',
					layout: 'kanban-board',
					emptyStateTitle: '',
					emptyStateDescription: '',
					emptyStateAction: ''
				}
			}
		} satisfies ApplicationConfig;
		const notes = {
			...baseModule,
			application_id: 'notes',
			renderer: 'notes'
		} satisfies ApplicationConfig;

		expect(getApplicationPageConfig(kanban).layout).toBe('kanban-board');
		expect(getApplicationPageConfig(notes).layout).toBe('list-grid');
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
						primaryAction: {
							label: 'New Note',
							action: 'create-from-template',
							template: 'template_default_note'
						}
					}
				},
				page: {
					enabled: true,
					route: '/apps/notes',
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
		} satisfies ApplicationConfig;

		const normalized = normalizeApplicationUiConfig(module);
		expect(normalized.sidebar?.enabled).toBe(true);
		expect(normalized.dashboard?.widget?.type).toBe('latest-notes');
		expect(normalized.page?.route).toBe('/apps/notes');
		expect(normalized.page?.renderer).toBe('notes');
		expect(normalized.page?.layout).toBe('list-grid');
	});

	it('filters disabled modules from sidebar', () => {
		const enabled = {
			...baseModule,
			ui_config: {
				sidebar: { enabled: true, order: 1, icon: 'sticky-note', label: 'Notes' }
			}
		} satisfies ApplicationConfig;
		const disabled = {
			...baseModule,
			application_id: 'disabled',
			enabled: false,
			ui_config: {
				sidebar: { enabled: true, order: 2, icon: 'folder', label: 'Disabled' }
			}
		} satisfies ApplicationConfig;
		const sidebarHidden = {
			...baseModule,
			application_id: 'hidden',
			enabled: true,
			ui_config: {
				sidebar: { enabled: false, order: 3, icon: 'folder', label: 'Hidden' }
			}
		} satisfies ApplicationConfig;

		const result = getEnabledSidebarModules([enabled, disabled, sidebarHidden]);
		expect(result.map((m) => m.application_id)).toEqual(['notes']);
	});

	it('filters disabled modules from dashboard', () => {
		const enabled = {
			...baseModule,
			ui_config: {
				dashboard: {
					enabled: true,
					order: 1,
					widget: {
						enabled: true,
						type: 'latest-notes',
						title: 'Notes',
						description: 'Recent notes.',
						size: 'small' as const,
						columns: { desktop: 3, tablet: 6, mobile: 12 },
						maxItems: 4
					}
				}
			}
		} satisfies ApplicationConfig;
		const disabled = {
			...baseModule,
			application_id: 'disabled',
			enabled: false,
			ui_config: {
				dashboard: {
					enabled: true,
					order: 2,
					widget: {
						enabled: true,
						type: 'generic-module-summary',
						title: 'Disabled',
						description: 'Disabled module.',
						size: 'small' as const,
						columns: { desktop: 3, tablet: 6, mobile: 12 },
						maxItems: 4
					}
				}
			}
		} satisfies ApplicationConfig;
		const dashboardHidden = {
			...baseModule,
			application_id: 'hidden',
			enabled: true,
			ui_config: {
				dashboard: {
					enabled: false,
					order: 3,
					widget: {
						enabled: false,
						type: 'generic-module-summary',
						title: 'Hidden',
						description: 'Hidden module.',
						size: 'small' as const,
						columns: { desktop: 3, tablet: 6, mobile: 12 },
						maxItems: 4
					}
				}
			}
		} satisfies ApplicationConfig;

		const result = getEnabledDashboardModules([enabled, disabled, dashboardHidden]);
		expect(result.map((m) => m.application_id)).toEqual(['notes']);
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
			const module = { ...baseModule, application_id: key } satisfies ApplicationConfig;
			const widget = getApplicationDashboardWidgetConfig(module);
			expect(widget.type).toBe(expectedWidgetType);
		}
	});
});
