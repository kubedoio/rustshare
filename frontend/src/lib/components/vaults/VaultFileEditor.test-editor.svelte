<script lang="ts">
	import { createEventDispatcher } from 'svelte';

	let { content = '' }: { content?: string } = $props();
	let markdown = $state('');
	const dispatch = createEventDispatcher<{ change: { markdown: string } }>();

	$effect(() => {
		markdown = content;
	});

	export function getMarkdown(): string {
		return markdown;
	}
</script>

<textarea
	value={content}
	oninput={(event) => {
		markdown = event.currentTarget.value;
		dispatch('change', { markdown });
	}}></textarea>
