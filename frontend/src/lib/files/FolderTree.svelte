<script lang="ts">
	import { ChevronRight, Folder, FolderOpen, Loader2 } from 'lucide-svelte';
	import { fileBrowserUi } from '$lib/stores/fileBrowserUi';
	import { folderTreeStore } from '$lib/stores/folderTree';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { createMutation } from '@tanstack/svelte-query';
	import { queryClient } from '$lib/query-client';
	import { moveFile } from '$lib/api/files';
	import { moveFolder, type FolderTree as FolderTreeType } from '$lib/api/folders';
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
		
		// If we're at the root of the file system (/files without folder param)
		if (!currentFolderId) {
			// Only highlight the actual top-level node in the sidebar tree
			return depth === 0;
		}
		
		// Otherwise, only the folder whose ID matches the URL param is active
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
		const isRoot = depth === 0;
		
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

	// Drag & Drop State
	let draggedOverFolderId = $state<string | null>(null);

	// Mutations
	const moveFileMutation = createMutation({
		mutationFn: ({ fileId, targetFolderId }: { fileId: string; targetFolderId: string | null }) => 
			moveFile(fileId, targetFolderId),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['folder-tree'] });
			queryClient.invalidateQueries({ queryKey: ['all-files'] });
		}
	});

	const moveFolderMutation = createMutation({
		mutationFn: ({ folderId, targetFolderId }: { folderId: string; targetFolderId: string | null }) => 
			moveFolder(folderId, targetFolderId),
		onSuccess: (_, { folderId, targetFolderId }) => {
			// Optimistically update the store if possible
			folderTreeStore.moveFolder(folderId, targetFolderId);
			// Expand destination folder so moved folder is visible
			if (targetFolderId) {
				fileBrowserUi.expandFolder(targetFolderId);
			}
			// Refresh queries
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['folder-tree'] });
			queryClient.invalidateQueries({ queryKey: ['all-files'] });
		}
	});

	// D&D Handlers
	function handleDragStart(e: DragEvent, folderToDrag: FolderTreeType) {
		if (e.dataTransfer) {
			e.dataTransfer.effectAllowed = 'move';
			e.dataTransfer.setData('application/json', JSON.stringify({
				id: folderToDrag.folder.id,
				isFolder: true,
				name: folderToDrag.folder.name,
				parentFolderId: folderToDrag.folder.parent_folder_id
			}));
		}
	}

	function handleDragOver(e: DragEvent, targetFolderId: string) {
		e.preventDefault();
		if (e.dataTransfer) {
			e.dataTransfer.dropEffect = 'move';
		}
		draggedOverFolderId = targetFolderId;
	}

	function handleDragLeave() {
		draggedOverFolderId = null;
	}

	async function handleDrop(e: DragEvent, targetFolderId: string) {
		e.preventDefault();
		draggedOverFolderId = null;

		if (!e.dataTransfer) return;

		try {
			const data = JSON.parse(e.dataTransfer.getData('application/json'));
			const { id: itemId, isFolder, parentFolderId: oldParentId } = data;

			// Don't drop on self or current parent
			if (itemId === targetFolderId || oldParentId === targetFolderId) return;

			if (isFolder) {
				await $moveFolderMutation.mutateAsync({ folderId: itemId, targetFolderId });
			} else {
				await $moveFileMutation.mutateAsync({ fileId: itemId, targetFolderId });
			}
		} catch (err) {
			console.error('Failed to parse drag data or perform move:', err);
		}
	}

	function isMoving(folderId: string): boolean {
		return $moveFolderMutation.isPending && $moveFolderMutation.variables?.folderId === folderId;
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
				class:drag-over={draggedOverFolderId === folderId}
				class:is-moving={isMoving(folderId)}
				style="padding-left: {indentPx}px"
				onclick={() => navigateToFolder(folder)}
				role="button"
				tabindex="0"
				onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); navigateToFolder(folder); } }}
				draggable={true}
				ondragstart={(e) => handleDragStart(e, folder)}
				ondragover={(e) => handleDragOver(e, folderId)}
				ondragleave={handleDragLeave}
				ondrop={(e) => handleDrop(e, folderId)}
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

				<!-- Folder Icon -->
				<span class="folder-icon-wrapper">
					{#if isMoving(folderId) || ($moveFileMutation.isPending && $moveFileMutation.variables?.targetFolderId === folderId)}
						<Loader2 size={16} class="animate-spin text-brand-500" />
					{:else if active || expanded}
						<FolderOpen 
							size={16} 
							class="folder-icon {active ? 'active' : ''}" 
						/>
					{:else}
						<Folder 
							size={16} 
							class="folder-icon {isAncestorOfActive ? 'is-ancestor' : ''}" 
						/>
					{/if}
				</span>

				<!-- Folder Name -->
				<span class="folder-name" class:active class:is-ancestor={isAncestorOfActive}>
					{folder.folder.name}
				</span>
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
		position: relative;
		z-index: 10;
		padding: 5px 8px 5px 0;
		margin: 1px 4px;
		border-radius: 6px;
		min-width: 0;
		cursor: pointer;
		transition: background-color 0.12s ease;
		gap: 6px;
	}

	.folder-row:hover {
		background-color: hsl(var(--bc) / 0.05);
	}

	.folder-row.active {
		background-color: color-mix(in srgb, var(--rs-brand, #c65a1e) 8%, transparent);
	}

	.folder-row.drag-over {
		background-color: color-mix(in srgb, var(--rs-brand, #c65a1e) 15%, transparent);
		box-shadow: inset 0 0 0 2px var(--rs-brand, #c65a1e);
	}

	.folder-row.is-moving {
		opacity: 0.6;
		pointer-events: none;
	}

	.folder-row.active:hover {
		background-color: color-mix(in srgb, var(--rs-brand, #c65a1e) 18%, transparent);
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
		color: color-mix(in srgb, var(--rs-text-muted, #6c665f) 40%, transparent);
		transition: transform 0.15s ease, background-color 0.12s ease, color 0.12s ease;
		cursor: pointer;
		background: transparent;
		border: none;
		padding: 0;
		position: relative;
		z-index: 20;
	}

	.chevron:hover {
		background-color: color-mix(in srgb, var(--rs-text-muted, #6c665f) 10%, transparent);
		color: var(--rs-text, #151515);
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
		color: color-mix(in srgb, var(--rs-text-muted, #6c665f) 70%, transparent);
		transition: color 0.12s ease;
	}

	:global(.folder-icon.active) {
		color: var(--rs-brand, #c65a1e);
	}

	/* Folder Name */
	.folder-name {
		flex: 1;
		font-size: 13.5px;
		color: var(--rs-text-soft, #3e3a35);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		min-width: 0;
		transition: color 0.12s ease;
	}

	.folder-row.active .folder-name {
		color: var(--rs-brand, #c65a1e);
		font-weight: 600;
	}

	.folder-row.is-ancestor .folder-name {
		color: var(--rs-text, #151515);
		font-weight: 500;
	}

	/* Children container */
	.children-container {
		display: flex;
		flex-direction: column;
	}
</style>
