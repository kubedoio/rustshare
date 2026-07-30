<script lang="ts">
	import { createEventDispatcher } from 'svelte';

	let { content = '' }: { content?: string } = $props();
	const dispatch = createEventDispatcher<{ change: { markdown: string } }>();
</script>

<textarea
	value={content}
	oninput={(event) => dispatch('change', { markdown: event.currentTarget.value })}></textarea>
