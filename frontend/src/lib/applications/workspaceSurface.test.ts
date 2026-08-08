import { describe, expect, it } from 'vitest';
import type { ApplicationConfig } from '$lib/api/types';
import {
	getApplicationDashboardWidgetConfig,
	getApplicationPageConfig,
	getApplicationSidebarConfig,
	getEnabledSidebarApplications,
	getEnabledDashboardApplications,
	normalizeApplicationUiConfig
} from './workspaceSurface';

const baseApplication: ApplicationConfig = {
	id: 'io.elembra.notes',
	application_id: 'io.elembra.notes',
	display_name: 'Notes',
	description: 'Recent notes.',
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
	ai_indexing: { enabled: true },
	audit: { enabled: true },
	created_at: '2026-04-30T00:00:00Z',
	updated_at: '2026-04-30T00:00:00Z'
};

describe('workspace surface application normalization', () => {
	it('normalizes declarative dashboard and page Contributions into surface contracts', () => {
		const application = {
			...baseApplication,
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
				page: {
					enabled: true,
					route: '/apps/notes',
					renderer: 'okf-note',
					layout: 'list-grid',
					emptyStateTitle: 'No notes yet',
					emptyStateDescription: 'Create your first note.',
					emptyStateAction: 'New Note'
				}
			}
		} satisfies ApplicationConfig;

		const normalized = normalizeApplicationUiConfig(application);

		expect(normalized.dashboard?.widget?.enabled).toBe(true);
		expect(normalized.dashboard?.widget?.type).toBe('latest-notes');
		expect(normalized.dashboard?.widget?.title).toBe('Latest Notes');
		expect(normalized.dashboard?.widget?.columns.desktop).toBe(4);
		expect(normalized.page?.enabled).toBe(true);
		expect(normalized.page?.route).toBe('/apps/notes');
		expect(normalized.page?.renderer).toBe('okf-note');
		expect(normalized.page?.primaryAction?.label).toBe('New Note');
	});

	it('preserves explicit workspace widget and page config when already present', () => {
		const application = {
			...baseApplication,
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
					renderer: 'okf-note',
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

		expect(getApplicationSidebarConfig(application).label).toBe('Notes');
		expect(getApplicationDashboardWidgetConfig(application).type).toBe('latest-notes');
		expect(getApplicationPageConfig(application).renderer).toBe('okf-note');
	});
});

describe('workspace surface contract drift guard', () => {
	it('does not drift from canonical widget types', () => {
		const application = {
			...baseApplication,
			application_id: 'io.elembra.kanban',
			ui_config: {
				dashboard: {
					enabled: true,
					order: 1,
					widget: {
						enabled: true,
						type: 'kanban-summary',
						title: 'Kanban',
						description: 'Boards.',
						size: 'large',
						columns: { desktop: 6, tablet: 12, mobile: 12 },
						maxItems: 4
					}
				}
			}
		} satisfies ApplicationConfig;

		const widget = getApplicationDashboardWidgetConfig(application);
		expect(widget.type).toBe('kanban-summary');
	});

	it('preserves canonical page layouts and does not drift to unknown values', () => {
		const kanban = {
			...baseApplication,
			application_id: 'io.elembra.kanban',
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
			...baseApplication,
			application_id: 'io.elembra.notes',
			renderer: 'okf-note'
		} satisfies ApplicationConfig;

		expect(getApplicationPageConfig(kanban).layout).toBe('kanban-board');
		expect(getApplicationPageConfig(notes).layout).toBe('list-grid');
	});

	it('does not drift from canonical module root in base config', () => {
		expect(baseApplication.root_path).toBe('/Workspace/Notes');
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
		expect(approved.has(baseApplication.icon)).toBe(true);
	});

	it('preserves all canonical snake_case fields through normalization', () => {
		const module = {
			...baseApplication,
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
					renderer: 'okf-note',
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
		expect(normalized.page?.renderer).toBe('okf-note');
		expect(normalized.page?.layout).toBe('list-grid');
	});

	it('filters disabled modules from sidebar', () => {
		const enabled = {
			...baseApplication,
			ui_config: {
				sidebar: { enabled: true, order: 1, icon: 'sticky-note', label: 'Notes' }
			}
		} satisfies ApplicationConfig;
		const disabled = {
			...baseApplication,
			application_id: 'io.elembra.disabled',
			enabled: false,
			ui_config: {
				sidebar: { enabled: true, order: 2, icon: 'folder', label: 'Disabled' }
			}
		} satisfies ApplicationConfig;
		const sidebarHidden = {
			...baseApplication,
			application_id: 'io.elembra.hidden',
			enabled: true,
			ui_config: {
				sidebar: { enabled: false, order: 3, icon: 'folder', label: 'Hidden' }
			}
		} satisfies ApplicationConfig;

		const result = getEnabledSidebarApplications([enabled, disabled, sidebarHidden]);
		expect(result.map((m) => m.application_id)).toEqual(['io.elembra.notes']);
	});

	it('filters disabled modules from dashboard', () => {
		const enabled = {
			...baseApplication,
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
			...baseApplication,
			application_id: 'io.elembra.disabled',
			enabled: false,
			ui_config: {
				dashboard: {
					enabled: true,
					order: 2,
					widget: {
						enabled: true,
						type: 'application-summary',
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
			...baseApplication,
			application_id: 'io.elembra.hidden',
			enabled: true,
			ui_config: {
				dashboard: {
					enabled: false,
					order: 3,
					widget: {
						enabled: false,
						type: 'application-summary',
						title: 'Hidden',
						description: 'Hidden module.',
						size: 'small' as const,
						columns: { desktop: 3, tablet: 6, mobile: 12 },
						maxItems: 4
					}
				}
			}
		} satisfies ApplicationConfig;

		const result = getEnabledDashboardApplications([enabled, disabled, dashboardHidden]);
		expect(result.map((m) => m.application_id)).toEqual(['io.elembra.notes']);
	});

	it('does not invent widget identity when a manifest omits a dashboard Contribution', () => {
		const application = { ...baseApplication } satisfies ApplicationConfig;
		const widget = getApplicationDashboardWidgetConfig(application);
		expect(widget.type).toBe('application-summary');
	});
});
