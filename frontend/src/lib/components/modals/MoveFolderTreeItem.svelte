<script lang="ts">
	import type { FolderTree } from '$lib/api/folders';
	import { Folder, ChevronRight } from 'lucide-svelte';
	import Self from './MoveFolderTreeItem.svelte';

	interface Props {
		folder: FolderTree;
		level?: number;
		selectedFolderId: string | null;
		currentFolderId: string | null;
		invalidFolderIds: Set<string>;
		expandedFolders: Set<string>;
		onSelect: (folderId: string | null) => void;
		onToggle: (folderId: string) => void;
	}

	let {
		folder,
		level = 0,
		selectedFolderId,
		currentFolderId,
		invalidFolderIds,
		expandedFolders,
		onSelect,
		onToggle
	}: Props = $props();

	let isSelected = $derived(selectedFolderId === folder.folder.id);
	let isCurrent = $derived(currentFolderId === folder.folder.id);
	let isInvalid = $derived(invalidFolderIds.has(folder.folder.id));
	let isExpanded = $derived(expandedFolders.has(folder.folder.id));
	let hasChildren = $derived(folder.subfolders && folder.subfolders.length > 0);

	function handleToggle(e: Event) {
		e.stopPropagation();
		onToggle(folder.folder.id);
	}

	function handleSelect() {
		if (!isInvalid) {
			onSelect(folder.folder.id);
		}
	}

	function handleKeydown(e: KeyboardEvent) {
		if (!isInvalid && (e.key === 'Enter' || e.key === ' ')) {
			onSelect(folder.folder.id);
		}
	}
</script>

<div class="folder-item">
	<div
		class="flex items-center gap-2 px-4 py-2 text-left transition-colors
			{isSelected
			? 'bg-brand-500/10 text-brand-600'
			: isInvalid
				? 'cursor-not-allowed opacity-40'
				: 'cursor-pointer hover:bg-base-200/50'}"
		style="padding-left: {level * 16 + 16}px"
		role="button"
		tabindex={isInvalid ? -1 : 0}
		onclick={handleSelect}
		onkeydown={handleKeydown}
		aria-disabled={isInvalid}
	>
		<!-- Expand/Collapse button (if has children) -->
		{#if hasChildren}
			<button
				type="button"
				class="flex h-5 w-5 shrink-0 items-center justify-center rounded transition-colors hover:bg-base-300/50"
				onclick={handleToggle}
				tabindex="-1"
				aria-label={isExpanded ? 'Collapse' : 'Expand'}
			>
				<ChevronRight size={14} class="transition-transform {isExpanded ? 'rotate-90' : ''}" />
			</button>
		{:else}
			<span class="w-5"></span>
		{/if}

		<Folder size={16} class="shrink-0 {isSelected ? 'text-brand-500' : 'text-base-content/50'}" />

		<span class="flex-1 truncate text-sm {isSelected ? 'font-medium' : ''}">
			{folder.folder.name}
		</span>

		{#if isCurrent}
			<span class="rounded-full bg-base-300/50 px-2 py-0.5 text-xs text-base-content/60"
				>Current</span
			>
		{:else if isInvalid}
			<span class="rounded-full bg-error/10 px-2 py-0.5 text-xs text-error">Invalid</span>
		{/if}
	</div>

	<!-- Children -->
	{#if isExpanded && hasChildren}
		<div class="children">
			{#each folder.subfolders as child (child.folder.id)}
				<Self
					folder={child}
					level={level + 1}
					{selectedFolderId}
					{currentFolderId}
					{invalidFolderIds}
					{expandedFolders}
					{onSelect}
					{onToggle}
				/>
			{/each}
		</div>
	{/if}
</div>
