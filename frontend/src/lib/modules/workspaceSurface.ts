import type {
	DashboardConfig,
	ModuleConfig,
	ModulePageDefinition,
	ModuleUiConfig,
	PrimaryActionConfig,
	SidebarConfig,
	WorkspaceSurfaceDefinition,
	WorkspaceWidgetColumns,
	WorkspaceWidgetConfig,
	WorkspaceWidgetSize
} from '$lib/api/types';

const DEFAULT_WIDGET_COLUMNS_BY_SIZE: Record<WorkspaceWidgetSize, WorkspaceWidgetColumns> = {
	small: { desktop: 3, tablet: 6, mobile: 12 },
	medium: { desktop: 4, tablet: 6, mobile: 12 },
	large: { desktop: 6, tablet: 12, mobile: 12 }
};

const DEFAULT_SURFACE: WorkspaceSurfaceDefinition = {
	id: 'workspace_dashboard_default',
	key: 'default-workspace-dashboard',
	name: 'Default Workspace Dashboard',
	version: '1.0',
	enabled: true,
	layout: {
		type: 'responsive-grid',
		columns: 12,
		gap: 24,
		compactOverview: true
	},
	sections: [
		{
			key: 'workspace-overview',
			type: 'workspace-summary',
			enabled: true,
			order: 10,
			renderer: 'compact-workspace-overview'
		},
		{
			key: 'summary-insights',
			type: 'dashboard-widgets',
			enabled: true,
			order: 20,
			title: 'Workspace Summary & Insights',
			renderer: 'workspace-widget-grid'
		}
	]
};

function getDefaultWidgetType(module: ModuleConfig): string {
	switch (module.module_key) {
		case 'kanban':
			return 'kanban-summary';
		case 'meetings':
			return 'decisions-meetings-summary';
		case 'notes':
			return 'latest-notes';
		case 'shares':
			return 'active-shares';
		default:
			return 'generic-module-summary';
	}
}

function getDefaultWidgetSize(module: ModuleConfig): WorkspaceWidgetSize {
	switch (module.module_key) {
		case 'kanban':
			return 'large';
		case 'meetings':
		case 'decisions':
			return 'medium';
		default:
			return 'small';
	}
}

function fallbackPrimaryAction(
	module: ModuleConfig,
	dashboard?: DashboardConfig
): PrimaryActionConfig | undefined {
	return dashboard?.primaryAction ?? undefined;
}

