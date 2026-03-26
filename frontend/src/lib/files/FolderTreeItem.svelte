<script lang="ts">
	import { ChevronRight, ChevronDown, Folder, FolderOpen } from 'lucide-svelte';
	import type { FolderNode } from '$lib/stores/folderTree';
	import { folderTreeStore } from '$lib/stores/folderTree';
	import { slide } from 'svelte/transition';

	export let folder: FolderNode;
	export let level: number = 0;
	export let onSelect: (folder: FolderNode) => void;
	export let onToggleExpand: (folder: FolderNode) => void;

	$: isExpanded = $folderTreeStore.expandedIds.has(folder.id);
	$: isSelected = $folderTreeStore.selectedId === folder.id;
	$: isLoading = $folderTreeStore.loadingIds.has(folder.id);
	$: hasChildren = folder.children && folder.children.length > 0;
	$: canExpand = folder.has_children || hasChildren;

	const paddingLeft = level * 12 + 8;

	function handleClick() {
		onSelect(folder);
	}

	function handleToggle(e: MouseEvent) {
		e.stopPropagation();
		onToggleExpand(folder);
	}

	function handleKeyDown(e: KeyboardEvent) {
		if (e.key === 'Enter' || e.key === ' ') {
			e.preventDefault();
			onSelect(folder);
		}
	}
</script>

<div class="select-none">
	<div
		role="treeitem"
		tabindex="0"
		aria-selected={isSelected}
		aria-expanded={canExpand ? isExpanded : undefined}
		style="padding-left: {paddingLeft}px"
		class="group flex items-center gap-1.5 py-1.5 pr-3 cursor-pointer transition-colors rounded-md mx-1
			{isSelected ? 'bg-brand-500/15 text-brand-300' : 'text-base-content/70 hover:bg-base-200/50 hover:text-base-content'}"
		on:click={handleClick}
		on:keydown={handleKeyDown}
	>
		<!-- Expand/Collapse Button -->
		<button
			type="button"
			class="w-5 h-5 flex items-center justify-center rounded hover:bg-base-300/50 transition-colors
				{canExpand ? 'opacity-100' : 'opacity-0 pointer-events-none'}"
			on:click={handleToggle}
			aria-label={isExpanded ? 'Collapse folder' : 'Expand folder'}
		>
			{#if isLoading}
				<div class="w-3.5 h-3.5 border-2 border-current border-t-transparent rounded-full animate-spin"></div>
			{:else if isExpanded}
				<ChevronDown size={14} />
			{:else}
				<ChevronRight size={14} />
			{/if}
		</button>

		<!-- Folder Icon -->
		<div class="flex-shrink-0 {isSelected ? 'text-brand-400' : 'text-brand-500/70 group-hover:text-brand-400'}">
			{#if isExpanded}
				<FolderOpen size={18} />
			{:else}
				<Folder size={18} />
			{/if}
		</div>

		<!-- Folder Name -->
		<span class="text-sm truncate flex-1 font-medium">
			{folder.name}
		</span>
	</div>

	<!-- Children -->
	{#if isExpanded && folder.children}
		<div transition:slide={{ duration: 150 }} role="group">
			{#each folder.children as child (child.id)}
				<svelte:self
					folder={child}
					level={level + 1}
					{onSelect}
					{onToggleExpand}
				/>
			{/each}
		</div>
	{/if}
</div>
