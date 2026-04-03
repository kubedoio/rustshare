<script lang="ts">
	import { ChevronRight, Folder, FolderOpen } from 'lucide-svelte';
	import { fileBrowserUi } from '$lib/stores/fileBrowserUi';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import type { FolderTree as FolderTreeType } from '$lib/api/folders';

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

	function isActive(folderId: string): boolean {
		const currentFolderId = $page.url.searchParams.get('folder');
		return currentFolderId === folderId;
	}

	function isAncestor(folderId: string): boolean {
		return ancestorIds.has(folderId);
	}

	function toggleExpand(e: MouseEvent, folderId: string) {
		e.stopPropagation();
		fileBrowserUi.toggleFolderExpanded(folderId);
	}

	function navigateToFolder(folderId: string) {
		fileBrowserUi.selectFolder(folderId);
		goto(`/files?folder=${folderId}`);
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
		{@const active = isActive(folderId)}
		{@const isAncestorOfActive = isAncestor(folderId)}
		{@const hasChildrenValue = hasChildren(folder)}
		{@const indentPx = depth * INDENT_SIZE}

		<div class="tree-node">
			<!-- Folder Row -->
			<button
				type="button"
				class="folder-row"
				class:active
				class:ancestor={isAncestorOfActive}
				style="padding-left: {indentPx}px"
				onclick={() => navigateToFolder(folderId)}
				aria-current={active ? 'page' : undefined}
			>
				<!-- Chevron (clickable for expand/collapse) -->
				<span
					class="chevron"
					class:expanded
					class:invisible={!hasChildrenValue}
					onclick={(e) => toggleExpand(e, folderId)}
					role="button"
					tabindex="-1"
					aria-label={expanded ? 'Collapse folder' : 'Expand folder'}
				>
					<ChevronRight size={14} />
				</span>

				<!-- Folder Icon -->
				<span class="folder-icon-wrapper">
					{#if active}
						<FolderOpen size={16} class="folder-icon active" />
					{:else if isAncestorOfActive}
						<Folder size={16} class="folder-icon ancestor" />
					{:else}
						<Folder size={16} class="folder-icon" />
					{/if}
				</span>

				<!-- Folder Name -->
				<span class="folder-name" class:active class:ancestor={isAncestorOfActive}>
					{folder.folder.name}
				</span>
			</button>

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
		gap: 4px;
		padding: 5px 8px 5px 0;
		margin: 1px 4px;
		border-radius: 6px;
		font-size: 13px;
		color: hsl(var(--bc) / 0.85);
		background: transparent;
		border: none;
		cursor: pointer;
		text-align: left;
		transition: background-color 0.12s ease, color 0.12s ease;
		min-width: 0;
	}

	.folder-row:hover {
		background-color: hsl(var(--bc) / 0.05);
		color: hsl(var(--bc));
	}

	.folder-row.active {
		background-color: hsl(var(--p) / 0.12);
		color: hsl(var(--p));
		font-weight: 500;
	}

	.folder-row.active:hover {
		background-color: hsl(var(--p) / 0.18);
	}

	.folder-row.ancestor {
		color: hsl(var(--bc));
	}

	/* Chevron */
	.chevron {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 18px;
		height: 18px;
		flex-shrink: 0;
		border-radius: 4px;
		color: hsl(var(--bc) / 0.5);
		transition: transform 0.15s ease, background-color 0.12s ease;
	}

	.chevron:hover {
		background-color: hsl(var(--bc) / 0.08);
	}

	.chevron.expanded {
		transform: rotate(90deg);
	}

	.chevron.invisible {
		visibility: hidden;
		pointer-events: none;
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

	:global(.folder-icon.ancestor) {
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

	.folder-name.ancestor {
		font-weight: 500;
	}

	/* Children container */
	.children-container {
		display: flex;
		flex-direction: column;
	}
</style>
