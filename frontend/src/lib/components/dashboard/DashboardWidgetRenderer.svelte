<script lang="ts">
	import type { ModuleDefinition } from '$lib/modules/registry';
	import KanbanSummaryWidget from './widgets/KanbanSummaryWidget.svelte';
	import DecisionsMeetingsSummaryWidget from './widgets/DecisionsMeetingsSummaryWidget.svelte';
	import LatestNotesWidget from './widgets/LatestNotesWidget.svelte';
	import ActiveSharesWidget from './widgets/ActiveSharesWidget.svelte';
	import StandupsSummaryWidget from './widgets/StandupsSummaryWidget.svelte';
	import RecentBrainstormBoardsWidget from './widgets/RecentBrainstormBoardsWidget.svelte';
	import GenericModuleSummaryWidget from './widgets/GenericModuleSummaryWidget.svelte';

	let {
		module,
		modules = []
	}: {
		module: ModuleDefinition;
		modules?: ModuleDefinition[];
	} = $props();

	const widgetRegistry: Record<string, any> = {
		'kanban-summary': KanbanSummaryWidget,
		'decisions-recent': DecisionsMeetingsSummaryWidget,
		'meetings-recent': DecisionsMeetingsSummaryWidget,
		'notes-recent': LatestNotesWidget,
		'standups-recent': StandupsSummaryWidget,
		'shares-summary': ActiveSharesWidget,
		'recent-brainstorm-boards': RecentBrainstormBoardsWidget
	};

	let widget = $derived(module.ui.dashboard.widget);
	let Renderer = $derived(widgetRegistry[widget.type] ?? GenericModuleSummaryWidget);
</script>

<svelte:component this={Renderer} {module} {modules} />
