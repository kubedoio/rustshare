<script lang="ts">
	import type { ModuleConfig } from '$lib/api/types';
	import { getModuleDashboardWidgetConfig } from '$lib/modules/workspaceSurface';
	import KanbanSummaryWidget from './widgets/KanbanSummaryWidget.svelte';
	import DecisionsMeetingsSummaryWidget from './widgets/DecisionsMeetingsSummaryWidget.svelte';
	import LatestNotesWidget from './widgets/LatestNotesWidget.svelte';
	import ActiveSharesWidget from './widgets/ActiveSharesWidget.svelte';
	import GenericModuleSummaryWidget from './widgets/GenericModuleSummaryWidget.svelte';

	export let module: ModuleConfig;
	export let modules: ModuleConfig[] = [];

	const widgetRegistry: Record<string, any> = {
		'kanban-summary': KanbanSummaryWidget,
		'decisions-meetings-summary': DecisionsMeetingsSummaryWidget,
		'latest-notes': LatestNotesWidget,
		'active-shares': ActiveSharesWidget,
		'generic-module-summary': GenericModuleSummaryWidget
	};

	$: widget = getModuleDashboardWidgetConfig(module);
	$: Renderer = widgetRegistry[widget.type] ?? GenericModuleSummaryWidget;
</script>

<svelte:component this={Renderer} {module} {modules} />
