<script lang="ts">
	import type { File as FileType, Folder } from '$lib/api/types';
	
	import type { SortField, SortOrder } from '$lib/stores/fileSort';
	import { selectionStore } from '$lib/stores/selection';
	import SortableTableHeader from '$lib/components/common/SortableTableHeader.svelte';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import FileListSkeleton from '$lib/components/common/FileListSkeleton.svelte';
	import { FolderOpen } from 'lucide-svelte';
	import FileListRow from './FileListRow.svelte';

	export let folders: Folder[] = [];
	export let files: FileType[] = [];
	export let isSharedRoot: boolean = false;
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

	export let activeSortField: SortField = 'name';
	export let activeSortOrder: SortOrder = 'asc';
	export let onSort: (field: SortField) => void = () => {};
	export let isLoading = false;

	// Drag and drop state
	let draggedItem: { id: string; isFolder: boolean; parentFolderId: string | null } | null = null;
	let dragOverFolderId: string | null = null;

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

	function handleSelectAll() {
		if (allSelected) {
			selectionStore.deselectAll();
		} else {
			selectionStore.selectAll(files, folders);
		}
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

	// Helper to determine if a folder is a valid drop target
	function isValidDropTarget(folder: Folder): boolean {
		if (!draggedItem) return false;

		// Can't drop onto itself
		if (draggedItem.id === folder.id) return false;

		// Can't drop a folder into itself or its children (would create cycle)
		if (draggedItem.isFolder && isDescendantOf(folder.id, draggedItem.id)) return false;

		// Can't drop into current parent (no-op)
		if (draggedItem.parentFolderId === folder.id) return false;

		return true;
	}

	$: allSelected =
		folders.length + files.length > 0 &&
		$selectionStore.selectedFolderIds.size + $selectionStore.selectedFileIds.size ===
			folders.length + files.length;
</script>

{#if isLoading}
	<FileListSkeleton />
{:else}
	<div class="relative overflow-x-auto rounded-xl border border-base-300 bg-base-100">
		<table class="w-full">
			<thead>
				<tr class="border-b border-base-300 bg-base-200/50">
					<th class="w-10 px-4 py-2 text-left">
						{#if selectionMode}
							<input
								type="checkbox"
								class="h-4 w-4 rounded border-base-300 bg-base-100 text-brand-500 focus:ring-brand-500"
								checked={allSelected}
								on:change={handleSelectAll}
							/>
						{/if}
					</th>
					<th
						class="w-12 px-2 py-2 text-left text-meta font-semibold tracking-wider text-base-content/60 uppercase"
						>Preview</th
					>
					<SortableTableHeader
						label="Name"
						field="name"
						activeField={activeSortField}
						activeOrder={activeSortOrder}
						{onSort}
					/>
					<SortableTableHeader
						label="Type"
						field="mime_type"
						activeField={activeSortField}
						activeOrder={activeSortOrder}
						{onSort}
						class="hidden md:table-cell"
					/>
					<SortableTableHeader
						label="Size"
						field="size"
						activeField={activeSortField}
						activeOrder={activeSortOrder}
						{onSort}
						class="hidden sm:table-cell"
					/>
					<SortableTableHeader
						label="Modified"
						field="modified_at"
						activeField={activeSortField}
						activeOrder={activeSortOrder}
						{onSort}
						class="hidden lg:table-cell"
					/>
					<th class="w-10 px-4 py-2"></th>
				</tr>
			</thead>
			<tbody class="divide-y divide-base-300/40">
				<!-- Folders -->
				{#each folders as folder (folder.id)}
					<FileListRow
						item={folder}
						isFolder={true}
						{isSharedRoot}
						{workspaceMode}
						{selectionMode}
						selected={$selectionStore.selectedFolderIds.has(folder.id)}
						isDragging={draggedItem?.id === folder.id}
						isDropTarget={dragOverFolderId === folder.id}
						canDrop={isValidDropTarget(folder)}
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
					<FileListRow
						item={file}
						isFolder={false}
						{workspaceMode}
						{selectionMode}
						selected={$selectionStore.selectedFileIds.has(file.id)}

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
						onEdit={() => {
							onEditFile(file);
						}}
						onDragStart={() =>
							handleDragStart({
								id: file.id,
								isFolder: false,
								parentFolderId: file.parent_folder_id
							})}
						onDragEnd={handleDragEnd}
					/>
				{/each}
			</tbody>
		</table>

		{#if folders.length === 0 && files.length === 0}
			<EmptyState
				icon={FolderOpen}
				title={emptyTitle}
				description={emptyDescription}
				actionLabel={emptyActionLabel ?? undefined}
				onAction={() => document.getElementById('upload-file-input')?.click()}
			/>
		{/if}
	</div>
{/if}
