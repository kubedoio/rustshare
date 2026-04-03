<script lang="ts">
	import { onMount } from 'svelte';
	import { createQuery } from '@tanstack/svelte-query';
	import { Folder, Home, Loader2, Edit, Trash2, Share2, Move, Plus } from 'lucide-svelte';
	import { getFolderContents } from '$lib/api/folders';
	import { folderTreeStore, selectedFolder, type FolderNode } from '$lib/stores/folderTree';
	import FolderTreeItem from './FolderTreeItem.svelte';
	import ContextMenu from '$lib/components/common/ContextMenu.svelte';
	import type { MenuItem } from '$lib/components/common/ContextMenu.svelte';

	export let selectedFolderId: string | null = null;
	export let onSelectFolder: (folderId: string | null, folderPath: FolderNode[]) => void;
	export let onRenameFolder: (folder: FolderNode, newName: string) => void = () => {};
	export let onDeleteFolder: (folder: FolderNode) => void = () => {};
	export let onShareFolder: (folder: FolderNode) => void = () => {};
	export let onMoveFolder: (folder: FolderNode, targetFolderId: string | null) => void = () => {};
	export let onCreateSubfolder: (parentFolderId: string) => void = () => {};
	export let onMoveFile: (fileId: string, targetFolderId: string | null) => void = () => {};
	export let onMoveFolderDirect: (folderId: string, targetFolderId: string | null) => void = () => {};

	let folderPath: FolderNode[] = [];
	let contextMenuVisible = false;
	let contextMenuX = 0;
	let contextMenuY = 0;
	let contextFolder: FolderNode | null = null;

	// Query for root folders
	const rootFoldersQuery = createQuery({
		queryKey: ['folder-root-contents'],
		queryFn: () => getFolderContents(null)
	});

	$: if ($rootFoldersQuery.data?.folders) {
		folderTreeStore.setRootFolders($rootFoldersQuery.data.folders);
	}

	// Sync external selection
	$: if (selectedFolderId !== undefined && selectedFolderId !== $folderTreeStore.selectedId) {
		folderTreeStore.selectFolder(selectedFolderId);
	}

	async function handleSelect(folder: FolderNode) {
		folderTreeStore.selectFolder(folder.id);
		
		// Build folder path
		folderPath = buildFolderPath($folderTreeStore.rootFolders, folder.id);
		onSelectFolder(folder.id, folderPath);
	}

	async function handleToggleExpand(folder: FolderNode) {
		const willExpand = !$folderTreeStore.expandedIds.has(folder.id);
		folderTreeStore.toggleExpand(folder.id);

		if (willExpand && !folder.children) {
			folderTreeStore.setLoading(folder.id, true);
			try {
				const contents = await getFolderContents(folder.id);
				folderTreeStore.setFolderChildren(folder.id, contents.folders);
			} catch (error) {
				console.error('Failed to load folder contents:', error);
			} finally {
				folderTreeStore.setLoading(folder.id, false);
			}
		}
	}

	function handleSelectRoot() {
		folderTreeStore.selectFolder(null);
		folderPath = [];
		onSelectFolder(null, []);
	}

	function buildFolderPath(folders: FolderNode[], targetId: string): FolderNode[] {
		for (const folder of folders) {
			if (folder.id === targetId) {
				return [folder];
			}
			if (folder.children) {
				const path = buildFolderPath(folder.children, targetId);
				if (path.length > 0) {
					return [folder, ...path];
				}
			}
		}
		return [];
	}

	// Context menu
	function handleContextMenu(e: MouseEvent, folder: FolderNode) {
		e.preventDefault();
		contextMenuX = e.clientX;
		contextMenuY = e.clientY;
		contextFolder = folder;
		contextMenuVisible = true;
	}

	function handleRootContextMenu(e: MouseEvent) {
		e.preventDefault();
	}

	$: contextMenuItems = contextFolder ? buildContextMenu(contextFolder) : [];

	function buildContextMenu(folder: FolderNode): MenuItem[] {
		return [
			{ 
				id: 'open', 
				label: 'Open', 
				icon: Folder,
				onClick: () => handleSelect(folder)
			},
			{ id: 'sep1', label: '', separator: true, onClick: () => {} },
			{ 
				id: 'rename', 
				label: 'Rename', 
				icon: Edit,
				onClick: () => startInlineRename(folder)
			},
			{ 
				id: 'move', 
				label: 'Move to...', 
				icon: Move,
				onClick: () => onMoveFolder(folder, null)
			},
			{ 
				id: 'share', 
				label: 'Share', 
				icon: Share2,
				onClick: () => onShareFolder(folder)
			},
			{ id: 'sep2', label: '', separator: true, onClick: () => {} },
			{ 
				id: 'create-subfolder', 
				label: 'Create subfolder', 
				icon: Plus,
				onClick: () => onCreateSubfolder(folder.id)
			},
			{ id: 'sep3', label: '', separator: true, onClick: () => {} },
			{ 
				id: 'delete', 
				label: 'Move to trash', 
				icon: Trash2,
				danger: true,
				onClick: () => onDeleteFolder(folder)
			}
		];
	}

	// Inline rename state
	let renamingFolderId: string | null = null;
	let renameValue = '';
	let renameInputRef: HTMLInputElement;

	function startInlineRename(folder: FolderNode) {
		renamingFolderId = folder.id;
		renameValue = folder.name;
		setTimeout(() => {
			renameInputRef?.focus();
			renameInputRef?.select();
		}, 0);
	}

	function confirmRename(folder: FolderNode) {
		if (renameValue.trim() && renameValue !== folder.name) {
			onRenameFolder(folder, renameValue.trim());
		}
		renamingFolderId = null;
	}

	function cancelRename() {
		renamingFolderId = null;
		renameValue = '';
	}

	function handleRenameKeydown(e: KeyboardEvent, folder: FolderNode) {
		if (e.key === 'Enter') {
			confirmRename(folder);
		} else if (e.key === 'Escape') {
			cancelRename();
		}
	}

	// Drag and drop state
	let draggedOverFolderId: string | null = null;

	function handleDragOver(folderId: string) {
		draggedOverFolderId = folderId;
	}

	function handleDragLeave() {
		draggedOverFolderId = null;
	}

	function handleDrop(targetFolder: FolderNode) {
		// This will be called when something is dropped on a folder
		// The actual drop handling is done via the onDrop event on the item
		draggedOverFolderId = null;
	}

	function handleRootDrop(e: DragEvent) {
		e.preventDefault();
		const data = e.dataTransfer?.getData('application/json');
		if (!data) return;
		
		try {
			const { id, isFolder } = JSON.parse(data);
			if (isFolder) {
				onMoveFolderDirect(id, null);
			} else {
				onMoveFile(id, null);
			}
		} catch {
			// Ignore invalid drop data
		}
		draggedOverFolderId = null;
	}

	// Select root on mount if no selection
	onMount(() => {
		if (!$folderTreeStore.selectedId && !selectedFolderId) {
			handleSelectRoot();
		}
	});
