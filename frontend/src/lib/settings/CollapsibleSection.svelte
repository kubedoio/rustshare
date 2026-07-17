<script lang="ts">
	import { ChevronDown } from 'lucide-svelte';
	import type { Snippet } from 'svelte';

	let {
		title,
		description = undefined,
		defaultOpen = false,
		children
	}: {
		title: string;
		description?: string | undefined;
		defaultOpen?: boolean;
		children: Snippet;
	} = $props();

	let open = $state(defaultOpen);
</script>

<section class="border-b border-[var(--rs-border)] last:border-b-0">
	<button
		type="button"
		class="flex w-full items-center justify-between gap-3 px-1 py-3 text-left"
		aria-expanded={open}
		onclick={() => (open = !open)}
	>
		<span class="min-w-0">
			<span class="block text-sm font-semibold text-base-content">{title}</span>
			{#if description}
				<span class="mt-0.5 block text-xs text-base-content/55">{description}</span>
			{/if}
		</span>
		<ChevronDown
			size={15}
			class="shrink-0 text-base-content/45 transition-transform {open ? 'rotate-180' : ''}"
		/>
	</button>
	{#if open}
		<div class="px-1 pb-4">
			{@render children()}
		</div>
	{/if}
</section>
