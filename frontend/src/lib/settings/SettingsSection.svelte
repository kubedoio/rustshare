<script lang="ts">
	import type { Snippet } from 'svelte';
	let {
		children,
		title,
		description = undefined
	}: {
		children?: Snippet;
		title: string;
		description?: string | undefined;
	} = $props();
</script>

<section class="py-6 first:pt-0 last:pb-0">
	<div class="mb-4">
		<h3 class="text-base font-semibold text-base-content">{title}</h3>
		{#if description}
			<p class="mt-1 text-sm text-base-content/60">{description}</p>
		{/if}
	</div>
	<div class="space-y-0 divide-y divide-base-300">
		{@render children?.()}
	</div>
</section>
