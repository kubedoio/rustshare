<!--
  SlashCommandMenu — floating command palette triggered by "/" in the editor.
  Supports keyboard navigation (arrows, Enter, Escape) and mouse selection.
-->
<script lang="ts">
	import { createEventDispatcher, onMount, tick } from 'svelte';
	import {
		Type,
		Heading1,
		Heading2,
		Heading3,
		List,
		ListOrdered,
		ListChecks,
		Quote,
		Braces,
		Table,
		Minus,
		Image,
		Paperclip
	} from 'lucide-svelte';
	import type { SlashCommand } from '../adapter/slash-commands';
	import { filterSlashCommands } from '../adapter/slash-commands';

	let {
		query = '',
		top = 0,
		left = 0,
		hasAttachmentHandler = false
	}: {
		query?: string;
		top?: number;
		left?: number;
		hasAttachmentHandler?: boolean;
	} = $props();

	const dispatch = createEventDispatcher<{
		select: { command: SlashCommand };
		close: void;
	}>();

	const ICON_MAP: Record<string, typeof Type> = {
		type: Type,
		'heading-1': Heading1,
		'heading-2': Heading2,
		'heading-3': Heading3,
		list: List,
		'list-ordered': ListOrdered,
		'list-checks': ListChecks,
		quote: Quote,
		braces: Braces,
		table: Table,
		minus: Minus,
		image: Image,
		paperclip: Paperclip
	};

	let selectedIndex = $state(0);
	let menuEl: HTMLDivElement;

	let filteredCommands = $derived(filterSlashCommands(query, { hasAttachmentHandler }));
	$effect(() => {
		if (filteredCommands.length > 0 && selectedIndex >= filteredCommands.length) {
			selectedIndex = 0;
		}
	});

	onMount(async () => {
		await tick();
		menuEl?.focus();
	});

	function selectCommand(cmd: SlashCommand) {
		dispatch('select', { command: cmd });
	}

	function handleKeydown(event: KeyboardEvent) {
		switch (event.key) {
			case 'ArrowDown':
				event.preventDefault();
				event.stopPropagation();
				selectedIndex = (selectedIndex + 1) % filteredCommands.length;
				scrollToSelected();
				break;
			case 'ArrowUp':
				event.preventDefault();
				event.stopPropagation();
				selectedIndex = (selectedIndex - 1 + filteredCommands.length) % filteredCommands.length;
				scrollToSelected();
				break;
			case 'Enter':
				event.preventDefault();
				event.stopPropagation();
				if (filteredCommands[selectedIndex]) {
					selectCommand(filteredCommands[selectedIndex]);
				}
				break;
			case 'Escape':
				event.preventDefault();
				event.stopPropagation();
				dispatch('close');
				break;
		}
	}

	function scrollToSelected() {
		const item = menuEl?.querySelector(`[data-index="${selectedIndex}"]`);
		item?.scrollIntoView({ block: 'nearest' });
	}

	function getIcon(iconKey: string) {
		return ICON_MAP[iconKey] || Type;
	}
</script>

<div
	class="slash-menu"
	style="top: {top}px; left: {left}px;"
	bind:this={menuEl}
	on:keydown={handleKeydown}
	tabindex="0"
	role="listbox"
	aria-label="Insert block"
>
	{#if filteredCommands.length === 0}
		<div class="slash-menu-empty">No matching commands</div>
	{:else}
		{#each filteredCommands as cmd, i}
			<button
				class="slash-menu-item"
				class:selected={i === selectedIndex}
				data-index={i}
				role="option"
				aria-selected={i === selectedIndex}
				on:click={() => selectCommand(cmd)}
				on:mouseenter={() => (selectedIndex = i)}
			>
				<span class="slash-menu-icon">
					<svelte:component this={getIcon(cmd.icon)} size={16} />
				</span>
				<span class="slash-menu-text">
					<span class="slash-menu-label">{cmd.label}</span>
					<span class="slash-menu-desc">{cmd.description}</span>
				</span>
			</button>
		{/each}
	{/if}
</div>

<style>
	.slash-menu {
		position: fixed;
		z-index: 50;
		min-width: 240px;
		max-width: 320px;
		max-height: 320px;
		overflow-y: auto;
		background: var(--color-base-100, #fff);
		border: 1px solid var(--color-base-300, #e5e7eb);
		border-radius: 0.5rem;
		box-shadow:
			0 4px 16px rgba(0, 0, 0, 0.1),
			0 1px 4px rgba(0, 0, 0, 0.06);
		padding: 0.25rem;
		outline: none;
	}

	.slash-menu-empty {
		padding: 0.75rem 1rem;
		font-size: 0.8125rem;
		color: var(--color-base-content, #9ca3af);
		text-align: center;
	}

	.slash-menu-item {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		width: 100%;
		padding: 0.5rem 0.625rem;
		border: none;
		border-radius: 0.375rem;
		background: transparent;
		cursor: pointer;
		text-align: left;
		transition: background 0.1s;
	}

	.slash-menu-item:hover,
	.slash-menu-item.selected {
		background: var(--color-base-200, #f3f4f6);
	}

	.slash-menu-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 2rem;
		height: 2rem;
		border-radius: 0.375rem;
		background: var(--color-base-200, #f3f4f6);
		color: var(--color-base-content, #6b7280);
		flex-shrink: 0;
	}

	.slash-menu-item.selected .slash-menu-icon {
		background: var(--color-primary, #3b82f6);
		color: var(--color-primary-content, #fff);
	}

	.slash-menu-text {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.slash-menu-label {
		font-size: 0.8125rem;
		font-weight: 500;
		color: var(--color-base-content, #374151);
	}

	.slash-menu-desc {
		font-size: 0.6875rem;
		color: var(--color-base-content, #9ca3af);
		opacity: 0.7;
	}
</style>
