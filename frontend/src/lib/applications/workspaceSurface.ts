import type {
	DashboardConfig,
	ApplicationConfig,
	ApplicationPageDefinition,
	ApplicationUiConfig,
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

function fallbackPrimaryAction(
	_application: ApplicationConfig,
	dashboard?: DashboardConfig
): PrimaryActionConfig | undefined {
	return dashboard?.primaryAction ?? undefined;
}

export function normalizeApplicationUiConfig(application: ApplicationConfig): ApplicationUiConfig {
	const ui = application.ui_config ?? {};
	const sidebar = ui.sidebar ?? {
		enabled: false,
		order: 99,
		icon: application.icon,
		label: application.display_name
	};

	const legacyDashboard = ui.dashboard;
	const widgetSize: WorkspaceWidgetSize = legacyDashboard?.widget?.size ?? 'medium';
	const widgetColumns =
		legacyDashboard?.widget?.columns ??
		DEFAULT_WIDGET_COLUMNS_BY_SIZE[legacyDashboard?.widget?.size ?? widgetSize];
	const widget: WorkspaceWidgetConfig = {
		enabled: legacyDashboard?.widget?.enabled ?? legacyDashboard?.enabled ?? true,
		type: legacyDashboard?.widget?.type ?? legacyDashboard?.summaryMode ?? 'application-summary',
		title: legacyDashboard?.widget?.title ?? legacyDashboard?.cardTitle ?? application.display_name,
		description:
			legacyDashboard?.widget?.description ??
			legacyDashboard?.cardDescription ??
			application.description,
		size: legacyDashboard?.widget?.size ?? widgetSize,
		columns: {
			desktop: widgetColumns.desktop,
			tablet: widgetColumns.tablet,
			mobile: widgetColumns.mobile
		},
		maxItems: legacyDashboard?.widget?.maxItems ?? legacyDashboard?.maxItems ?? 4,
		primaryAction:
			legacyDashboard?.widget?.primaryAction ?? fallbackPrimaryAction(application, legacyDashboard)
	};

	const normalizedDashboard: DashboardConfig = {
		enabled: legacyDashboard?.enabled ?? true,
		order: legacyDashboard?.order ?? 99,
		cardTitle: legacyDashboard?.cardTitle ?? widget.title,
		cardDescription: legacyDashboard?.cardDescription ?? widget.description,
		summaryMode: legacyDashboard?.summaryMode ?? widget.type,
		maxItems: legacyDashboard?.maxItems ?? widget.maxItems,
		primaryAction: fallbackPrimaryAction(application, legacyDashboard) ?? widget.primaryAction,
		widget
	};

	const pageConfig = ui.page;
	const page: ApplicationPageDefinition = {
		enabled: ui.page?.enabled ?? true,
		route: ui.page?.route ?? `/apps/${application.application_id.split('.').at(-1)}`,
		renderer: ui.page?.renderer ?? application.renderer,
		layout: pageConfig?.layout ?? 'list-grid',
		emptyStateTitle:
			pageConfig?.emptyStateTitle ?? `No ${application.display_name.toLowerCase()} yet`,
		emptyStateDescription: pageConfig?.emptyStateDescription ?? application.description,
		emptyStateAction:
			pageConfig?.emptyStateAction ?? normalizedDashboard.primaryAction?.label ?? 'Create',
		primaryAction: ui.page?.primaryAction ?? normalizedDashboard.primaryAction,
		searchPlaceholder:
			ui.page?.searchPlaceholder ?? `Search ${application.display_name.toLowerCase()}...`,
		filterLabel: ui.page?.filterLabel ?? `All ${application.display_name.toLowerCase()}`,
		sortLabel: ui.page?.sortLabel ?? 'Modified',
		itemSingular: ui.page?.itemSingular ?? application.display_name.toLowerCase(),
		itemPlural: ui.page?.itemPlural ?? application.display_name.toLowerCase()
	};

	return {
		documentFormat: ui.documentFormat,
		okf: ui.okf,
		sidebar,
		dashboard: normalizedDashboard,
		page
	};
}

export function normalizeApplicationConfig(application: ApplicationConfig): ApplicationConfig {
	return {
		...application,
		ui_config: normalizeApplicationUiConfig(application)
	};
}

export function getApplicationSidebarConfig(application: ApplicationConfig): SidebarConfig {
	return normalizeApplicationUiConfig(application).sidebar!;
}

export function getApplicationDashboardConfig(application: ApplicationConfig): DashboardConfig {
	return normalizeApplicationUiConfig(application).dashboard!;
}

export function getApplicationDashboardWidgetConfig(
	application: ApplicationConfig
): WorkspaceWidgetConfig {
	return getApplicationDashboardConfig(application).widget!;
}

export function getApplicationPageConfig(
	application: ApplicationConfig
): ApplicationPageDefinition {
	return normalizeApplicationUiConfig(application).page!;
}

export function getEnabledDashboardApplications(
	applications: ApplicationConfig[]
): ApplicationConfig[] {
	return applications
		.map(normalizeApplicationConfig)
		.filter((application) => {
			if (application.enabled === false) return false;
			const dashboard = getApplicationDashboardConfig(application);
			return dashboard.enabled !== false && dashboard.widget?.enabled !== false;
		})
		.sort(
			(a, b) => getApplicationDashboardConfig(a).order - getApplicationDashboardConfig(b).order
		);
}

export function getEnabledSidebarApplications(
	applications: ApplicationConfig[]
): ApplicationConfig[] {
	return applications
		.map(normalizeApplicationConfig)
		.filter(
			(application) =>
				application.enabled !== false && getApplicationSidebarConfig(application).enabled === true
		)
		.sort((a, b) => getApplicationSidebarConfig(a).order - getApplicationSidebarConfig(b).order);
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
