<script lang="ts">
	import type { ApplicationDefinition } from '$lib/applications/registry';
	import NotesApplicationView from '$lib/components/apps/NotesApplicationView.svelte';
	import MeetingsApplicationView from '$lib/components/apps/MeetingsApplicationView.svelte';
	import StandupsApplicationView from '$lib/components/apps/StandupsApplicationView.svelte';
	import KanbanApplicationView from '$lib/components/apps/KanbanApplicationView.svelte';
	import DecisionsApplicationView from '$lib/components/apps/DecisionsApplicationView.svelte';
	import SharesApplicationView from '$lib/components/apps/SharesApplicationView.svelte';
	import BrainstormingApplicationView from '$lib/components/apps/BrainstormingApplicationView.svelte';
	import MailApplicationView from '$lib/components/apps/MailApplicationView.svelte';
	import ChatApplicationView from '$lib/components/chat/ChatApplicationView.svelte';
	import GenericApplicationView from '$lib/components/apps/GenericApplicationView.svelte';

	let { module }: { module: ApplicationDefinition } = $props();

	const rendererMap: Record<string, any> = {
		notes: NotesApplicationView,
		'okf-note': NotesApplicationView,
		meetings: MeetingsApplicationView,
		standups: StandupsApplicationView,
		kanban: KanbanApplicationView,
		decisions: DecisionsApplicationView,
		shares: SharesApplicationView,
		brainstorming: BrainstormingApplicationView,
		'mail-list': MailApplicationView,
		chat: ChatApplicationView
	};

	let Renderer = $derived(rendererMap[module.ui.page.renderer] || GenericApplicationView);
</script>

<svelte:component this={Renderer} {module} />
