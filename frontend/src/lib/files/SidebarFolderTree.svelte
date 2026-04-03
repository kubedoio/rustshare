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
		onFolderClick?: (folderId: string | null) => void;
		getExpandedIds?: () => Set<string>;
		// Ancestry metadata for tree drawing
		// Each boolean indicates if the ancestor at that depth has more siblings after it
		ancestorHasNextSibling?: boolean[];
	}

	let { 
		folders,
		onFolderClick = () => {},
		getExpandedIds = () => new Set(),
		ancestorHasNextSibling = []
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

<div class="folder-tree">
	{#each folders as folder, i (folder.folder.id)}
		{@const folderId = folder.folder.id}
		{@const isExpanded = isFolderExpanded(folderId)}
		{@const isActive = $page.url.searchParams.get('folder') === folderId}
		{@const hasChildrenValue = hasChildren(folder)}
		{@const isLastChild = i === folders.length - 1}
		<!-- For children: extend ancestry with whether THIS node has next siblings -->
		{@const childAncestorInfo = [...ancestorHasNextSibling, !isLastChild]}

		<div class="folder-row" class:is-last={isLastChild}>
			<!-- Ancestor guide columns - one per ancestor depth -->
			{#each ancestorHasNextSibling as hasNextAtDepth}
				<div class="guide-column">
					{#if hasNextAtDepth}
						<!-- Vertical continuation line - branch continues below -->
						<div class="guide-line-vertical"></div>
					{/if}
				</div>
			{/each}

			<!-- Current node connector column -->
			<div class="guide-column current-node-column">
				{#if ancestorHasNextSibling.length > 0}
					<!-- Branch connector: tee (├) if not last, elbow (└) if last -->
					<div class="branch-connector" class:is-last={isLastChild}>
						<div class="connector-horizontal"></div>
						{#if !isLastChild}
							<!-- Tee connector - vertical continues down for siblings -->
							<div class="connector-vertical-continuation"></div>
						{:else}
							<!-- Elbow connector - just a small stub down -->
							<div class="connector-vertical-stub"></div>
						{/if}
					</div>
				{/if}
			</div>

			<!-- Expand/Collapse button -->
			<button
				type="button"
				class="chevron-button"
				class:invisible={!hasChildrenValue}
				onclick={(e) => toggleFolder(e, folder.folder.id)}
				aria-label={isExpanded ? 'Collapse folder' : 'Expand folder'}
				tabindex="-1"
			>
				<ChevronRight 
					size={12} 
					class="chevron-icon {isExpanded ? 'rotate-90' : ''}" 
				/>
			</button>
			
			<!-- Folder link -->
			<button
				type="button"
				class="folder-button"
				class:active={isActive}
				onclick={() => navigateToFolder(folder.folder.id)}
			>
				{#if isActive}
					<FolderOpen size={14} class="folder-icon active" />
				{:else}
					<Folder size={14} class="folder-icon" />
				{/if}
				<span class="folder-name">{folder.folder.name}</span>
			</button>
		</div>

		<!-- Children - Recursive -->
		{#if isExpanded && hasChildrenValue}
			<div class="children-container">
				<SidebarFolderTree 
					folders={folder.subfolders} 
					onFolderClick={onFolderClick}
					getExpandedIds={getExpandedIds}
					ancestorHasNextSibling={childAncestorInfo}
				/>
			</div>
		{/if}
	{/each}
</div>

<style>
	/* Tree container */
	.folder-tree {
		display: flex;
		flex-direction: column;
	}

	/* Each row is a flex container with fixed-width guide columns */
	.folder-row {
		display: flex;
		align-items: center;
		height: 28px;
		position: relative;
	}

	/* Guide column - fixed width for each depth level */
	.guide-column {
		width: 16px;
		height: 100%;
		position: relative;
		flex-shrink: 0;
	}

	/* Vertical continuation line for ancestors that have more siblings */
	.guide-line-vertical {
		position: absolute;
		left: 50%;
		top: 0;
		bottom: 0;
		width: 1px;
		background-color: var(--base-content-color, hsl(var(--bc) / 0.1));
		transform: translateX(-50%);
	}

	/* Current node connector column */
	.current-node-column {
		position: relative;
	}

	/* Branch connector container */
	.branch-connector {
		position: absolute;
		left: 0;
		right: 0;
		top: 0;
		bottom: 0;
	}

	/* Horizontal line from parent to this node */
	.connector-horizontal {
		position: absolute;
		left: 0;
		width: 50%;
		top: 50%;
		height: 1px;
		background-color: var(--base-content-color, hsl(var(--bc) / 0.1));
	}

	/* Vertical continuation for tee connector (not last child) */
	.connector-vertical-continuation {
		position: absolute;
		left: 50%;
		top: 50%;
		bottom: 0;
		width: 1px;
		background-color: var(--base-content-color, hsl(var(--bc) / 0.1));
		transform: translateX(-50%);
	}

	/* Vertical stub for elbow connector (last child) */
	.connector-vertical-stub {
		position: absolute;
		left: 50%;
		top: 50%;
		width: 1px;
		height: 4px;
		background-color: var(--base-content-color, hsl(var(--bc) / 0.1));
		transform: translateX(-50%);
	}

	/* Chevron button */
	.chevron-button {
		width: 16px;
		height: 16px;
		flex-shrink: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		border-radius: 4px;
		transition: background-color 0.15s;
		background: transparent;
		border: none;
		cursor: pointer;
		padding: 0;
		color: hsl(var(--bc) / 0.4);
	}

	.chevron-button:hover {
		background-color: hsl(var(--bc) / 0.1);
	}

	.chevron-button.invisible {
		visibility: hidden;
		pointer-events: none;
	}

	.chevron-icon {
		transition: transform 0.2s;
	}

	.chevron-icon.rotate-90 {
		transform: rotate(90deg);
	}

	/* Folder button */
	.folder-button {
		flex: 1;
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 3px 6px;
		margin-left: 2px;
		border-radius: 4px;
		font-size: 13px;
		line-height: 1.4;
		background: transparent;
		border: none;
		cursor: pointer;
		text-align: left;
		color: hsl(var(--bc) / 0.8);
		transition: background-color 0.15s, color 0.15s;
		min-width: 0; /* Allow truncation */
	}

	.folder-button:hover {
		background-color: hsl(var(--bc) / 0.05);
		color: hsl(var(--bc));
	}

	.folder-button.active {
		background-color: hsl(var(--p) / 0.1);
		color: hsl(var(--p));
		font-weight: 500;
	}

	.folder-icon {
		flex-shrink: 0;
		color: hsl(var(--bc) / 0.4);
	}

	.folder-icon.active {
		color: hsl(var(--p));
	}

	.folder-name {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	/* Children container */
	.children-container {
		display: flex;
		flex-direction: column;
	}

	/* Fallback for CSS variables if not defined */
	:global(:root) {
		--base-content-color: hsl(var(--bc) / 0.1);
	}
</style>
