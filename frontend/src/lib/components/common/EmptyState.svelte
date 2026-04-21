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
	<div class="w-16 h-16 rounded-2xl bg-base-200 flex items-center justify-center mb-4">
		<Icon size={48} class="text-base-content/30" />
	</div>
	<h3 class="text-title-md font-semibold text-base-content mb-1">{title}</h3>
	{#if description}
		<p class="text-body-sm text-base-content/60 mb-4">{description}</p>
	{/if}
	{#if actionLabel && onAction}
		<button
			type="button"
			class="px-4 py-2 text-sm font-medium bg-brand-500 hover:bg-brand-600 text-white rounded-lg transition-colors"
			onclick={onAction}
		>
			{actionLabel}
		</button>
	{/if}
</div>
