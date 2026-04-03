<script lang="ts">
	import { browser } from '$app/environment';
	import { goto } from '$app/navigation';
	import { ChevronRight, Folder, FolderOpen } from 'lucide-svelte';
	import { fileBrowserUi } from '$lib/stores/fileBrowserUi';
	import { page } from '$app/stores';
	import SidebarFolderTree from './SidebarFolderTree.svelte';
	import type { FolderTree } from '$lib/api/folders';

	// Props
	interface Props {
		folders: FolderTree[];
		level?: number;
		onFolderClick?: (folderId: string | null) => void;
		getExpandedIds?: () => Set<string>;
		// For tree connectors: array indicating if each ancestor is the last child
		ancestorIsLast?: boolean[];
	}

	let { 
		folders,
		level = 0,
		onFolderClick = () => {},
		getExpandedIds = () => new Set(),
		ancestorIsLast = []
	}: Props = $props();

	function isFolderExpanded(folderId: string): boolean {
		return $fileBrowserUi.expandedFolderIds.has(folderId);
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

<div class="folder-tree-container">
	{#each folders as folder, i (folder.folder.id)}
		{@const folderId = folder.folder.id}
		{@const isExpanded = isFolderExpanded(folderId)}
		{@const isActive = $page.url.searchParams.get('folder') === folderId}
		{@const hasChildrenValue = hasChildren(folder)}
		{@const isLast = i === folders.length - 1}
		<!-- Pass isLast to children so they know to not draw vertical line through this ancestor -->
		{@const childAncestorIsLast = [...ancestorIsLast, isLast]}

		<div class="folder-tree-node">
			<!-- Folder Item -->
			<div
				class="group flex items-center gap-1 transition-all duration-200 cursor-pointer py-0.5 relative
					{isActive 
						? 'text-brand-700 font-semibold' 
						: 'text-base-content/80 hover:text-base-content'}"
			>
				<!-- Tree connector lines container -->
				<div class="tree-connectors absolute inset-0 pointer-events-none" aria-hidden="true">
					{#each ancestorIsLast as isAncestorLast, ancestorIdx}
						<!-- Vertical continuation line from each ancestor -->
						<!-- Position based on ancestor depth: level 0 = 6px, each level adds 16px -->
						{@const leftPos = 6 + ancestorIdx * 16}
						{#if !isAncestorLast}
							<div 
								class="absolute top-0 bottom-0 w-[1px] bg-base-content/10"
								style="left: {leftPos}px;"
							></div>
						{/if}
					{/each}
					
					{#if level > 0}
						<!-- Horizontal branch line to this folder (T or L connector) -->
						{@const branchLeft = 6 + (level - 1) * 16}
						{@const connectorWidth = 10}
						<div 
							class="absolute w-[10px] h-[1px] bg-base-content/10"
							class:top-[14px]={!isLast}
							class:top-[15px]={isLast}
							style="left: {branchLeft}px;"
						></div>
						
						{#if !isLast}
							<!-- Vertical continuation line down to siblings (drawn from this folder downward) -->
							{@const verticalLeft = 6 + (level - 1) * 16}
							<div 
								class="absolute top-[14px] bottom-0 w-[1px] bg-base-content/10"
								style="left: {verticalLeft}px;"
							></div>
						{:else}
							<!-- L-connector: small vertical segment down from the horizontal line -->
							{@const verticalLeft = 6 + (level - 1) * 16}
							<div 
								class="absolute top-[15px] h-[6px] w-[1px] bg-base-content/10"
								style="left: {verticalLeft}px;"
							></div>
						{/if}
					{/if}
				</div>

				<!-- Expand/Collapse button -->
				<button
					type="button"
					class="w-4 h-4 flex items-center justify-center rounded hover:bg-base-200/60 transition-colors shrink-0 z-10 bg-base-100 relative
						{hasChildrenValue ? '' : 'invisible'}"
					style="margin-left: {level * 16}px;"
					onclick={(e) => toggleFolder(e, folder.folder.id)}
					aria-label={isExpanded ? 'Collapse folder' : 'Expand folder'}
					tabindex="-1"
				>
					<ChevronRight 
						size={12} 
						class="transition-transform duration-200 {isExpanded ? 'rotate-90' : ''}" 
					/>
				</button>
				
				<!-- Folder link -->
				<button
					type="button"
					class="flex-1 flex items-center gap-2 py-1 text-left min-w-0 text-[13px] rounded-md px-1.5 transition-colors relative z-10
						{isActive ? 'bg-brand-500/10' : 'hover:bg-base-200/40'}"
					onclick={() => navigateToFolder(folder.folder.id)}
				>
					{#if isActive}
						<FolderOpen size={14} class="text-brand-500 shrink-0" />
					{:else}
						<Folder size={14} class="text-base-content/40 shrink-0" />
					{/if}
					<span class="flex-1 truncate">{folder.folder.name}</span>
				</button>
			</div>
			
			<!-- Children - Recursive -->
			{#if isExpanded && hasChildrenValue}
				<div class="children-container">
					<SidebarFolderTree 
						folders={folder.subfolders} 
						level={level + 1}
						{onFolderClick}
						{getExpandedIds}
						ancestorIsLast={childAncestorIsLast}
					/>
				</div>
			{/if}
		</div>
	{/each}
</div>

<style>
	.folder-tree-container {
		position: relative;
	}

	.folder-tree-node {
		position: relative;
	}

	.tree-connectors {
		z-index: 0;
	}
</style>
