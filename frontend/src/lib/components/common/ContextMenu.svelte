<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import type { ComponentType } from 'svelte';

	interface MenuItem {
		id: string;
		label: string;
		icon?: ComponentType;
		shortcut?: string;
		disabled?: boolean;
		danger?: boolean;
		separator?: boolean;
		onClick: () => void;
	}

	let {
		items = [],
		x = 0,
		y = 0,
		visible = false,
		onClose = () => {}
	}: {
		items?: MenuItem[];
		x?: number;
		y?: number;
		visible?: boolean;
		onClose?: () => void;
	} = $props();

	let menuRef = $state<HTMLDivElement>();

	let adjustedX = $state(x);
	let adjustedY = $state(y);

	$effect(() => {
		if (menuRef && visible) {
			const rect = menuRef.getBoundingClientRect();
			const viewportWidth = window.innerWidth;
			const viewportHeight = window.innerHeight;

			let newX = x;
			let newY = y;
			if (x + rect.width > viewportWidth) {
				newX = viewportWidth - rect.width - 8;
			}
			if (y + rect.height > viewportHeight) {
				newY = viewportHeight - rect.height - 8;
			}
			if (newX < 8) newX = 8;
			if (newY < 8) newY = 8;
			adjustedX = newX;
			adjustedY = newY;
		} else {
			adjustedX = x;
			adjustedY = y;
		}
	});

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
							item.onClick();
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
