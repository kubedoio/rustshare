<script lang="ts">
	import type { Component, ComponentType } from 'svelte';

	interface Props {
		icon: ComponentType | Component;
		title: string;
		description?: string;
		actionLabel?: string;
		onAction?: () => void;
	}

	let { icon: Icon, title, description, actionLabel, onAction }: Props = $props();
</script>

<div class="flex flex-col items-center justify-center py-16 text-center">
	<div class="mb-4 flex h-16 w-16 items-center justify-center rounded-2xl bg-base-200">
		<Icon size={48} class="text-base-content/30" />
	</div>
	<h3 class="text-title-md mb-1 font-semibold text-base-content">{title}</h3>
	{#if description}
		<p class="mb-4 text-body-sm text-base-content/60">{description}</p>
	{/if}
	{#if actionLabel && onAction}
		<button
			type="button"
			class="rounded-lg bg-brand-500 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-600"
			onclick={onAction}
		>
			{actionLabel}
		</button>
	{/if}
</div>