export function normalizeModuleUiConfig(module: ModuleConfig): ModuleUiConfig {
	const ui = module.ui_config ?? {};
	const sidebar = ui.sidebar ?? {
		enabled: false,
		order: 99,
		icon: module.icon,
		label: module.display_name
	};

	const legacyDashboard = ui.dashboard;
	const widgetSize = getDefaultWidgetSize(module);
	const widgetColumns =
		legacyDashboard?.widget?.columns ??
		DEFAULT_WIDGET_COLUMNS_BY_SIZE[legacyDashboard?.widget?.size ?? widgetSize];
	const widget: WorkspaceWidgetConfig = {
		enabled: legacyDashboard?.widget?.enabled ?? legacyDashboard?.enabled ?? true,
		type:
			legacyDashboard?.widget?.type ?? legacyDashboard?.summaryMode ?? getDefaultWidgetType(module),
		title: legacyDashboard?.widget?.title ?? legacyDashboard?.cardTitle ?? module.display_name,
		description:
			legacyDashboard?.widget?.description ??
			legacyDashboard?.cardDescription ??
			module.description,
		size: legacyDashboard?.widget?.size ?? widgetSize,
		columns: {
			desktop: widgetColumns.desktop,
			tablet: widgetColumns.tablet,
			mobile: widgetColumns.mobile
		},
		maxItems: legacyDashboard?.widget?.maxItems ?? legacyDashboard?.maxItems ?? 4,
		primaryAction:
			legacyDashboard?.widget?.primaryAction ?? fallbackPrimaryAction(module, legacyDashboard)
	};

	const normalizedDashboard: DashboardConfig = {
		enabled: legacyDashboard?.enabled ?? true,
		order: legacyDashboard?.order ?? 99,
		cardTitle: legacyDashboard?.cardTitle ?? widget.title,
		cardDescription: legacyDashboard?.cardDescription ?? widget.description,
		summaryMode: legacyDashboard?.summaryMode ?? widget.type,
		maxItems: legacyDashboard?.maxItems ?? widget.maxItems,
		primaryAction: fallbackPrimaryAction(module, legacyDashboard) ?? widget.primaryAction,
		widget
	};

	const legacyPage = ui.page ?? ui.modulePage;
	const page: ModulePageDefinition = {
		enabled: ui.page?.enabled ?? true,
		route: ui.page?.route ?? `/modules/${module.module_key}`,
		renderer: ui.page?.renderer ?? module.renderer,
		layout: legacyPage?.layout ?? 'list-grid',
		emptyStateTitle: legacyPage?.emptyStateTitle ?? `No ${module.display_name.toLowerCase()} yet`,
		emptyStateDescription: legacyPage?.emptyStateDescription ?? module.description,
		emptyStateAction:
			legacyPage?.emptyStateAction ?? normalizedDashboard.primaryAction?.label ?? 'Create',
		primaryAction: ui.page?.primaryAction ?? normalizedDashboard.primaryAction
	};

	return {
		sidebar,
		dashboard: normalizedDashboard,
		modulePage: {
			layout: page.layout,
			emptyStateTitle: page.emptyStateTitle,
			emptyStateDescription: page.emptyStateDescription,
			emptyStateAction: page.emptyStateAction
		},
		page
	};
}

export function normalizeModuleConfig(module: ModuleConfig): ModuleConfig {
	return {
		...module,
		ui_config: normalizeModuleUiConfig(module)
	};
}

export function getModuleSidebarConfig(module: ModuleConfig): SidebarConfig {
	return normalizeModuleUiConfig(module).sidebar!;
}

export function getModuleDashboardConfig(module: ModuleConfig): DashboardConfig {
	return normalizeModuleUiConfig(module).dashboard!;
}

export function getModuleDashboardWidgetConfig(module: ModuleConfig): WorkspaceWidgetConfig {
	return getModuleDashboardConfig(module).widget!;
}

export function getModulePageConfig(module: ModuleConfig): ModulePageDefinition {
	return normalizeModuleUiConfig(module).page!;
}

export function getEnabledDashboardModules(modules: ModuleConfig[]): ModuleConfig[] {
	return modules
		.map(normalizeModuleConfig)
		.filter((module) => {
			const dashboard = getModuleDashboardConfig(module);
			return dashboard.enabled !== false && dashboard.widget?.enabled !== false;
		})
		.sort((a, b) => getModuleDashboardConfig(a).order - getModuleDashboardConfig(b).order);
}

export function getEnabledSidebarModules(modules: ModuleConfig[]): ModuleConfig[] {
	return modules
		.map(normalizeModuleConfig)
		.filter((module) => getModuleSidebarConfig(module).enabled === true)
		.sort((a, b) => getModuleSidebarConfig(a).order - getModuleSidebarConfig(b).order);
}

export function normalizeWorkspaceSurfaceDefinition(
	surface: WorkspaceSurfaceDefinition | null | undefined
): WorkspaceSurfaceDefinition {
	if (!surface) {
		return DEFAULT_SURFACE;
	}

	return {
		...DEFAULT_SURFACE,
		...surface,
		layout: {
			...DEFAULT_SURFACE.layout,
			...(surface.layout ?? {})
		},
		sections: [...(surface.sections ?? DEFAULT_SURFACE.sections)].sort((a, b) => a.order - b.order)
	};
}
