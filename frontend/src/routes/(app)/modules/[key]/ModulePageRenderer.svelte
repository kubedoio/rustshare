<script lang="ts">
	import type { ModuleDefinition } from '$lib/modules/registry';
	import NotesModuleView from '$lib/components/modules/NotesModuleView.svelte';
	import MeetingsModuleView from '$lib/components/modules/MeetingsModuleView.svelte';
	import StandupsModuleView from '$lib/components/modules/StandupsModuleView.svelte';
	import KanbanModuleView from '$lib/components/modules/KanbanModuleView.svelte';
	import DecisionsModuleView from '$lib/components/modules/DecisionsModuleView.svelte';
	import SharesModuleView from '$lib/components/modules/SharesModuleView.svelte';
	import BrainstormingModuleView from '$lib/components/modules/BrainstormingModuleView.svelte';
	import GenericModuleView from '$lib/components/modules/GenericModuleView.svelte';

	export let module: ModuleDefinition;

	const rendererMap: Record<string, any> = {
		'notes': NotesModuleView,
		'meetings': MeetingsModuleView,
		'standups': StandupsModuleView,
		'kanban': KanbanModuleView,
		'decisions': DecisionsModuleView,
		'shares': SharesModuleView,
		'brainstorming': BrainstormingModuleView
	};

	$: Renderer = rendererMap[module.ui.page.renderer] || GenericModuleView;
</script>

<svelte:component this={Renderer} {module} />
