<script lang="ts">
	import type { File as FileType, Folder } from '$lib/api/types';
	import type { ReplicationStatus } from '$lib/stores/replication';
	import { selectionStore } from '$lib/stores/selection';
	import FileListRow from './FileListRow.svelte';

	export let folders: Folder[] = [];
	export let files: FileType[] = [];
	export let emptyTitle = 'No files yet';
	export let emptyDescription = 'Upload your first file to get started';
	export let emptyActionLabel: string | null = 'Upload files';
	export let workspaceMode: 'all' | 'photos' | 'recent' | 'starred' | 'deleted' = 'all';
	export let onFolderClick: (folder: Folder) => void;
	export let onFileClick: (file: FileType) => void;
	export let onRenameFolder: (folder: Folder, newName: string) => void = () => {};
	export let onDeleteFolder: (folder: Folder) => void = () => {};
	export let onToggleFolderStar: (folder: Folder) => void = () => {};
	export let onRestoreFolder: (folder: Folder) => void = () => {};
	export let onPermanentDeleteFolder: (folder: Folder) => void = () => {};
	export let onShareFolder: (folder: Folder) => void = () => {};
	export let onMoveFolder: (folder: Folder, targetFolderId: string | null) => void = () => {};
	export let onRenameFile: (file: FileType, newName: string) => void = () => {};
	export let onDeleteFile: (file: FileType) => void = () => {};
	export let onToggleFileStar: (file: FileType) => void = () => {};
	export let onRestoreFile: (file: FileType) => void = () => {};
	export let onPermanentDeleteFile: (file: FileType) => void = () => {};
	export let onShareFile: (file: FileType) => void = () => {};
	export let onVersionHistory: (file: FileType) => void = () => {};
	export let onMoveFile: (file: FileType, targetFolderId: string | null) => void = () => {};
	export let onDownloadFile: (file: FileType) => void = () => {};
	export let onReplaceFile: (file: FileType) => void = () => {};
	export let onEditFile: (file: FileType) => void = () => {};
	export let selectionMode = false;
	export let replicationStatuses: Record<string, ReplicationStatus> = {};

	// Drag and drop state
	let draggedItem: { id: string; isFolder: boolean } | null = null;
	let dragOverFolderId: string | null = null;

	function handleFileToggle(file: FileType, event?: MouseEvent) {
		const isShiftKey = event?.shiftKey ?? false;
		const allFileIds = files.map(f => f.id);
		selectionStore.toggleFile(file.id, isShiftKey, allFileIds);
	}

	function handleFolderToggle(folder: Folder, event?: MouseEvent) {
		const isShiftKey = event?.shiftKey ?? false;
		const allFolderIds = folders.map(f => f.id);
		selectionStore.toggleFolder(folder.id, isShiftKey, allFolderIds);
	}

	function handleSelectAll() {
		selectionStore.selectAll(files, folders);
	}

	// Drag handlers
	function handleDragStart(item: { id: string; isFolder: boolean }) {
		draggedItem = item;
	}

	function handleDragEnd() {
		draggedItem = null;
		dragOverFolderId = null;
	}

	function handleDragOverFolder(folderId: string) {
		if (draggedItem && draggedItem.id !== folderId) {
			dragOverFolderId = folderId;
		}
	}

	function handleDragLeaveFolder() {
		dragOverFolderId = null;
	}

	function handleDropOnFolder(folder: Folder) {
		if (!draggedItem) return;
		
		// Can't drop a folder into itself or its children
		if (draggedItem.id === folder.id) {
			dragOverFolderId = null;
			draggedItem = null;
			return;
		}

		if (draggedItem.isFolder) {
			const draggedFolder = folders.find(f => f.id === draggedItem?.id);
			if (draggedFolder) {
				onMoveFolder(draggedFolder, folder.id);
			}
		} else {
			const draggedFile = files.find(f => f.id === draggedItem?.id);
			if (draggedFile) {
				onMoveFile(draggedFile, folder.id);
			}
		}
		
		dragOverFolderId = null;
		draggedItem = null;
	}

	$: allSelected = folders.length + files.length > 0 &&
		$selectionStore.selectedFolderIds.size + $selectionStore.selectedFileIds.size === folders.length + files.length;
</script>

