import { get, writable } from 'svelte/store';
import type {
	ApplicationConfig,
	ApplicationContribution,
	ApplicationManifest,
	ApplicationShellEntry,
	User
} from '$lib/api/types';
import { listEnabledApplications } from '$lib/api/applications';
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

export interface ApplicationDefinition {
	id: string;
	/** Route slug declared by the manifest Contribution, not the Application ID. */
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
	settings?: ApplicationContribution[];
	okf?: {
		enabled: boolean;
		conceptType: string;
		frontmatterRequired: boolean;
		preserveUnknownFields?: boolean;
	};
	aiIndexing: { enabled: boolean };
	audit: { enabled: boolean };
}

const DEFAULT_COLUMNS: ApplicationUiDashboardWidgetColumns = {
	desktop: 6,
	tablet: 12,
	mobile: 12
};

function routeSlug(manifest: ApplicationManifest): string {
	const route = manifest.contributions.navigation.find((item) => item.route)?.route ?? '';
	return route.split('/').filter(Boolean)[1] ?? manifest.metadata.id.split('.').at(-1)!;
}

function shellContributionConfig(
	manifest: ApplicationManifest
): NonNullable<ApplicationConfig['ui_config']> {
	const navigation = manifest.contributions.navigation[0];
	const route =
		manifest.contributions.routes.find((item) => item.route) ?? manifest.contributions.routes[0];
	const dashboard = manifest.contributions.dashboard[0];
	const slug = routeSlug(manifest);
	const label = navigation?.label ?? manifest.metadata.name;
	const renderer = route?.renderer ?? slug;
	const primaryAction = dashboard?.action
		? {
				label: dashboard.label ?? `Create ${label.toLowerCase()}`,
				action: dashboard.action.endsWith('.create') ? 'create-from-template' : dashboard.action,
				template: dashboard.template
			}
		: undefined;

	return {
		sidebar: {
			enabled: Boolean(navigation),
			order: navigation?.order ?? 99,
			icon: navigation?.icon ?? 'layout-dashboard',
			label
		},
		dashboard: {
			enabled: Boolean(dashboard),
			order: dashboard?.order ?? 99,
			cardTitle: label,
			cardDescription: manifest.metadata.description,
			summaryMode: dashboard?.renderer ?? `${slug}-dashboard`,
			primaryAction,
			widget: {
				enabled: Boolean(dashboard),
				type: dashboard?.renderer ?? `${slug}-dashboard`,
				title: label,
				description: manifest.metadata.description,
				size: 'medium',
				columns: DEFAULT_COLUMNS,
				maxItems: 4,
				primaryAction
			}
		},
		page: {
			enabled: Boolean(route),
			route: route?.route ?? `/apps/${slug}`,
			renderer,
			layout: 'list-grid',
			emptyStateTitle: `No ${label.toLowerCase()} yet`,
			emptyStateDescription: manifest.metadata.description,
			emptyStateAction: `Create ${label.toLowerCase()}`,
			searchPlaceholder: `Search ${label.toLowerCase()}...`,
			filterLabel: `All ${label.toLowerCase()}`,
			sortLabel: 'Modified',
			itemSingular: label.toLowerCase(),
			itemPlural: `${label.toLowerCase()}s`
		},
		settings: manifest.contributions.settings
	};
}

export function applicationShellEntryToConfig(entry: ApplicationShellEntry): ApplicationConfig {
	const { manifest } = entry;
	const slug = routeSlug(manifest);
	const configuration = (entry.configuration ?? {}) as Record<string, any>;
	const permissions = configuration.permissions ?? {};
	const uiConfig = shellContributionConfig(manifest);

	return {
		id: manifest.metadata.id,
		application_id: manifest.metadata.id,
		display_name: manifest.metadata.name,
		description: manifest.metadata.description,
		enabled: entry.enabled,
		root_path: configuration.rootPath ?? `/Workspace/${slug[0].toUpperCase()}${slug.slice(1)}`,
		renderer: configuration.renderer ?? uiConfig.page?.renderer ?? slug,
		default_template: configuration.defaultTemplate ?? null,
		icon: configuration.icon ?? uiConfig.sidebar?.icon ?? 'layout-dashboard',
		schema_version: manifest.apiVersion,
		permissions: {
			admin_can_configure: permissions.admin_can_configure ?? true,
			workspace_members_can_use: permissions.workspace_members_can_use ?? true,
			allow_public_share: permissions.allow_public_share ?? false,
			allow_internal_share: permissions.allow_internal_share ?? true
		},
		ai_indexing: configuration.aiIndexing ?? { enabled: true },
		audit: configuration.audit ?? { enabled: true },
		ui_config: uiConfig,
		created_at: '',
		updated_at: ''
	};
}

export function applicationConfigToDefinition(config: ApplicationConfig): ApplicationDefinition {
	const ui = normalizeApplicationUiConfig(config);
	return {
		id: config.application_id,
		key: ui.page?.route.split('/').filter(Boolean)[1] ?? config.application_id,
		displayName: config.display_name,
		description: config.description,
		enabled: config.enabled,
		rootPath: config.root_path,
		renderer: config.renderer,
		documentFormat: config.ui_config?.documentFormat,
		defaultTemplate: config.default_template,
		icon: config.icon,
		schemaVersion: config.schema_version,
		permissions: {
			adminCanConfigure: config.permissions.admin_can_configure,
			workspaceMembersCanUse: config.permissions.workspace_members_can_use,
			allowPublicShare: config.permissions.allow_public_share,
			allowInternalShare: config.permissions.allow_internal_share
		},
		okf: config.ui_config?.okf,
		ui: {
			sidebar: ui.sidebar!,
			dashboard: {
				enabled: ui.dashboard!.enabled,
				order: ui.dashboard!.order,
				widget: ui.dashboard!.widget!
			},
			page: ui.page!
		},
		settings: ui.settings ?? [],
		aiIndexing: config.ai_indexing,
		audit: config.audit
	};
}

export const applicationsStore = writable<ApplicationDefinition[]>([]);

export async function refreshApplications(): Promise<void> {
	const entries = await listEnabledApplications();
	applicationsStore.set(
		entries.map(applicationShellEntryToConfig).map(applicationConfigToDefinition)
	);
}

export function getAllApplications(): ApplicationDefinition[] {
	return get(applicationsStore);
}

export function getEnabledApplications(): ApplicationDefinition[] {
	return getAllApplications().filter((application) => application.enabled);
}

export function getSidebarApplicationsForUser(user: User | null): ApplicationDefinition[] {
	if (!user) return [];
	return getEnabledApplications()
		.filter(
			(application) =>
				application.ui.sidebar.enabled && application.permissions.workspaceMembersCanUse
		)
		.sort((a, b) => a.ui.sidebar.order - b.ui.sidebar.order);
}

export function getDashboardApplicationsForUser(user: User | null): ApplicationDefinition[] {
	if (!user) return [];
	return getEnabledApplications()
		.filter((application) => application.ui.dashboard.enabled)
		.sort((a, b) => a.ui.dashboard.order - b.ui.dashboard.order);
}

export function getApplicationByRouteSlug(slug: string): ApplicationDefinition | undefined {
	return getAllApplications().find((application) => application.key === slug);
}
