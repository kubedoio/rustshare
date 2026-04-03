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

<div class="folder-tree-container {level > 0 ? 'ml-3 border-l border-base-content/10' : ''}">
	{#each folders as folder, i (folder.folder.id)}
		{@const folderId = folder.folder.id}
		{@const isExpanded = isFolderExpanded(folderId)}
		{@const isActive = $page.url.searchParams.get('folder') === folderId}
		{@const hasChildrenValue = hasChildren(folder)}
		{@const isLast = i === folders.length - 1}

		<div class="folder-tree-node relative">
			<!-- Folder Item -->
			<div
				class="group flex items-center gap-1 transition-all duration-200 cursor-pointer py-0.5
					{isActive 
						? 'text-brand-700 font-semibold' 
						: 'text-base-content/80 hover:text-base-content'}"
			>
				{#if level > 0}
					<!-- Classic Tree Line (Horizontal) -->
					<div class="absolute left-[-12px] top-[14px] w-3 h-[1px] bg-base-content/10"></div>
					{#if isLast}
						<!-- Cover the bottom of the line for last item -->
						<div class="absolute left-[-13px] top-[15px] w-[1px] h-full bg-base-100"></div>
					{/if}
				{/if}

				<!-- Expand/Collapse button -->
				<button
					type="button"
					class="w-4 h-4 flex items-center justify-center rounded hover:bg-base-200/60 transition-colors shrink-0 z-10 bg-base-100
						{hasChildrenValue ? '' : 'invisible'}"
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
					class="flex-1 flex items-center gap-2 py-1 text-left min-w-0 text-[13px] rounded-md px-1.5 transition-colors
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
				<div class="mt-0">
					<SidebarFolderTree 
						folders={folder.subfolders} 
						level={level + 1}
						{onFolderClick}
						{getExpandedIds}
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

	/* Last item vertical line cleanup */
	.folder-tree-node:last-child > .mt-0 {
		border-left: none;
	}
</style>
