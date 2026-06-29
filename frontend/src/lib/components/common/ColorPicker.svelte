<script lang="ts">
	import { X } from 'lucide-svelte';
	import { COLOR_PALETTE } from '$lib/utils/colorPalette';

	interface Props {
		value?: string | null;
		onSelect: (color: string | null) => void;
		onClose?: () => void;
		title?: string;
	}

	let { value = null, onSelect, onClose, title = 'Set color' }: Props = $props();
</script>

<div
	class="flex flex-col gap-2 p-2"
	role="dialog"
	tabindex="-1"
	aria-label={title}
	onclick={(e) => e.stopPropagation()}
	onkeydown={(e) => {
		e.stopPropagation();
		if (e.key === 'Escape' && onClose) {
			onClose();
		}
	}}
>
	<div class="flex items-center justify-between px-1">
		<span class="text-xs font-medium text-base-content/70">{title}</span>
		{#if onClose}
			<button
				type="button"
				class="rounded-md p-1 text-base-content/50 hover:bg-base-200"
				onclick={(e) => {
					e.stopPropagation();
					onClose();
				}}
			>
				<X size={12} />
			</button>
		{/if}
	</div>
	<div class="grid grid-cols-4 gap-1.5">
		{#each COLOR_PALETTE as color}
			<button
				type="button"
				class="h-6 w-6 rounded-full {color.bgClass} ring-offset-2 hover:ring-2 hover:ring-base-content/30 focus:outline-hidden focus:ring-2 focus:ring-base-content/30"
				class:ring-2={value === color.key}
				class:ring-base-content={value === color.key}
				aria-label={color.label}
				onclick={(e) => {
					e.stopPropagation();
					onSelect(color.key);
				}}
			></button>
		{/each}
		<button
			type="button"
			class="flex h-6 w-6 items-center justify-center rounded-full border border-base-300 text-base-content/50 hover:bg-base-200"
			class:bg-base-200={value === null}
			aria-label="Clear color"
			onclick={(e) => {
				e.stopPropagation();
				onSelect(null);
			}}
		>
			<X size={12} />
		</button>
	</div>
</div>
