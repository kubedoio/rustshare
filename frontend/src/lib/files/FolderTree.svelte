<script lang="ts">
	import { ChevronRight, Folder, FolderOpen } from 'lucide-svelte';
	import { fileBrowserUi } from '$lib/stores/fileBrowserUi';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import type { FolderTree as FolderTreeType } from '$lib/api/folders';
	import FolderTree from './FolderTree.svelte';

	interface Props {
		folders: FolderTreeType[];
		depth?: number;
		onFolderClick?: (folderId: string) => void;
		// Ancestor IDs of the currently selected folder (for emphasis)
		ancestorIds?: Set<string>;
	}

	let { 
		folders,
		depth = 0,
		onFolderClick = () => {},
		ancestorIds = new Set()
	}: Props = $props();

	const INDENT_SIZE = 18; // pixels per depth level

	function isExpanded(folderId: string): boolean {
		return $fileBrowserUi.expandedFolderIds.has(folderId);
	}

	function isActive(folder: FolderTreeType): boolean {
		const currentFolderId = $page.url.searchParams.get('folder');
		const isRoot = isRootFolder(folder);
		
		if (isRoot) {
			// Root is active when no folder param (at /files) or when folder param matches root ID
			return !currentFolderId || currentFolderId === folder.folder.id;
		}
		
		return currentFolderId === folder.folder.id;
	}

	function isAncestor(folderId: string): boolean {
		return ancestorIds.has(folderId);
	}

	function toggleExpand(e: MouseEvent, folderId: string) {
		e.stopPropagation();
		fileBrowserUi.toggleFolderExpanded(folderId);
	}

	function isRootFolder(folder: FolderTreeType): boolean {
		// Root folder has no parent
		return !folder.folder.parent_folder_id;
	}

	function navigateToFolder(folder: FolderTreeType) {
		const folderId = folder.folder.id;
		const isRoot = isRootFolder(folder);
		
		fileBrowserUi.selectFolder(folderId);
		
		if (isRoot) {
			// Navigate to home (no folder param) for root
			goto('/files');
		} else {
			goto(`/files?folder=${folderId}`);
		}
		
		onFolderClick(folderId);
	}

	function hasChildren(folder: FolderTreeType): boolean {
		return folder.subfolders && folder.subfolders.length > 0;
	}
</script>

<div class="folder-tree">
	{#each folders as folder (folder.folder.id)}
		{@const folderId = folder.folder.id}
		{@const expanded = isExpanded(folderId)}
		{@const active = isActive(folder)}
		{@const isAncestorOfActive = isAncestor(folderId)}
		{@const hasChildrenValue = hasChildren(folder)}
		{@const indentPx = depth * INDENT_SIZE}

		<div class="tree-node">
			<!-- Folder Row -->
			<div
				class="folder-row"
				class:active
				class:is-ancestor={isAncestorOfActive}
				style="padding-left: {indentPx}px"
			>
				<!-- Chevron (clickable for expand/collapse) -->
				<button
					type="button"
					class="chevron"
					class:expanded
					class:invisible={!hasChildrenValue}
					onclick={(e) => toggleExpand(e, folderId)}
					aria-label={expanded ? 'Collapse folder' : 'Expand folder'}
				>
					<ChevronRight size={14} />
				</button>

				<!-- Folder Content (clickable for navigation) -->
				<button
					type="button"
					class="folder-content"
					onclick={() => navigateToFolder(folder)}
					aria-current={active ? 'page' : undefined}
				>
					<!-- Folder Icon -->
					<span class="folder-icon-wrapper">
						{#if active}
							<FolderOpen size={16} class="folder-icon active" />
						{:else if isAncestorOfActive}
							<Folder size={16} class="folder-icon is-ancestor" />
						{:else}
							<Folder size={16} class="folder-icon" />
						{/if}
					</span>

					<!-- Folder Name -->
					<span class="folder-name" class:active class:is-ancestor={isAncestorOfActive}>
						{folder.folder.name}
					</span>
				</button>
			</div>

			<!-- Children -->
			{#if expanded && hasChildrenValue}
				<div class="children-container">
					<FolderTree
						folders={folder.subfolders}
						depth={depth + 1}
						onFolderClick={onFolderClick}
						{ancestorIds}
					/>
				</div>
			{/if}
		</div>
	{/each}
</div>

<style>
	.folder-tree {
		display: flex;
		flex-direction: column;
	}

	.tree-node {
		display: flex;
		flex-direction: column;
	}

	.folder-row {
		display: flex;
		align-items: center;
		margin: 1px 4px;
		border-radius: 6px;
		min-width: 0;
		transition: background-color 0.12s ease;
	}

	.folder-row:hover {
		background-color: hsl(var(--bc) / 0.05);
	}

	.folder-row.active {
		background-color: hsl(var(--p) / 0.12);
	}

	.folder-row.active:hover {
		background-color: hsl(var(--p) / 0.18);
	}

	/* Chevron */
	.chevron {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		flex-shrink: 0;
		border-radius: 6px;
		color: hsl(var(--bc) / 0.4);
		transition: transform 0.15s ease, background-color 0.12s ease, color 0.12s ease;
		cursor: pointer;
		background: transparent;
		border: none;
		padding: 0;
	}

	.chevron:hover {
		background-color: hsl(var(--bc) / 0.08);
		color: hsl(var(--bc) / 0.7);
	}

	.chevron.expanded {
		transform: rotate(90deg);
	}

	.chevron.invisible {
		visibility: hidden;
		pointer-events: none;
	}

	/* Folder Content (clickable for navigation) */
	.folder-content {
		flex: 1;
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 8px 6px 0;
		font-size: 13px;
		color: hsl(var(--bc) / 0.85);
		background: transparent;
		border: none;
		cursor: pointer;
		text-align: left;
		transition: color 0.12s ease;
		min-width: 0;
	}

	.folder-row.active .folder-content {
		color: hsl(var(--p));
		font-weight: 500;
	}

	.folder-row.is-ancestor .folder-content {
		color: hsl(var(--bc));
		font-weight: 500;
	}

	/* Folder Icon */
	.folder-icon-wrapper {
		display: flex;
		align-items: center;
		flex-shrink: 0;
	}

	:global(.folder-icon) {
		color: hsl(var(--bc) / 0.45);
		transition: color 0.12s ease;
	}

	:global(.folder-icon.active) {
		color: hsl(var(--p));
	}

	:global(.folder-icon.is-ancestor) {
		color: hsl(var(--bc) / 0.7);
	}

	/* Folder Name */
	.folder-name {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		min-width: 0;
	}

	.folder-name.active {
		font-weight: 500;
	}

	.folder-name.is-ancestor {
		font-weight: 500;
	}

	/* Children container */
	.children-container {
		display: flex;
		flex-direction: column;
	}
</style>
