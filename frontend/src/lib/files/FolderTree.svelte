<script lang="ts">
	/**
	 * ==============================================================================
	 * FOLDER TREE COMPONENT
	 * ==============================================================================
	 *
	 * Refactored to support dual-root structure (My Files + Shared).
	 *
	 * Per SPEC:
	 * - My Files and Shared must have identical interaction behavior
	 * - Tree renderer must be shared, not duplicated
	 * - Shared root must use the provided SVG icon
	 * - Selected state reflects store state
	 */

	import { ChevronRight, Folder, FolderOpen, Loader2 } from 'lucide-svelte';
	import { fileBrowserUi } from '$lib/stores/fileBrowserUi';
	import { folderTreeStore } from '$lib/stores/folderTree';
	import { page } from '$app/stores';
	import { createMutation } from '$lib/query-compat';
	import { queryClient } from '$lib/query-client';
	import { moveFile } from '$lib/api/files';
	import { moveFolder, type FolderTree as FolderTreeType } from '$lib/api/folders';
	import ShareIndicator from '$lib/components/files/ShareIndicator.svelte';
	import FolderTree from './FolderTree.svelte';
	import type { ExplorerRoot } from '$lib/explorer';

	interface Props {
		folders: FolderTreeType[];
		depth?: number;
		onFolderClick?: (folderId: string | null) => void;
		// Ancestor IDs of the currently selected folder (for emphasis)
		ancestorIds?: Set<string>;
		// Root type for this tree section
		rootType?: ExplorerRoot;
		// Whether this tree section is currently active
		isActive?: boolean;
		// Custom icon SVG for the root (used for Shared)
		sharedIcon?: string;
	}

	let {
		folders,
		depth = 0,
		onFolderClick = () => {},
		ancestorIds = new Set(),
		rootType = 'my-files',
		isActive = true,
		sharedIcon = ''
	}: Props = $props();

	const INDENT_SIZE = 18; // pixels per depth level

	// ============================================================================
	// STATE CHECKS
	// ============================================================================

	function isExpanded(folderId: string): boolean {
		return $fileBrowserUi.expandedFolderIds.has(folderId);
	}

	function isActiveFolder(folder: FolderTreeType): boolean {
		// If this tree section is not active, no folder is active
		if (!isActive) return false;

		const currentFolderId = $page.url.searchParams.get('folder');
		const currentRoot = ($page.url.searchParams.get('root') as ExplorerRoot) || 'my-files';

		// Only match if we're in the same root
		if (currentRoot !== rootType) return false;

		// If we're at the root of the file system (/files without folder param)
		if (!currentFolderId) {
			// Only highlight the actual top-level node in the sidebar tree
			return depth === 0;
		}

		// Otherwise, only the folder whose ID matches the URL param is active
		return currentFolderId === folder.folder.id;
	}

	function isAncestor(folderId: string): boolean {
		// Only show ancestor highlight if this tree section is active
		if (!isActive) return false;
		return ancestorIds.has(folderId);
	}

	function hasChildren(folder: FolderTreeType): boolean {
		return folder.subfolders && folder.subfolders.length > 0;
	}

	// ============================================================================
	// EVENT HANDLERS
	// ============================================================================

	function toggleExpand(e: MouseEvent, folderId: string) {
		e.stopPropagation();
		fileBrowserUi.toggleFolderExpanded(folderId);
	}

	function navigateToFolder(folder: FolderTreeType) {
		const folderId = folder.folder.id;
		const isRoot = depth === 0;

		fileBrowserUi.selectFolder(folderId);
		// For root folder, pass null to navigate to root URL (/files)
		onFolderClick(isRoot ? null : folderId);
	}

	// ============================================================================
	// DRAG & DROP STATE
	// ============================================================================

	let draggedOverFolderId = $state<string | null>(null);

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
		mutationFn: ({
			folderId,
			targetFolderId
		}: {
			folderId: string;
			targetFolderId: string | null;
		}) => moveFolder(folderId, targetFolderId),
		onSuccess: (_, { folderId, targetFolderId }) => {
			folderTreeStore.moveFolder(folderId, targetFolderId);
			if (targetFolderId) {
				fileBrowserUi.expandFolder(targetFolderId);
			}
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['folder-tree'] });
			queryClient.invalidateQueries({ queryKey: ['all-files'] });
		}
	});

	function handleDragStart(e: DragEvent, folderToDrag: FolderTreeType) {
		if (e.dataTransfer) {
			e.dataTransfer.effectAllowed = 'move';
			e.dataTransfer.setData(
				'application/json',
				JSON.stringify({
					id: folderToDrag.folder.id,
					isFolder: true,
					name: folderToDrag.folder.name,
					parentFolderId: folderToDrag.folder.parent_folder_id
				})
			);
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

	// ============================================================================
	// ICON RENDERING
	// ============================================================================

	function isRootNode(folder: FolderTreeType): boolean {
		return depth === 0;
	}

	function shouldUseSharedIcon(folder: FolderTreeType): boolean {
		return isRootNode(folder) && rootType === 'shared' && !!sharedIcon;
	}
</script>

<div class="folder-tree">
	{#each folders as folder (folder.folder.id)}
		{@const folderId = folder.folder.id}
		{@const expanded = isExpanded(folderId)}
		{@const active = isActiveFolder(folder)}
		{@const isAncestorOfActive = isAncestor(folderId)}
		{@const hasChildrenValue = hasChildren(folder)}
		{@const indentPx = depth * INDENT_SIZE}
		{@const useSharedIcon = shouldUseSharedIcon(folder)}
		{@const isRoot = isRootNode(folder)}

		<div class="tree-node">
			<!-- Folder Row -->
			<div
				class="folder-row"
				class:active
				class:is-ancestor={isAncestorOfActive}
				class:drag-over={draggedOverFolderId === folderId}
				class:is-moving={isMoving(folderId)}
				class:root-node={isRoot}
				style="padding-left: {indentPx}px"
				onclick={() => navigateToFolder(folder)}
				role="button"
				tabindex="0"
				onkeydown={(e) => {
					if (e.key === 'Enter' || e.key === ' ') {
						e.preventDefault();
						navigateToFolder(folder);
					}
				}}
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
					{:else if useSharedIcon}
						<!-- Shared Root Icon (from SPEC section 1.4) -->
						<span class="shared-icon" class:active>
							{@html sharedIcon}
						</span>
					{:else if active || expanded}
						<FolderOpen size={16} class="folder-icon {active ? 'active' : ''}" />
					{:else}
						<Folder size={16} class="folder-icon {isAncestorOfActive ? 'is-ancestor' : ''}" />
					{/if}
				</span>

				<!-- Folder Name -->
				<span
					class="folder-name"
					class:active
					class:is-ancestor={isAncestorOfActive}
					class:root-node={isRoot}
				>
					{folder.folder.name}
				</span>

				<!-- Share Indicator -->
				{#if folder.folder.is_shared && !useSharedIcon}
					<ShareIndicator
						isShared={folder.folder.is_shared}
						shareCount={folder.folder.share_count}
						shareExpiresAt={folder.folder.share_expires_at}
						size="xs"
					/>
				{/if}
			</div>

			<!-- Children -->
			{#if expanded && hasChildrenValue}
				<div class="children-container">
					<FolderTree
						folders={folder.subfolders}
						depth={depth + 1}
						{onFolderClick}
						{ancestorIds}
						{rootType}
						{isActive}
						{sharedIcon}
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

	.folder-row.root-node {
		font-weight: 500;
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
		transition:
			transform 0.15s ease,
			background-color 0.12s ease,
			color 0.12s ease;
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

	/* Shared Icon */
	.shared-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		color: color-mix(in srgb, var(--rs-text-muted, #6c665f) 70%, transparent);
		transition: color 0.12s ease;
	}

	.shared-icon :global(svg) {
		width: 16px;
		height: 16px;
	}

	.shared-icon.active {
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

	.folder-name.root-node {
		font-weight: 500;
	}

	/* Children container */
	.children-container {
		display: flex;
		flex-direction: column;
	}
</style>
