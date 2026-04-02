<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import type { ComponentType } from 'svelte';

	export interface MenuItem {
		id: string;
		label: string;
		icon?: ComponentType;
		shortcut?: string;
		disabled?: boolean;
		danger?: boolean;
		separator?: boolean;
		onClick: () => void;
	}

	export let items: MenuItem[] = [];
	export let x: number = 0;
	export let y: number = 0;
	export let visible: boolean = false;
	export let onClose: () => void = () => {};

	let menuRef: HTMLDivElement;

	// Adjust position to keep menu in viewport
	$: adjustedX = x;
	$: adjustedY = y;
	$: if (menuRef && visible) {
		const rect = menuRef.getBoundingClientRect();
		const viewportWidth = window.innerWidth;
		const viewportHeight = window.innerHeight;
		
		if (x + rect.width > viewportWidth) {
			adjustedX = viewportWidth - rect.width - 8;
		}
		if (y + rect.height > viewportHeight) {
			adjustedY = viewportHeight - rect.height - 8;
		}
		if (adjustedX < 8) adjustedX = 8;
		if (adjustedY < 8) adjustedY = 8;
	}

	function handleClickOutside(e: MouseEvent) {
		if (menuRef && !menuRef.contains(e.target as Node)) {
			onClose();
		}
	}

	function handleKeyDown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			onClose();
		}
	}

	onMount(() => {
		document.addEventListener('click', handleClickOutside, true);
		document.addEventListener('keydown', handleKeyDown);
	});

	onDestroy(() => {
		document.removeEventListener('click', handleClickOutside, true);
		document.removeEventListener('keydown', handleKeyDown);
	});
</script>

{#if visible}
	<div
		bind:this={menuRef}
		class="fixed z-[9999] min-w-[180px] rounded-lg border border-base-300 bg-base-100 py-1 shadow-xl shadow-black/20"
		style="left: {adjustedX}px; top: {adjustedY}px;"
		role="menu"
	>
		{#each items as item}
			{#if item.separator}
				<div class="my-1 border-t border-base-200"></div>
			{:else}
				<button
					type="button"
					class="flex w-full items-center gap-3 px-3 py-2 text-left text-sm transition-colors
						{item.disabled ? 'cursor-not-allowed opacity-50' : 'hover:bg-base-200'}
						{item.danger ? 'text-error hover:bg-error/10' : 'text-base-content'}"
					disabled={item.disabled}
					on:click={() => {
						if (!item.disabled) {
							console.log("[ContextMenu] clicked item:", item.id, item.label); item.onClick();
							onClose();
						}
					}}
					role="menuitem"
				>
					{#if item.icon}
						<span class="flex-shrink-0 opacity-70">
							<svelte:component this={item.icon} size={16} />
						</span>
					{:else}
						<span class="w-4 flex-shrink-0"></span>
					{/if}
					<span class="flex-1">{item.label}</span>
					{#if item.shortcut}
						<span class="ml-4 text-xs opacity-40">{item.shortcut}</span>
					{/if}
				</button>
			{/if}
		{/each}
	</div>
{/if}
