import type { User, ApplicationConfig } from '$lib/api/types';
import { getApplicationRoot } from './applicationPaths';
import { normalizeApplicationUiConfig } from './workspaceSurface';

export interface ApplicationUiSidebar {
	enabled: boolean;
	order: number;
	icon: string;
	label: string;
}

export interface ApplicationUiDashboardWidgetColumns {
	desktop: number;
	tablet: number;
	mobile: number;
}

export interface ApplicationUiPrimaryAction {
	label: string;
	action: string;
	template?: string;
}

export interface ApplicationUiDashboardWidget {
	enabled: boolean;
	type: string;
	title: string;
	description: string;
	size: 'small' | 'medium' | 'large';
	columns: ApplicationUiDashboardWidgetColumns;
	maxItems: number;
	primaryAction?: ApplicationUiPrimaryAction;
}

export interface ApplicationUiDashboard {
	enabled: boolean;
	order: number;
	widget: ApplicationUiDashboardWidget;
}

export interface ApplicationUiPage {
	enabled: boolean;
	route: string;
	renderer: string;
	layout: string;
	emptyStateTitle: string;
	emptyStateDescription: string;
	primaryAction?: ApplicationUiPrimaryAction;
	searchPlaceholder?: string;
	filterLabel?: string;
	sortLabel?: string;
	itemSingular?: string;
	itemPlural?: string;
}

export interface ApplicationUiDefinition {
	sidebar: ApplicationUiSidebar;
	dashboard: ApplicationUiDashboard;
	page: ApplicationUiPage;
}

export interface ApplicationPermissions {
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

export interface OkfApplicationConfig {
	enabled: boolean;
	conceptType: string;
	frontmatterRequired: boolean;
	preserveUnknownFields?: boolean;
}

export interface ApplicationDefinition {
	id: string;
	key: string;
	displayName: string;
	description: string;
	enabled: boolean;
	rootPath: string;
	renderer: string;
	documentFormat?: string;
	defaultTemplate: string | null;
	icon: string;
	schemaVersion: string;
	permissions: ApplicationPermissions;
	ui: ApplicationUiDefinition;
	okf?: OkfApplicationConfig;
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
	'lightbulb',
	'activity',
	'mail'
]);

export function isValidIconKey(icon: string): boolean {
	return APPROVED_ICONS.has(icon);
}

