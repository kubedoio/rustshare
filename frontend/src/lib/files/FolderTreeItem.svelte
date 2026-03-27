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
	$: hasNoChildren = folder.children !== undefined && (!folder.children || folder.children.length === 0);
	// Show expand button if: we have children, or we haven't checked yet (children undefined)
	$: canExpand = hasChildren || !hasNoChildren;

	const paddingLeft = level * 12 + 8;

	function handleClick() {
		onSelect(folder);
	}

	function handleToggle(e: MouseEvent) {
		e.preventDefault();
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
			{isSelected ? 'bg-[#1e3a5f]/60 text-[#e5e7eb]' : 'text-[#9ca3af] hover:bg-[#1a1d24] hover:text-[#e5e7eb]'}"
		on:click={handleClick}
		on:keydown={handleKeyDown}
	>
		<!-- Expand/Collapse Button -->
		<button
			type="button"
			class="w-6 h-6 flex items-center justify-center rounded hover:bg-[#2a2f35] transition-colors flex-shrink-0
				{canExpand ? 'opacity-100' : 'opacity-0 pointer-events-none'}"
			on:click|preventDefault|stopPropagation={handleToggle}
			aria-label={isExpanded ? 'Collapse folder' : 'Expand folder'}
			disabled={!canExpand}
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
		<div class="flex-shrink-0 {isSelected ? 'text-[#2563eb]' : 'text-[#4b5563] group-hover:text-[#6b7280]'}">
			{#if isExpanded}
				<FolderOpen size={18} />
			{:else}
				<Folder size={18} />
			{/if}
		</div>

		<!-- Folder Name -->
		<span class="text-sm truncate flex-1 font-normal">
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
