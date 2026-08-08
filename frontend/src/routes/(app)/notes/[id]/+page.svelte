<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';

	const noteId = $page.params.id as string;

	onMount(() => {
		goto(`/apps/notes/${noteId}`, { replaceState: true });
	});
</script>

<div class="flex h-full items-center justify-center">
	<div class="loading loading-lg loading-spinner text-brand-500"></div>
</div>
