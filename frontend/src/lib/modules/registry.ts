import type { User } from '$lib/api/types';
import { getModuleRoot } from './modulePaths';

export interface ModuleUiSidebar {
	enabled: boolean;
	order: number;
	icon: string;
	label: string;
}

export interface ModuleUiDashboardWidgetColumns {
	desktop: number;
	tablet: number;
	mobile: number;
}

export interface ModuleUiPrimaryAction {
	label: string;
	action: string;
	template?: string;
}

export interface ModuleUiDashboardWidget {
	enabled: boolean;
	type: string;
	title: string;
	description: string;
	size: 'small' | 'medium' | 'large';
	columns: ModuleUiDashboardWidgetColumns;
	maxItems: number;
	primaryAction?: ModuleUiPrimaryAction;
}

export interface ModuleUiDashboard {
	enabled: boolean;
	order: number;
	widget: ModuleUiDashboardWidget;
}

export interface ModuleUiPage {
	enabled: boolean;
	route: string;
	renderer: string;
	layout: string;
	emptyStateTitle: string;
	emptyStateDescription: string;
	primaryAction?: ModuleUiPrimaryAction;
}

export interface ModuleUiDefinition {
	sidebar: ModuleUiSidebar;
	dashboard: ModuleUiDashboard;
	page: ModuleUiPage;
}

export interface ModulePermissions {
	adminCanConfigure: boolean;
	workspaceMembersCanUse: boolean;
	allowPublicShare: boolean;
	allowInternalShare: boolean;
}

export interface WorkspaceSurfaceSection {
	key: string;
	type: string;
	enabled: boolean;
	order: number;
	title?: string;
	renderer: string;
}

export interface WorkspaceSurfaceLayout {
	type: string;
	columns: number;
	gap: number;
	compactOverview: boolean;
}

export interface WorkspaceSurfaceDefinition {
	id: string;
	key: string;
	name: string;
	version: string;
	enabled: boolean;
	layout: WorkspaceSurfaceLayout;
	sections: WorkspaceSurfaceSection[];
}

export interface ModuleDefinition {
	id: string;
	key: string;
	displayName: string;
	description: string;
	enabled: boolean;
	rootPath: string;
	renderer: string;
	defaultTemplate: string | null;
	schemaVersion: string;
	permissions: ModulePermissions;
	ui: ModuleUiDefinition;
	aiIndexing: { enabled: boolean };
	audit: { enabled: boolean };
}

const APPROVED_ICONS = new Set([
	'layout-dashboard',
	'folder',
	'file-text',
	'sticky-note',
	'calendar-days',
	'clipboard-list',
	'columns',
	'git-branch',
	'share-2',
	'lock',
	'globe',
	'settings',
	'activity',
	'users'
]);

export function isValidIconKey(icon: string): boolean {
	return APPROVED_ICONS.has(icon);
}