</script>

<div class="h-full flex flex-col bg-base-100">
	<!-- Header -->
	<div class="px-3 py-3 border-b border-base-300 flex items-center justify-between">
		<h2 class="text-xs font-semibold text-base-content/50 uppercase tracking-wider px-2">Folders</h2>
	</div>

	<!-- Root Item -->
	<button
		type="button"
		class="flex items-center gap-2 px-3 py-2 mx-2 mt-2 rounded-md text-sm transition-colors
			{$folderTreeStore.selectedId === null 
				? 'bg-brand-500/15 text-brand-600 font-medium' 
				: 'text-base-content/70 hover:bg-base-200/50 hover:text-base-content'}
			{draggedOverFolderId === 'root' ? 'ring-2 ring-brand-500/50 bg-brand-500/10' : ''}"
		on:click={handleSelectRoot}
		on:contextmenu={handleRootContextMenu}
		on:dragover|preventDefault={() => handleDragOver('root')}
		on:dragleave={handleDragLeave}
		on:drop={handleRootDrop}
	>
		<Home size={18} class={$folderTreeStore.selectedId === null ? 'text-brand-500' : 'text-base-content/50'} />
		<span>My Files</span>
	</button>

	<!-- Tree -->
	<div class="flex-1 overflow-y-auto py-2" role="tree" aria-label="Folder tree">
		{#if $rootFoldersQuery.isLoading}
			<div class="px-4 py-4 flex items-center justify-center">
				<Loader2 size={20} class="animate-spin text-base-content/30" />
			</div>
		{:else if $rootFoldersQuery.isError}
			<div class="px-4 py-4 text-sm text-error">
				Failed to load folders
			</div>
		{:else if $folderTreeStore.rootFolders.length === 0}
			<div class="px-4 py-8 text-center">
				<div class="w-12 h-12 rounded-xl bg-base-200 flex items-center justify-center mx-auto mb-3">
					<Folder size={24} class="text-base-content/30" />
				</div>
				<p class="text-sm text-base-content/50">No folders yet</p>
				<p class="text-xs text-base-content/40 mt-1">Create folders to organize your files</p>
			</div>
		{:else}
			{#each $folderTreeStore.rootFolders as folder (folder.id)}
				<FolderTreeItem
					{folder}
					level={0}
					{renamingFolderId}
					{renameValue}
					{renameInputRef}
					{draggedOverFolderId}
					onSelect={handleSelect}
					onToggleExpand={handleToggleExpand}
					onContextMenu={handleContextMenu}
					onRenameConfirm={confirmRename}
					onRenameCancel={cancelRename}
					onRenameKeydown={handleRenameKeydown}
					onDragOver={handleDragOver}
					onDragLeave={handleDragLeave}
					onDrop={handleDrop}
				/>
			{/each}
		{/if}
	</div>
</div>

<!-- Context Menu -->
<ContextMenu
	items={contextMenuItems}
	x={contextMenuX}
	y={contextMenuY}
	visible={contextMenuVisible}
	onClose={() => { contextMenuVisible = false; contextFolder = null; }}
/>
