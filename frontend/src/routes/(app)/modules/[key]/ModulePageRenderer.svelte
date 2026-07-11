<script lang="ts">
	import type { ModuleDefinition } from '$lib/modules/registry';
	import NotesModuleView from '$lib/components/modules/NotesModuleView.svelte';
	import MeetingsModuleView from '$lib/components/modules/MeetingsModuleView.svelte';
	import StandupsModuleView from '$lib/components/modules/StandupsModuleView.svelte';
	import KanbanModuleView from '$lib/components/modules/KanbanModuleView.svelte';
	import DecisionsModuleView from '$lib/components/modules/DecisionsModuleView.svelte';
	import SharesModuleView from '$lib/components/modules/SharesModuleView.svelte';
	import BrainstormingModuleView from '$lib/components/modules/BrainstormingModuleView.svelte';
	import MailModuleView from '$lib/components/modules/MailModuleView.svelte';
	import GenericModuleView from '$lib/components/modules/GenericModuleView.svelte';

	let { module }: { module: ModuleDefinition } = $props();

	const rendererMap: Record<string, any> = {
		notes: NotesModuleView,
		'okf-note': NotesModuleView,
		meetings: MeetingsModuleView,
		standups: StandupsModuleView,
		kanban: KanbanModuleView,
		decisions: DecisionsModuleView,
		shares: SharesModuleView,
		brainstorming: BrainstormingModuleView,
		'mail-list': MailModuleView
	};

	let Renderer = $derived(rendererMap[module.ui.page.renderer] || GenericModuleView);
</script>

<svelte:component this={Renderer} {module} />
