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
	searchPlaceholder?: string;
	filterLabel?: string;
	sortLabel?: string;
	itemSingular?: string;
	itemPlural?: string;
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
	'path-separation',
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
		description: 'Write and keep file-backed notes in your workspace.',
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
						label: 'New note',
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
				emptyStateDescription:
					'No notes yet. Create your first note to capture ideas, documentation, or working knowledge.',
				primaryAction: {
					label: 'New note',
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
				emptyStateDescription:
					'No meeting notes yet. Create a meeting note to capture agenda, discussion, decisions, and follow-up items.',
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
		displayName: 'Standup Records',
		description: 'Capture simple daily updates, blockers, and follow-up items.',
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
			sidebar: { enabled: true, order: 30, icon: 'activity', label: 'Standup Records' },
			dashboard: {
				enabled: true,
				order: 30,
				widget: {
					enabled: true,
					type: 'standups-recent',
					title: 'Standup Records',
					description: 'Recent standup records.',
					size: 'medium',
					columns: { desktop: 6, tablet: 12, mobile: 12 },
					maxItems: 4,
					primaryAction: {
						label: 'New standup',
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
				emptyStateTitle: 'No standup records yet',
				emptyStateDescription:
					'No standup records yet. Create a daily update to capture progress, blockers, and follow-up items.',
				primaryAction: {
					label: 'New standup',
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
		displayName: 'Kanban',
		description: 'Organize lightweight work boards in your workspace.',
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
					description: 'Recent boards.',
					size: 'large',
					columns: { desktop: 6, tablet: 12, mobile: 12 },
					maxItems: 4,
					primaryAction: {
						label: 'New board',
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
				emptyStateDescription:
					'No boards yet. Create a lightweight board to organize work, ideas, or follow-up items.',
				primaryAction: {
					label: 'New board',
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
		description: 'Record important decisions with context and rationale.',
		enabled: true,
		rootPath: getModuleRoot('Decisions'),
		renderer: 'decisions',
		defaultTemplate: 'template_default_decision',
		schemaVersion: '1.0',
		permissions: {
			adminCanConfigure: true,
			workspaceMembersCanUse: true,
			allowPublicShare: false,
			allowInternalShare: true
		},
		ui: {
			sidebar: { enabled: true, order: 50, icon: 'path-separation', label: 'Decisions' },
			dashboard: {
				enabled: true,
				order: 50,
				widget: {
					enabled: true,
					type: 'decisions-recent',
					title: 'Decisions',
					description: 'Recent decision records.',
					size: 'medium',
					columns: { desktop: 6, tablet: 12, mobile: 12 },
					maxItems: 4,
					primaryAction: {
						label: 'New decision',
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
				emptyStateDescription:
					'No decisions yet. Create a decision record to preserve context, rationale, and follow-up.',
				primaryAction: {
					label: 'New decision',
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
		description: 'Capture sketches, flows, and early ideas as visual workspace boards.',
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
			sidebar: { enabled: true, order: 55, icon: 'lightbulb', label: 'Brainstorming' },
			dashboard: {
				enabled: true,
				order: 55,
				widget: {
					enabled: true,
					type: 'recent-brainstorm-boards',
					title: 'Brainstorming',
					description: 'Recent idea boards.',
					size: 'medium',
					columns: { desktop: 6, tablet: 12, mobile: 12 },
					maxItems: 4,
					primaryAction: {
						label: 'New idea board',
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
				emptyStateTitle: 'No idea boards yet',
				emptyStateDescription:
					'No idea boards yet. Create a simple visual board to capture sketches, flows, or early thinking.',
				primaryAction: {
					label: 'New idea board',
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
		description: 'Manage items shared from your workspace.',
		enabled: true,
		rootPath: getModuleRoot('Shares'),
		renderer: 'shares',
		defaultTemplate: null,
		schemaVersion: '1.0',
		permissions: {
			adminCanConfigure: true,
			workspaceMembersCanUse: true,
			allowPublicShare: false,
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
					description: 'Recent shares.',
					size: 'medium',
					columns: { desktop: 6, tablet: 12, mobile: 12 },
					maxItems: 4,
					primaryAction: { label: 'New share', action: 'generic-create' }
				}
			},
			page: {
				enabled: true,
				route: '/modules/shares',
				renderer: 'shares',
				layout: 'share-manager',
				emptyStateTitle: 'No active shares',
				emptyStateDescription: 'No active shares. Share a file or folder when you are ready.',
				primaryAction: { label: 'New share', action: 'generic-create' }
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
								primaryAction: uiConfig?.page?.primaryAction ?? m.ui.page.primaryAction,
								searchPlaceholder: uiConfig?.page?.searchPlaceholder ?? m.ui.page.searchPlaceholder,
								filterLabel: uiConfig?.page?.filterLabel ?? m.ui.page.filterLabel,
								sortLabel: uiConfig?.page?.sortLabel ?? m.ui.page.sortLabel,
								itemSingular: uiConfig?.page?.itemSingular ?? m.ui.page.itemSingular,
								itemPlural: uiConfig?.page?.itemPlural ?? m.ui.page.itemPlural
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
