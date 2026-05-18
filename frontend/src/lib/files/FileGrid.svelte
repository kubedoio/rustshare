<script lang="ts">
	import type { File as FileType, Folder } from '$lib/api/types';
	
	import { selectionStore } from '$lib/stores/selection';
	import FileGridTile from './FileGridTile.svelte';

	interface Props {
		folders?: Folder[];
		files?: FileType[];
		emptyTitle?: string;
		emptyDescription?: string;
		emptyActionLabel?: string | null;
		workspaceMode?: 'all' | 'photos' | 'recent' | 'starred' | 'deleted' | 'week';
		onFolderClick: (folder: Folder) => void;
		onFileClick: (file: FileType) => void;
		onRenameFolder?: (folder: Folder, newName: string) => void;
		onDeleteFolder?: (folder: Folder) => void;
		onToggleFolderStar?: (folder: Folder) => void;
		onRestoreFolder?: (folder: Folder) => void;
		onPermanentDeleteFolder?: (folder: Folder) => void;
		onShareFolder?: (folder: Folder) => void;
		onMoveFolder?: (folder: Folder, targetFolderId: string | null) => void;
		onRenameFile?: (file: FileType, newName: string) => void;
		onDeleteFile?: (file: FileType) => void;
		onToggleFileStar?: (file: FileType) => void;
		onRestoreFile?: (file: FileType) => void;
		onPermanentDeleteFile?: (file: FileType) => void;
		onShareFile?: (file: FileType) => void;
		onVersionHistory?: (file: FileType) => void;
		onMoveFile?: (file: FileType, targetFolderId: string | null) => void;
		onDownloadFile?: (file: FileType) => void;
		onReplaceFile?: (file: FileType) => void;
		onEditFile?: (file: FileType) => void;
		selectionMode?: boolean;
		isSharedRoot?: boolean;
	}

	let {
		folders = [],
		files = [],
		emptyTitle = 'This folder is empty',
		emptyDescription = 'Upload files or create folders to get started',
		emptyActionLabel = 'Upload files',
		workspaceMode = 'all',
		onFolderClick,
		onFileClick,
		onRenameFolder = () => {},
		onDeleteFolder = () => {},
		onToggleFolderStar = () => {},
		onRestoreFolder = () => {},
		onPermanentDeleteFolder = () => {},
		onShareFolder = () => {},
		onMoveFolder = () => {},
		onRenameFile = () => {},
		onDeleteFile = () => {},
		onToggleFileStar = () => {},
		onRestoreFile = () => {},
		onPermanentDeleteFile = () => {},
		onShareFile = () => {},
		onVersionHistory = () => {},
		onMoveFile = () => {},
		onDownloadFile = () => {},
		onReplaceFile = () => {},
		onEditFile = () => {},
		selectionMode = false,
		isSharedRoot = false
	}: Props = $props();

	// Drag and drop state
	let draggedItem = $state<{ id: string; isFolder: boolean; parentFolderId: string | null } | null>(null);
	let dragOverFolderId = $state<string | null>(null);

	function handleFileToggle(file: FileType, event?: MouseEvent) {
		const isShiftKey = event?.shiftKey ?? false;
		const allFileIds = files.map((f) => f.id);
		selectionStore.toggleFile(file.id, isShiftKey, allFileIds);
	}

	function handleFolderToggle(folder: Folder, event?: MouseEvent) {
		const isShiftKey = event?.shiftKey ?? false;
		const allFolderIds = folders.map((f) => f.id);
		selectionStore.toggleFolder(folder.id, isShiftKey, allFolderIds);
	}

	// Drag handlers
	function handleDragStart(item: { id: string; isFolder: boolean; parentFolderId: string | null }) {
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

	// Check if target folder is a descendant of the dragged folder (would create cycle)
	function isDescendantOf(folderId: string, potentialParentId: string): boolean {
		const folder = folders.find((f) => f.id === folderId);
		if (!folder) return false;
		if (folder.parent_folder_id === potentialParentId) return true;
		if (!folder.parent_folder_id) return false;
		return isDescendantOf(folder.parent_folder_id, potentialParentId);
	}

	function readDragPayload(
		e: DragEvent
	): { id: string; isFolder: boolean; parentFolderId: string | null } | null {
		try {
			const data = e.dataTransfer?.getData('application/json');
			if (data) {
				const parsed = JSON.parse(data);
				if (parsed && typeof parsed.id === 'string') {
					return {
						id: parsed.id,
						isFolder: !!parsed.isFolder,
						parentFolderId: parsed.parentFolderId || null
					};
				}
			}
		} catch {
			// ignore
		}
		return null;
	}

	function handleDropOnFolder(folder: Folder, e?: DragEvent) {
		let payload = draggedItem;
		if (!payload && e) {
			payload = readDragPayload(e);
		}
		if (!payload) {
			dragOverFolderId = null;
			draggedItem = null;
			return;
		}

		// Can't drop onto itself
		if (payload.id === folder.id) {
			dragOverFolderId = null;
			draggedItem = null;
			return;
		}

		// Can't drop a folder into itself or its children (would create cycle)
		if (payload.isFolder) {
			if (isDescendantOf(folder.id, payload.id)) {
				dragOverFolderId = null;
				draggedItem = null;
				return;
			}
		}

		// Can't drop file/folder into its current parent (no-op)
		if (payload.parentFolderId === folder.id) {
			dragOverFolderId = null;
			draggedItem = null;
			return;
		}

		if (payload.isFolder) {
			const draggedFolder = folders.find((f) => f.id === payload?.id);
			if (draggedFolder) {
				onMoveFolder(draggedFolder, folder.id);
			}
		} else {
			const draggedFile = files.find((f) => f.id === payload?.id);
			if (draggedFile) {
				onMoveFile(draggedFile, folder.id);
			}
		}

		dragOverFolderId = null;
		draggedItem = null;
	}

	// Handle rename with new signature
	function handleRenameFolder(folder: Folder, newName: string) {
		onRenameFolder(folder, newName);
	}

	function handleRenameFile(file: FileType, newName: string) {
		onRenameFile(file, newName);
	}
</script>

{#if folders.length === 0 && files.length === 0}
	<div class="flex flex-col items-center justify-center px-4 py-16 text-center">
		<div class="mb-4 flex h-16 w-16 items-center justify-center rounded-2xl bg-base-200">
			<svg
				xmlns="http://www.w3.org/2000/svg"
				viewBox="0 0 24 24"
				fill="none"
				stroke="currentColor"
				stroke-width="1.5"
				class="h-8 w-8 text-base-content/30"
			>
				<path
					d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"
				/>
			</svg>
		</div>
		<h3 class="mb-1 text-lg font-semibold text-base-content">{emptyTitle}</h3>
		<p class="mb-4 text-sm text-base-content/60">{emptyDescription}</p>
		{#if emptyActionLabel}
			<button
				type="button"
				class="rounded-lg bg-brand-500 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-600"
				on:click={() => document.getElementById('upload-file-input')?.click()}
			>
				{emptyActionLabel}
			</button>
		{/if}
	</div>
{:else}
	<!-- Responsive grid: 1 col mobile, 2 cols sm, auto-fill on larger -->
	<div
		class="grid grid-cols-1 gap-2 sm:grid-cols-2 sm:gap-3 lg:grid-cols-[repeat(auto-fill,minmax(12rem,1fr))] xl:grid-cols-[repeat(auto-fill,minmax(13rem,1fr))]"
	>
		<!-- Folders -->
		{#each folders as folder (folder.id)}
			<FileGridTile
				item={folder}
				isFolder={true}
				{isSharedRoot}
				{workspaceMode}
				selected={selectionMode && $selectionStore.selectedFolderIds.has(folder.id)}
				{selectionMode}
				isDragging={draggedItem?.id === folder.id}
				isDropTarget={dragOverFolderId === folder.id}
				onSelect={() => onFolderClick(folder)}
				onToggle={(e) => handleFolderToggle(folder, e)}
				onRename={(newName) => handleRenameFolder(folder, newName)}
				onDelete={() => onDeleteFolder(folder)}
				onToggleStar={() => onToggleFolderStar(folder)}
				onRestore={() => onRestoreFolder(folder)}
				onPermanentDelete={() => onPermanentDeleteFolder(folder)}
				onShare={() => onShareFolder(folder)}
				onMove={() => onMoveFolder(folder, null)}
				onDragStart={() =>
					handleDragStart({
						id: folder.id,
						isFolder: true,
						parentFolderId: folder.parent_folder_id
					})}
				onDragEnd={handleDragEnd}
				onDrop={(e) => handleDropOnFolder(folder, e)}
				onDragOver={() => handleDragOverFolder(folder.id)}
				onDragLeave={handleDragLeaveFolder}
			/>
		{/each}

		<!-- Files -->
		{#each files as file (file.id)}
			<FileGridTile
				item={file}
				isFolder={false}
				{isSharedRoot}
				{workspaceMode}
				selected={selectionMode && $selectionStore.selectedFileIds.has(file.id)}
				{selectionMode}

				isDragging={draggedItem?.id === file.id}
				onSelect={() => onFileClick(file)}
				onToggle={(e) => handleFileToggle(file, e)}
				onRename={(newName) => handleRenameFile(file, newName)}
				onDelete={() => onDeleteFile(file)}
				onToggleStar={() => onToggleFileStar(file)}
				onRestore={() => onRestoreFile(file)}
				onPermanentDelete={() => onPermanentDeleteFile(file)}
				onShare={() => onShareFile(file)}
				onMove={() => onMoveFile(file, null)}
				onDownload={() => onDownloadFile(file)}
				onVersionHistory={() => onVersionHistory(file)}
				onReplace={() => onReplaceFile(file)}
				onEdit={() => {
					onEditFile(file);
				}}
				onDragStart={() =>
					handleDragStart({ id: file.id, isFolder: false, parentFolderId: file.parent_folder_id })}
				onDragEnd={handleDragEnd}
			/>
		{/each}
	</div>
{/if}