export const PREDEFINED_MODULES: ApplicationDefinition[] = [
	{
		id: 'module_notes',
		key: 'notes',
		displayName: 'Notes',
		description: 'Write OKF-compatible, file-backed notes for durable company memory.',
		enabled: true,
		rootPath: getApplicationRoot('Notes'),
		renderer: 'okf-note',
		documentFormat: 'okf-markdown',
		defaultTemplate: 'template_default_okf_note',
		icon: 'sticky-note',
		schemaVersion: '1.0',
		permissions: {
			adminCanConfigure: true,
			workspaceMembersCanUse: true,
			allowPublicShare: false,
			allowInternalShare: true
		},
		okf: {
			enabled: true,
			conceptType: 'Note',
			frontmatterRequired: true,
			preserveUnknownFields: true
		},
		ui: {
			sidebar: { enabled: true, order: 10, icon: 'sticky-note', label: 'Notes' },
			dashboard: {
				enabled: true,
				order: 10,
				widget: {
					enabled: true,
					type: 'latest-notes',
					title: 'Notes',
					description: 'Recent OKF notes.',
					size: 'medium',
					columns: { desktop: 6, tablet: 12, mobile: 12 },
					maxItems: 4,
					primaryAction: {
						label: 'New note',
						action: 'create-from-template',
						template: 'template_default_okf_note'
					}
				}
			},
			page: {
				enabled: true,
				route: '/apps/notes',
				renderer: 'okf-note',
				layout: 'list-grid',
				emptyStateTitle: 'No notes yet',
				emptyStateDescription:
					'No notes yet. Create your first OKF note to capture ideas, documentation, or working knowledge.',
				primaryAction: {
					label: 'New note',
					action: 'create-from-template',
					template: 'template_default_okf_note'
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
		rootPath: getApplicationRoot('Meetings'),
		renderer: 'meetings',
		defaultTemplate: 'template_default_meeting',
		icon: 'calendar-days',
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
				route: '/apps/meetings',
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
		rootPath: getApplicationRoot('Standups'),
		renderer: 'standups',
		defaultTemplate: 'template_default_standup',
		icon: 'activity',
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
				route: '/apps/standups',
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
		rootPath: getApplicationRoot('Kanban'),
		renderer: 'kanban',
		defaultTemplate: 'template_default_kanban',
		icon: 'columns',
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
				route: '/apps/kanban',
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
		rootPath: getApplicationRoot('Decisions'),
		renderer: 'decisions',
		defaultTemplate: 'template_default_decision',
		icon: 'path-separation',
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
				route: '/apps/decisions',
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
		rootPath: getApplicationRoot('Brainstorming'),
		renderer: 'brainstorming',
		defaultTemplate: 'template_blank_brainstorm',
		icon: 'lightbulb',
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
				route: '/apps/brainstorming',
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
		rootPath: getApplicationRoot('Shares'),
		renderer: 'shares',
		defaultTemplate: null,
		icon: 'share-2',
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
				route: '/apps/shares',
				renderer: 'shares',
				layout: 'share-manager',
				emptyStateTitle: 'No active shares',
				emptyStateDescription: 'No active shares. Share a file or folder when you are ready.',
				primaryAction: { label: 'New share', action: 'generic-create' }
			}
		},
		aiIndexing: { enabled: true },
		audit: { enabled: true }
	},
	{
		id: 'module_mail',
		key: 'mail',
		displayName: 'Mail',
		description: 'Import, archive, and reference email inside RustShare workspaces.',
		enabled: true,
		rootPath: getApplicationRoot('Mail'),
		renderer: 'mail-list',
		defaultTemplate: null,
		icon: 'mail',
		schemaVersion: '1.0',
		permissions: {
			adminCanConfigure: true,
			workspaceMembersCanUse: true,
			allowPublicShare: false,
			allowInternalShare: true
		},
		ui: {
			sidebar: { enabled: true, order: 65, icon: 'mail', label: 'Mail' },
			dashboard: {
				enabled: true,
				order: 65,
				widget: {
					enabled: true,
					type: 'mail-summary',
					title: 'Mail',
					description: 'Imported messages.',
					size: 'small',
					columns: { desktop: 3, tablet: 6, mobile: 12 },
					maxItems: 0,
					primaryAction: { label: 'Import mail', action: 'generic-create' }
				}
			},
			page: {
				enabled: true,
				route: '/apps/mail',
				renderer: 'mail-list',
				layout: 'list-grid',
				emptyStateTitle: 'No imported mail yet',
				emptyStateDescription:
					'No imported mail yet. Upload an .eml file or connect an IMAP account to import messages.',
				primaryAction: { label: 'Import mail', action: 'generic-create' },
				searchPlaceholder: 'Search messages...',
				filterLabel: 'All messages',
				sortLabel: 'Imported',
				itemSingular: 'message',
				itemPlural: 'messages'
			}
		},
		aiIndexing: { enabled: false },
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
import { listEnabledApplications } from '$lib/api/applications';

export function applicationConfigToDefinition(config: ApplicationConfig): ApplicationDefinition {
	const ui = normalizeApplicationUiConfig(config);
	const dashboard = ui.dashboard!;
	const widget = dashboard.widget!;
	const page = ui.page!;

	const uiConfig = config.ui_config ?? {};

	return {
		id: config.id,
		key: config.application_id,
		displayName: config.display_name,
		description: config.description,
		enabled: config.enabled,
		rootPath: config.root_path,
		renderer: config.renderer,
		documentFormat: (uiConfig.documentFormat as string) || undefined,
		defaultTemplate: config.default_template,
		icon: config.icon,
		schemaVersion: config.schema_version,
		permissions: {
			adminCanConfigure: config.permissions.admin_can_configure,
			workspaceMembersCanUse: config.permissions.workspace_members_can_use,
			allowPublicShare: config.permissions.allow_public_share,
			allowInternalShare: config.permissions.allow_internal_share
		},
		okf: (uiConfig.okf as OkfApplicationConfig) || undefined,
		ui: {
			sidebar: ui.sidebar!,
			dashboard: {
				enabled: dashboard.enabled,
				order: dashboard.order,
				widget: {
					enabled: widget.enabled,
					type: widget.type,
					title: widget.title,
					description: widget.description,
					size: widget.size,
					columns: widget.columns,
					maxItems: widget.maxItems,
					primaryAction: widget.primaryAction
						? {
								label: widget.primaryAction.label,
								action: widget.primaryAction.action,
								template: widget.primaryAction.template
							}
						: undefined
				}
			},
			page: {
				enabled: page.enabled,
				route: page.route,
				renderer: page.renderer,
				layout: page.layout,
				emptyStateTitle: page.emptyStateTitle,
				emptyStateDescription: page.emptyStateDescription,
				primaryAction: page.primaryAction
					? {
							label: page.primaryAction.label,
							action: page.primaryAction.action,
							template: page.primaryAction.template
						}
					: undefined,
				searchPlaceholder: page.searchPlaceholder,
				filterLabel: page.filterLabel,
				sortLabel: page.sortLabel,
				itemSingular: page.itemSingular,
				itemPlural: page.itemPlural
			}
		},
		aiIndexing: config.ai_indexing,
		audit: config.audit
	};
}

export const modulesStore = writable<ApplicationDefinition[]>(PREDEFINED_MODULES);

export async function refreshModules() {
	try {
		const enabled = await listEnabledApplications();
		modulesStore.update((current) => {
			const updated = current.map((m) => {
				const serverModule = enabled.find((sm) => sm.application_id === m.key);
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
				// Backend no longer returns this module → disable it
				return { ...m, enabled: false };
			});

			const existingKeys = new Set(current.map((m) => m.key));
			const added = enabled
				.filter((sm) => !existingKeys.has(sm.application_id))
				.map(applicationConfigToDefinition);

			return [...updated, ...added];
		});
	} catch (err) {
		console.error('Failed to refresh modules:', err);
	}
}

export function getAllModules(): ApplicationDefinition[] {
	return get(modulesStore);
}

export function getEnabledModules(): ApplicationDefinition[] {
	return getAllModules().filter((m) => m.enabled);
}

export function getSidebarModulesForUser(user: User | null): ApplicationDefinition[] {
	if (!user) return [];
	return getEnabledModules()
		.filter((m) => m.ui.sidebar.enabled && m.permissions.workspaceMembersCanUse)
		.sort((a, b) => a.ui.sidebar.order - b.ui.sidebar.order);
}

export function getDashboardModulesForUser(user: User | null): ApplicationDefinition[] {
	if (!user) return [];
	return getEnabledModules()
		.filter((m) => m.ui.dashboard.enabled && m.permissions.workspaceMembersCanUse)
		.sort((a, b) => a.ui.dashboard.order - b.ui.dashboard.order);
}

export function getApplicationByKey(key: string): ApplicationDefinition | undefined {
	return getAllModules().find((m) => m.key === key);
}

export function filterModulesByUserPreference(
	modules: ApplicationDefinition[],
	preferences: Record<string, boolean>
): ApplicationDefinition[] {
	return modules.filter((m) => preferences[m.key] !== false);
}
