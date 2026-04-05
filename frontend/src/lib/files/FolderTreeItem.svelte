<script lang="ts">
	import { Folder, ChevronRight, Loader as Loader2, Check, X } from 'lucide-svelte';
	import { folderTreeStore, type FolderNode } from '$lib/stores/folderTree';

	export let folder: FolderNode;
	export let level = 0;
	export let renamingFolderId: string | null = null;
	export let renameValue = '';
	export let renameInputRef: HTMLInputElement | null = null;
	export let draggedOverFolderId: string | null = null;
	
	export let onSelect: (folder: FolderNode) => void;
	export let onToggleExpand: (folder: FolderNode) => void;
	export let onContextMenu: (e: MouseEvent, folder: FolderNode) => void;
	export let onRenameConfirm: (folder: FolderNode) => void;
	export let onRenameCancel: () => void;
	export let onRenameKeydown: (e: KeyboardEvent, folder: FolderNode) => void;
	export let onDragOver: (folderId: string) => void = () => {};
	export let onDragLeave: () => void = () => {};
	export let onDrop: (folder: FolderNode) => void = () => {};

	$: isSelected = $folderTreeStore.selectedId === folder.id;
	$: isExpanded = $folderTreeStore.expandedIds.has(folder.id);
	$: isLoading = $folderTreeStore.loadingIds.has(folder.id);
	$: isRenaming = renamingFolderId === folder.id;
	$: isDragOver = draggedOverFolderId === folder.id;

	function handleClick() {
		onSelect(folder);
	}

	function handleToggle(e: MouseEvent) {
		e.stopPropagation();
		onToggleExpand(folder);
	}

	function handleContextMenu(e: MouseEvent) {
		onContextMenu(e, folder);
	}

	// Drag and drop
	function handleDragStart(e: DragEvent) {
		if (e.dataTransfer) {
			e.dataTransfer.effectAllowed = 'move';
			e.dataTransfer.setData('application/json', JSON.stringify({ 
				id: folder.id, 
				isFolder: true,
				name: folder.name 
			}));
		}
	}

	function handleDragOver(e: DragEvent) {
		e.preventDefault();
		onDragOver(folder.id);
	}

	function handleDragLeave() {
		onDragLeave();
	}

	function handleDrop(e: DragEvent) {
		e.preventDefault();
		onDrop(folder);
	}
</script>

<div class="select-none">
	<div
		class="group flex items-center gap-1.5 px-2 py-1 mx-1 rounded-md text-[13px] transition-all cursor-pointer
			{isSelected 
				? 'bg-brand-500/15 text-brand-600 font-medium' 
				: 'text-base-content/70 hover:bg-base-200/50 hover:text-base-content'}
			{isDragOver ? 'ring-2 ring-brand-500/50 bg-brand-500/10' : ''}"
		style="padding-left: {level * 16 + 8}px"
		on:click={handleClick}
		on:contextmenu={handleContextMenu}
		draggable={!isRenaming}
		on:dragstart={handleDragStart}
		on:dragover={handleDragOver}
		on:dragleave={handleDragLeave}
		on:drop={handleDrop}
		role="treeitem"
		aria-selected={isSelected}
		aria-expanded={isExpanded}
		tabindex="0"
		on:keydown={(e) => {
			if (e.key === 'Enter') handleClick();
			if (e.key === 'ArrowRight' && !isExpanded) onToggleExpand(folder);
			if (e.key === 'ArrowLeft' && isExpanded) onToggleExpand(folder);
		}}
	>
		<!-- Expand/Collapse button -->
		<button
			type="button"
			class="w-5 h-5 flex items-center justify-center rounded hover:bg-base-300/50 transition-colors shrink-0
				{folder.children === undefined && !isLoading ? 'invisible' : ''}"
			on:click={handleToggle}
			tabindex="-1"
		>
			{#if isLoading}
				<Loader2 size={12} class="animate-spin text-base-content/40" />
			{:else}
				<ChevronRight 
					size={12} 
					class="text-base-content/40 transition-transform {isExpanded ? 'rotate-90' : ''}" 
				/>
			{/if}
		</button>

		<!-- Folder Icon -->
		<Folder 
			size={16} 
			class="shrink-0 {isSelected ? 'text-brand-500' : 'text-base-content/50'}"
		/>

		{#if isRenaming}
			<!-- Inline Rename Input -->
			<div class="flex items-center gap-1 flex-1 min-w-0">
				<input
					bind:this={renameInputRef}
					type="text"
					class="flex-1 min-w-0 px-1.5 py-0.5 text-xs bg-base-100 border border-brand-500 rounded focus:outline-none focus:ring-2 focus:ring-brand-500/20"
					value={renameValue}
					on:input={(e) => renameValue = e.currentTarget.value}
					on:keydown={(e) => onRenameKeydown(e, folder)}
					on:blur={() => onRenameConfirm(folder)}
					on:click|stopPropagation
				/>
			</div>
		{:else}
			<!-- Folder Name -->
			<span class="flex-1 truncate select-none" title={folder.name}>
				{folder.name}
			</span>
		{/if}
	</div>

	<!-- Children - properly recursive with all props -->
	{#if isExpanded && folder.children && folder.children.length > 0}
		<div class="mt-0.5" role="group">
			{#each folder.children as child (child.id)}
				<svelte:self
					folder={child}
					level={level + 1}
					{renamingFolderId}
					{renameValue}
					{renameInputRef}
					{draggedOverFolderId}
					{onSelect}
					{onToggleExpand}
					{onContextMenu}
					{onRenameConfirm}
					{onRenameCancel}
					{onRenameKeydown}
					{onDragOver}
					{onDragLeave}
					{onDrop}
				/>
			{/each}
		</div>
	{/if}
</div>
