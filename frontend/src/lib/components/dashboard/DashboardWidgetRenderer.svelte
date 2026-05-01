<script lang="ts">
	import type { ModuleDefinition } from '$lib/modules/registry';
	import KanbanSummaryWidget from './widgets/KanbanSummaryWidget.svelte';
	import DecisionsMeetingsSummaryWidget from './widgets/DecisionsMeetingsSummaryWidget.svelte';
	import LatestNotesWidget from './widgets/LatestNotesWidget.svelte';
	import ActiveSharesWidget from './widgets/ActiveSharesWidget.svelte';
	import StandupsSummaryWidget from './widgets/StandupsSummaryWidget.svelte';
	import GenericModuleSummaryWidget from './widgets/GenericModuleSummaryWidget.svelte';

	export let module: ModuleDefinition;
	export let modules: ModuleDefinition[] = [];

	const widgetRegistry: Record<string, any> = {
		'kanban-summary': KanbanSummaryWidget,
		'decisions-recent': DecisionsMeetingsSummaryWidget,
		'meetings-recent': DecisionsMeetingsSummaryWidget,
		'notes-recent': LatestNotesWidget,
		'standups-recent': StandupsSummaryWidget,
		'shares-summary': ActiveSharesWidget
	};

	$: widget = module.ui.dashboard.widget;
	$: Renderer = widgetRegistry[widget.type] ?? GenericModuleSummaryWidget;
</script>

<svelte:component this={Renderer} {module} {modules} />