<div class="relative overflow-x-auto rounded-xl border border-base-300 bg-base-100">
	<table class="w-full">
		<thead>
			<tr class="border-b border-base-300 bg-base-200/50">
				<th class="w-10 px-4 py-2 text-left">
					{#if selectionMode}
						<input
							type="checkbox"
							class="w-4 h-4 rounded border-base-300 text-brand-500 focus:ring-brand-500 bg-base-100"
							checked={allSelected}
							on:change={handleSelectAll}
						/>
					{/if}
				</th>
				<th class="w-12 px-2 py-2 text-left text-xs font-semibold text-base-content/60 uppercase tracking-wider">Preview</th>
				<th class="px-4 py-2 text-left text-xs font-semibold text-base-content/60 uppercase tracking-wider">Name</th>
				<th class="px-4 py-2 text-left text-xs font-semibold text-base-content/60 uppercase tracking-wider hidden md:table-cell">Type</th>
				<th class="px-4 py-2 text-left text-xs font-semibold text-base-content/60 uppercase tracking-wider hidden sm:table-cell">Size</th>
				<th class="px-4 py-2 text-left text-xs font-semibold text-base-content/60 uppercase tracking-wider hidden lg:table-cell">Modified</th>
				<th class="w-10 px-4 py-2"></th>
			</tr>
		</thead>
		<tbody class="divide-y divide-base-300/40">
			<!-- Folders -->
			{#each folders as folder (folder.id)}
				<FileListRow
					item={folder}
					isFolder={true}
					{workspaceMode}
					{selectionMode}
					selected={$selectionStore.selectedFolderIds.has(folder.id)}
					isDragging={draggedItem?.id === folder.id}
					isDropTarget={dragOverFolderId === folder.id}
					onSelect={(e) => handleFolderToggle(folder, e)}
					onToggleSelect={() => handleFolderToggle(folder)}
					onNavigate={() => onFolderClick(folder)}
					onRename={(newName) => onRenameFolder(folder, newName)}
					onDelete={() => onDeleteFolder(folder)}
					onToggleStar={() => onToggleFolderStar(folder)}
					onRestore={() => onRestoreFolder(folder)}
					onPermanentDelete={() => onPermanentDeleteFolder(folder)}
					onShare={() => onShareFolder(folder)}
					onMove={() => onMoveFolder(folder, null)}
					onDragStart={() => handleDragStart({ id: folder.id, isFolder: true })}
					onDragEnd={handleDragEnd}
					onDrop={() => handleDropOnFolder(folder)}
					onDragOver={() => handleDragOverFolder(folder.id)}
					onDragLeave={handleDragLeaveFolder}
				/>
			{/each}

			<!-- Files -->
			{#each files as file (file.id)}
				<FileListRow
					item={file}
					isFolder={false}
					{workspaceMode}
					{selectionMode}
					selected={$selectionStore.selectedFileIds.has(file.id)}
					replicationStatus={replicationStatuses[file.id]}
					isDragging={draggedItem?.id === file.id}
					onSelect={(e) => (selectionMode ? handleFileToggle(file, e) : onFileClick(file))}
					onToggleSelect={() => handleFileToggle(file)}
					onNavigate={() => {}}
					onRename={(newName) => onRenameFile(file, newName)}
					onDelete={() => onDeleteFile(file)}
					onToggleStar={() => onToggleFileStar(file)}
					onRestore={() => onRestoreFile(file)}
					onPermanentDelete={() => onPermanentDeleteFile(file)}
					onShare={() => onShareFile(file)}
					onMove={() => onMoveFile(file, null)}
					onDownload={() => onDownloadFile(file)}
					onVersionHistory={() => onVersionHistory(file)}
					onReplace={() => onReplaceFile(file)}
					onEdit={() => { console.log('[FileList] onEdit triggered for', file.name); onEditFile(file); }}
					onDragStart={() => handleDragStart({ id: file.id, isFolder: false })}
					onDragEnd={handleDragEnd}
				/>
			{/each}
		</tbody>
	</table>

	{#if folders.length === 0 && files.length === 0}
		<div class="flex flex-col items-center justify-center py-16 text-center">
			<div class="w-16 h-16 rounded-2xl bg-base-200 flex items-center justify-center mb-4">
				<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="w-8 h-8 text-base-content/30">
					<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>
				</svg>
			</div>
			<h3 class="text-lg font-semibold text-base-content mb-1">{emptyTitle}</h3>
			<p class="text-sm text-base-content/60 mb-4">{emptyDescription}</p>
			{#if emptyActionLabel}
				<button
					type="button"
					class="px-4 py-2 text-sm font-medium bg-brand-500 hover:bg-brand-600 text-white rounded-lg transition-colors"
					on:click={() => document.getElementById('upload-file-input')?.click()}
				>
					{emptyActionLabel}
				</button>
			{/if}
		</div>
	{/if}
</div>
