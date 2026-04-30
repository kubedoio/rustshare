<script lang="ts">
	import type { FolderTree } from '$lib/api/folders';
	import { ChevronRight, Folder, FolderOpen } from 'lucide-svelte';
	import Self from './FolderTreeItem.svelte';

	interface Props {
		folder: FolderTree;
		level: number;
		selectedFolderId: string | null;
		currentFolderId: string | null;
		expandedFolders: Set<string>;
		disabledFolderIds: Set<string>;
		onSelect: (folderId: string | null) => void;
		onToggle: (folderId: string) => void;
	}

	let {
		folder,
		level,
		selectedFolderId,
		currentFolderId,
		expandedFolders,
		disabledFolderIds,
		onSelect,
		onToggle
	}: Props = $props();

	let isExpanded = $derived(expandedFolders.has(folder.folder.id));
	let isSelected = $derived(selectedFolderId === folder.folder.id);
	let isCurrent = $derived(currentFolderId === folder.folder.id);
	let isDisabled = $derived(disabledFolderIds.has(folder.folder.id));
	let hasChildren = $derived(folder.subfolders && folder.subfolders.length > 0);

	function handleToggle(e: Event) {
		e.stopPropagation();
		onToggle(folder.folder.id);
	}

	function handleSelect() {
		if (!isDisabled) {
			onSelect(folder.folder.id);
		}
	}

	function handleKeydown(e: KeyboardEvent) {
		if (!isDisabled && (e.key === 'Enter' || e.key === ' ')) {
			onSelect(folder.folder.id);
		}
	}
</script>

<div class="select-none">
	<div
		class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm transition-colors
      {isSelected ? 'bg-brand-500/10 text-brand-600' : 'hover:bg-base-200/50'}
      {isDisabled ? 'cursor-not-allowed opacity-50' : 'cursor-pointer'}"
		style="padding-left: {16 + level * 20}px"
		role="button"
		tabindex={isDisabled ? -1 : 0}
		onclick={handleSelect}
		onkeydown={handleKeydown}
		aria-disabled={isDisabled}
	>
		<!-- Expand/collapse toggle -->
		{#if hasChildren}
			<button
				type="button"
				class="rounded p-0.5 hover:bg-base-300/50"
				onclick={handleToggle}
				tabindex="-1"
				aria-label={isExpanded ? 'Collapse folder' : 'Expand folder'}
			>
				<ChevronRight size={14} class="transition-transform {isExpanded ? 'rotate-90' : ''}" />
			</button>
		{:else}
			<span class="w-5"></span>
		{/if}

		<!-- Folder icon -->
		{#if isExpanded}
			<FolderOpen size={16} class="text-amber-500" />
		{:else}
			<Folder size={16} class="text-amber-500" />
		{/if}

		<!-- Folder name -->
		<span class="truncate">{folder.folder.name}</span>

		<!-- Current badge -->
		{#if isCurrent}
			<span
				class="ml-auto shrink-0 rounded-full bg-base-300/50 px-2 py-0.5 text-xs text-base-content/60"
				>Current</span
			>
		{/if}
	</div>

	<!-- Children -->
	{#if isExpanded && hasChildren}
		{#each folder.subfolders as child (child.folder.id)}
			<Self
				folder={child}
				level={level + 1}
				{selectedFolderId}
				{currentFolderId}
				{expandedFolders}
				{disabledFolderIds}
				{onSelect}
				{onToggle}
			/>
		{/each}
	{/if}
</div>