export const PREDEFINED_MODULES: ModuleDefinition[] = [
	{
		id: 'module_notes',
		key: 'notes',
		displayName: 'Notes',
		description: 'Capture file-backed notes and reusable knowledge.',
		enabled: true,
		rootPath: getModuleRoot('Notes'),
		renderer: 'notes',
		defaultTemplate: 'template_default_note',
		schemaVersion: '1.0',
		permissions: {
			adminCanConfigure: true,
			workspaceMembersCanUse: true,
			allowPublicShare: false,
			allowInternalShare: true
		},
		ui: {
			sidebar: { enabled: true, order: 10, icon: 'sticky-note', label: 'Notes' },
			dashboard: {
				enabled: true,
				order: 10,
				widget: {
					enabled: true,
					type: 'notes-recent',
					title: 'Notes',
					description: 'Recent file-backed notes.',
					size: 'medium',
					columns: { desktop: 6, tablet: 12, mobile: 12 },
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
				emptyStateDescription: 'Create your first file-backed note.',
				primaryAction: {
					label: 'New Note',
					action: 'create-from-template',
					template: 'template_default_note'
				}
			}
		},
		aiIndexing: { enabled: true },
		audit: { enabled: true }
	},
	{
		id: 'module_meeting_notes',
		key: 'meetings',
		displayName: 'Meeting Notes',
		description: 'Record simple meeting notes, decisions, and follow-up items.',
		enabled: true,
		rootPath: getModuleRoot('Meetings'),
		renderer: 'meetings',
		defaultTemplate: 'template_default_meeting',
		schemaVersion: '1.0',
		permissions: {
			adminCanConfigure: true,
			workspaceMembersCanUse: true,
			allowPublicShare: false,
			allowInternalShare: true
		},
		ui: {
			sidebar: { enabled: true, order: 20, icon: 'calendar-days', label: 'Meeting Notes' },
			dashboard: {
				enabled: true,
				order: 20,
				widget: {
					enabled: true,
					type: 'meetings-recent',
					title: 'Meetings',
					description: 'Recent meeting notes.',
					size: 'medium',
					columns: { desktop: 6, tablet: 12, mobile: 12 },
					maxItems: 4,
					primaryAction: {
						label: 'New meeting note',
						action: 'create-from-template',
						template: 'template_default_meeting'
					}
				}
			},
			page: {
				enabled: true,
				route: '/modules/meetings',
				renderer: 'meetings',
				layout: 'list-grid',
				emptyStateTitle: 'No meeting notes yet',
				emptyStateDescription: 'No meeting notes yet. Create a meeting note to capture agenda, discussion, decisions, and follow-up items.',
				primaryAction: {
					label: 'New meeting note',
					action: 'create-from-template',
					template: 'template_default_meeting'
				}
			}
		},
		aiIndexing: { enabled: true },
		audit: { enabled: true }
	},
	{
		id: 'module_standups',
		key: 'standups',
		displayName: 'Standups',
		description: 'Daily standup records.',
		enabled: true,
		rootPath: getModuleRoot('Standups'),
		renderer: 'standups',
		defaultTemplate: 'template_default_standup',
		schemaVersion: '1.0',
		permissions: {
			adminCanConfigure: true,
			workspaceMembersCanUse: true,
			allowPublicShare: false,
			allowInternalShare: true
		},
		ui: {
			sidebar: { enabled: true, order: 30, icon: 'activity', label: 'Standups' },
			dashboard: {
				enabled: true,
				order: 30,
				widget: {
					enabled: true,
					type: 'standups-recent',
					title: 'Standups',
					description: 'Recent standups.',
					size: 'medium',
					columns: { desktop: 6, tablet: 12, mobile: 12 },
					maxItems: 4,
					primaryAction: {
						label: 'New Standup',
						action: 'create-from-template',
						template: 'template_default_standup'
					}
				}
			},
			page: {
				enabled: true,
				route: '/modules/standups',
				renderer: 'standups',
				layout: 'list-grid',
				emptyStateTitle: 'No standups yet',
				emptyStateDescription: 'Create your first standup record.',
				primaryAction: {
					label: 'New Standup',
					action: 'create-from-template',
					template: 'template_default_standup'
				}
			}
		},
		aiIndexing: { enabled: true },
		audit: { enabled: true }
	},
	{
		id: 'module_kanban',
		key: 'kanban',
		displayName: 'Kanban Dashboard',
		description: 'Project boards and task tracking.',
		enabled: true,
		rootPath: getModuleRoot('Kanban'),
		renderer: 'kanban',
		defaultTemplate: 'template_default_kanban',
		schemaVersion: '1.0',
		permissions: {
			adminCanConfigure: true,
			workspaceMembersCanUse: true,
			allowPublicShare: false,
			allowInternalShare: true
		},
		ui: {
			sidebar: { enabled: true, order: 40, icon: 'columns', label: 'Kanban' },
			dashboard: {
				enabled: true,
				order: 40,
				widget: {
					enabled: true,
					type: 'kanban-summary',
					title: 'Kanban',
					description: 'Active boards and cards.',
					size: 'large',
					columns: { desktop: 6, tablet: 12, mobile: 12 },
					maxItems: 4,
					primaryAction: {
						label: 'New Board',
						action: 'create-from-template',
						template: 'template_default_kanban'
					}
				}
			},
			page: {
				enabled: true,
				route: '/modules/kanban',
				renderer: 'kanban',
				layout: 'kanban-board',
				emptyStateTitle: 'No boards yet',
				emptyStateDescription: 'Create your first kanban board.',
				primaryAction: {
					label: 'New Board',
					action: 'create-from-template',
					template: 'template_default_kanban'
				}
			}
		},
		aiIndexing: { enabled: true },
		audit: { enabled: true }
	},
	{
		id: 'module_decisions',
		key: 'decisions',
		displayName: 'Decisions',
		description: 'Architecture and product decisions.',
		enabled: true,
		rootPath: getModuleRoot('Decisions'),
		renderer: 'decisions',
		defaultTemplate: 'template_default_decision',
		schemaVersion: '1.0',
		permissions: {
			adminCanConfigure: true,
			workspaceMembersCanUse: true,
			allowPublicShare: true,
			allowInternalShare: true
		},
		ui: {
			sidebar: { enabled: true, order: 50, icon: 'git-branch', label: 'Decisions' },
			dashboard: {
				enabled: true,
				order: 50,
				widget: {
					enabled: true,
					type: 'decisions-recent',
					title: 'Decisions',
					description: 'Recent decisions.',
					size: 'medium',
					columns: { desktop: 6, tablet: 12, mobile: 12 },
					maxItems: 4,
					primaryAction: {
						label: 'New Decision',
						action: 'create-from-template',
						template: 'template_default_decision'
					}
				}
			},
			page: {
				enabled: true,
				route: '/modules/decisions',
				renderer: 'decisions',
				layout: 'decision-registry',
				emptyStateTitle: 'No decisions yet',
				emptyStateDescription: 'Create your first decision record.',
				primaryAction: {
					label: 'New Decision',
					action: 'create-from-template',
					template: 'template_default_decision'
				}
			}
		},
		aiIndexing: { enabled: true },
		audit: { enabled: true }
	},
	{
		id: 'module_brainstorming',
		key: 'brainstorming',
		displayName: 'Brainstorming',
		description: 'Visual decision boards and brainstorming whiteboards.',
		enabled: true,
		rootPath: getModuleRoot('Brainstorming'),
		renderer: 'brainstorming',
		defaultTemplate: 'template_blank_brainstorm',
		schemaVersion: '1.0',
		permissions: {
			adminCanConfigure: true,
			workspaceMembersCanUse: true,
			allowPublicShare: false,
			allowInternalShare: true
		},
		ui: {
			sidebar: { enabled: true, order: 55, icon: 'pen-tool', label: 'Brainstorming' },
			dashboard: {
				enabled: true,
				order: 55,
				widget: {
					enabled: true,
					type: 'recent-brainstorm-boards',
					title: 'Brainstorming',
					description: 'Recent visual decision boards.',
					size: 'medium',
					columns: { desktop: 6, tablet: 12, mobile: 12 },
					maxItems: 4,
					primaryAction: {
						label: 'New Board',
						action: 'create-from-template',
						template: 'template_blank_brainstorm'
					}
				}
			},
			page: {
				enabled: true,
				route: '/modules/brainstorming',
				renderer: 'brainstorming',
				layout: 'gallery-grid',
				emptyStateTitle: 'No brainstorming boards yet',
				emptyStateDescription: 'Create your first visual decision board.',
				primaryAction: {
					label: 'New Board',
					action: 'create-from-template',
					template: 'template_blank_brainstorm'
				}
			}
		},
		aiIndexing: { enabled: true },
		audit: { enabled: true }
	},
	{
		id: 'module_shares',
		key: 'shares',
		displayName: 'Shares',
		description: 'Shared external and internal files.',
		enabled: true,
		rootPath: getModuleRoot('Shares'),
		renderer: 'shares',
		defaultTemplate: null,
		schemaVersion: '1.0',
		permissions: {
			adminCanConfigure: true,
			workspaceMembersCanUse: true,
			allowPublicShare: true,
			allowInternalShare: true
		},
		ui: {
			sidebar: { enabled: true, order: 60, icon: 'share-2', label: 'Shares' },
			dashboard: {
				enabled: true,
				order: 60,
				widget: {
					enabled: true,
					type: 'shares-summary',
					title: 'Shares',
					description: 'Active shares.',
					size: 'medium',
					columns: { desktop: 6, tablet: 12, mobile: 12 },
					maxItems: 4,
					primaryAction: { label: 'New Share', action: 'generic-create' }
				}
			},
			page: {
				enabled: true,
				route: '/modules/shares',
				renderer: 'shares',
				layout: 'share-manager',
				emptyStateTitle: 'No shares',
				emptyStateDescription: 'You have not shared any files.',
				primaryAction: { label: 'New Share', action: 'generic-create' }
			}
		},
		aiIndexing: { enabled: true },
		audit: { enabled: true }
	}
];

export const DEFAULT_WORKSPACE_SURFACE: WorkspaceSurfaceDefinition = {
	id: 'surface_default_dashboard',
	key: 'dashboard',
	name: 'Default Workspace Dashboard',
	version: '1.0',
	enabled: true,
	layout: {
		type: 'grid',
		columns: 12,
		gap: 24,
		compactOverview: true
	},
	sections: [
		{
			key: 'overview',
			type: 'overview',
			enabled: true,
			order: 10,
			renderer: 'compact-workspace-overview'
		},
		{
			key: 'insights',
			type: 'widget-grid',
			enabled: true,
			order: 20,
			title: 'Workspace Summary & Insights',
			renderer: 'workspace-widget-grid'
		}
	]
};

import { writable, get } from 'svelte/store';
import { listEnabledModules } from '$lib/api/modules';

export const modulesStore = writable<ModuleDefinition[]>(PREDEFINED_MODULES);

export async function refreshModules() {
	try {
		const enabled = await listEnabledModules();
		modulesStore.update((current) => {
			return current.map((m) => {
				const serverModule = enabled.find((sm) => sm.module_key === m.key);
				if (serverModule) {
					const uiConfig = serverModule.ui_config;
					// Merge server-side config into predefined definition
					return {
						...m,
						displayName: serverModule.display_name,
						description: serverModule.description || m.description,
						enabled: serverModule.enabled,
						rootPath: serverModule.root_path || m.rootPath,
						renderer: serverModule.renderer || m.renderer,
						defaultTemplate: serverModule.default_template || m.defaultTemplate,
						ui: {
							sidebar: {
								enabled: uiConfig?.sidebar?.enabled ?? m.ui.sidebar.enabled,
								order: uiConfig?.sidebar?.order ?? m.ui.sidebar.order,
								icon: uiConfig?.sidebar?.icon ?? m.ui.sidebar.icon,
								label: uiConfig?.sidebar?.label ?? m.ui.sidebar.label
							},
							dashboard: {
								enabled: uiConfig?.dashboard?.enabled ?? m.ui.dashboard.enabled,
								order: uiConfig?.dashboard?.order ?? m.ui.dashboard.order,
								widget: {
									...m.ui.dashboard.widget,
									enabled: uiConfig?.dashboard?.widget?.enabled ?? m.ui.dashboard.widget.enabled,
									type: uiConfig?.dashboard?.widget?.type ?? m.ui.dashboard.widget.type,
									size: uiConfig?.dashboard?.widget?.size ?? m.ui.dashboard.widget.size,
									maxItems: uiConfig?.dashboard?.widget?.maxItems ?? m.ui.dashboard.widget.maxItems
								}
							},
							page: {
								enabled: uiConfig?.page?.enabled ?? m.ui.page.enabled,
								route: uiConfig?.page?.route ?? m.ui.page.route,
								renderer: uiConfig?.page?.renderer ?? m.ui.page.renderer,
								layout: uiConfig?.page?.layout ?? m.ui.page.layout,
								emptyStateTitle: uiConfig?.page?.emptyStateTitle ?? m.ui.page.emptyStateTitle,
								emptyStateDescription:
									uiConfig?.page?.emptyStateDescription ?? m.ui.page.emptyStateDescription,
								primaryAction: uiConfig?.page?.primaryAction ?? m.ui.page.primaryAction
							}
						}
					};
				}
				return m;
			});
		});
	} catch (err) {
		console.error('Failed to refresh modules:', err);
	}
}

export function getAllModules(): ModuleDefinition[] {
	return get(modulesStore);
}

export function getEnabledModules(): ModuleDefinition[] {
	return getAllModules().filter((m) => m.enabled);
}

export function getSidebarModulesForUser(user: User | null): ModuleDefinition[] {
	if (!user) return [];
	return getEnabledModules()
		.filter((m) => m.ui.sidebar.enabled && m.permissions.workspaceMembersCanUse)
		.sort((a, b) => a.ui.sidebar.order - b.ui.sidebar.order);
}

export function getDashboardModulesForUser(user: User | null): ModuleDefinition[] {
	if (!user) return [];
	return getEnabledModules()
		.filter((m) => m.ui.dashboard.enabled && m.permissions.workspaceMembersCanUse)
		.sort((a, b) => a.ui.dashboard.order - b.ui.dashboard.order);
}

export function getModuleByKey(key: string): ModuleDefinition | undefined {
	return getAllModules().find((m) => m.key === key);
}

export function filterModulesByUserPreference(
	modules: ModuleDefinition[],
	preferences: Record<string, boolean>
): ModuleDefinition[] {
	return modules.filter((m) => preferences[m.key] !== false);
}
