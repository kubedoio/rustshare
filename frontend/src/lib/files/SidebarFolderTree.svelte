<script lang="ts">
	import { browser } from '$app/environment';
	import { goto } from '$app/navigation';
	import { ChevronRight, Folder, FolderOpen } from 'lucide-svelte';
	import { fileBrowserUi } from '$lib/stores/fileBrowserUi';
	import type { FolderTree } from '$lib/api/folders';

	// Props
	interface Props {
		folders: FolderTree[];
		level?: number;
		onFolderClick?: (folderId: string | null) => void;
		getExpandedIds?: () => Set<string>;
	}

	let { 
		folders,
		level = 0,
		onFolderClick = () => {},
		getExpandedIds = () => new Set()
	}: Props = $props();

	function isFolderExpanded(folderId: string): boolean {
		return $fileBrowserUi.expandedFolderIds.has(folderId);
	}

	function isFolderActive(folderId: string): boolean {
		if (!browser) return false;
		const params = new URLSearchParams(window.location.search);
		return params.get('folder') === folderId;
	}

	function toggleFolder(e: Event, folderId: string) {
		e.stopPropagation();
		fileBrowserUi.toggleFolderExpanded(folderId);
	}

	function navigateToFolder(folderId: string) {
		fileBrowserUi.selectFolder(folderId);
		goto(`/files?folder=${folderId}`);
		onFolderClick(folderId);
	}

	function hasChildren(folder: FolderTree): boolean {
		return folder.subfolders && folder.subfolders.length > 0;
	}
</script>

{#each folders as folder (folder.folder.id)}
	{@const isExpanded = isFolderExpanded(folder.folder.id)}
	{@const isActive = isFolderActive(folder.folder.id)}
	{@const hasChildrenValue = hasChildren(folder)}
	<div class="folder-tree-node">
		<!-- Folder Item -->
		<div
			class="group flex items-center gap-1 rounded-lg transition-colors cursor-pointer
				{isActive 
					? 'bg-brand-500/10 text-brand-600' 
					: 'text-base-content/80 hover:bg-base-200/50'}"
			style="padding-left: {level * 12 + 8}px"
		>
			<!-- Expand/Collapse button -->
			<button
				type="button"
				class="w-5 h-5 flex items-center justify-center rounded hover:bg-base-300/50 transition-colors shrink-0
					{hasChildrenValue ? '' : 'invisible'}"
				onclick={(e) => toggleFolder(e, folder.folder.id)}
				aria-label={isExpanded ? 'Collapse folder' : 'Expand folder'}
				tabindex="-1"
			>
				<ChevronRight 
					size={14} 
					class="transition-transform duration-200 {isExpanded ? 'rotate-90' : ''}" 
				/>
			</button>
			
			<!-- Folder link -->
			<button
				type="button"
				class="flex-1 flex items-center gap-2 py-1.5 text-left min-w-0 text-sm"
				onclick={() => navigateToFolder(folder.folder.id)}
			>
				{#if isActive}
					<FolderOpen size={16} class="text-brand-500 shrink-0" />
				{:else}
					<Folder size={16} class="text-base-content/50 shrink-0" />
				{/if}
				<span class="flex-1 truncate">{folder.folder.name}</span>
			</button>
		</div>
		
		<!-- Children - Recursive -->
		{#if isExpanded && hasChildrenValue}
			<div class="mt-0.5">
				<svelte:self 
					folders={folder.subfolders} 
					level={level + 1}
					{onFolderClick}
					{getExpandedIds}
				/>
			</div>
		{/if}
	</div>
{/each}
