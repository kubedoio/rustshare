<script lang="ts">
	import { onMount } from 'svelte';
	import { createQuery, createMutation } from '@tanstack/svelte-query';
	import { listAllFiles, downloadFile, uploadFile, renameFile, deleteFile, moveFile } from '$lib/api/files';
	import { getFolderContents, createFolder, renameFolder, deleteFolder, moveFolder } from '$lib/api/folders';
	import { queryClient } from '$lib/query-client';
	import { searchQuery } from '$lib/stores/search';
	import { fileSortState } from '$lib/stores/fileSort';
	import { selectionStore, selectionCount, hasSelection } from '$lib/stores/selection';
	import { activityStore } from '$lib/stores/activity';
	import { replicationStore, type ReplicationStatus } from '$lib/stores/replication';
	import type { File, Folder } from '$lib/api/types';
	import type { WebSocketEvent } from '$lib/websocket/events';
	import type { FolderNode } from '$lib/stores/folderTree';

	// Components
	import FileExplorer from '$lib/files/FileExplorer.svelte';
	import DropZone from '$lib/components/files/DropZone.svelte';
	import UploadProgress from '$lib/components/files/UploadProgress.svelte';
	import Toast from '$lib/components/common/Toast.svelte';

	// Modals
	import RenameModal from '$lib/components/modals/RenameModal.svelte';
	import DeleteConfirmation from '$lib/components/modals/DeleteConfirmation.svelte';
	import MoveModal from '$lib/components/modals/MoveModal.svelte';
	import ShareModal from '$lib/components/modals/ShareModal.svelte';
	import CreateFolderModal from '$lib/components/modals/CreateFolderModal.svelte';
	import VersionHistoryModal from '$lib/components/modals/VersionHistoryModal.svelte';
	import FilePreviewModal from '$lib/components/modals/FilePreviewModal.svelte';
	import ReplaceFileModal from '$lib/components/modals/ReplaceFileModal.svelte';

	type UploadTask = {
		id: string;
		fileName: string;
		size: number;
		status: 'pending' | 'uploading' | 'success' | 'error';
		progress: number;
		error?: string;
		previewUrl?: string;
	};

	let uploadTasks: UploadTask[] = [];
	let selectionMode = false;
	let currentFolderId: string | null = null;
	let folderPath: Folder[] = [];

	// Modal states
	let showRenameModal = false;
	let showDeleteModal = false;
	let showMoveModal = false;
	let showShareModal = false;
	let showCreateFolderModal = false;
	let showVersionHistoryModal = false;
	let showFilePreviewModal = false;
	let showReplaceFileModal = false;

	// Targets
	let renameTarget: File | Folder | null = null;
	let renameType: 'file' | 'folder' = 'file';
	let deleteTarget: File | Folder | null = null;
	let deleteType: 'file' | 'folder' = 'file';
	let moveTarget: File | Folder | null = null;
	let moveType: 'file' | 'folder' = 'file';
	let shareTarget: File | Folder | null = null;
	let shareType: 'file' | 'folder' = 'file';
	let versionHistoryTarget: File | null = null;
	let previewTarget: File | null = null;
	let replaceFileTarget: File | null = null;

	// Toast
	let showToast = false;
	let toastMessage = '';
	let toastType: 'success' | 'error' | 'info' = 'info';

	// Query for folder contents - use derived store pattern for Svelte 5
	const filesQuery = createQuery({
		queryKey: ['folder-contents', currentFolderId],
		queryFn: () => getFolderContents(currentFolderId)
	});

	// Mutations
	const uploadMutation = createMutation({
		mutationFn: (file: globalThis.File) => uploadFile(currentFolderId, file),
		onSuccess: (_, file) => {
			queryClient.invalidateQueries({ queryKey: ['folder-contents', currentFolderId] });
			queryClient.invalidateQueries({ queryKey: ['folder-root-contents'] });
			activityStore.addActivity('file_uploaded', file.name);
		}
	});

	const createFolderMutation = createMutation({
		mutationFn: (name: string) => createFolder(name, currentFolderId),
		onSuccess: (folder) => {
			queryClient.invalidateQueries({ queryKey: ['folder-contents', currentFolderId] });
			queryClient.invalidateQueries({ queryKey: ['folder-root-contents'] });
			showCreateFolderModal = false;
			showNotification('Folder created', 'success');
			activityStore.addActivity('folder_created', folder.name);
		},
		onError: (error) => {
			showNotification(error instanceof Error ? error.message : 'Failed to create folder', 'error');
		}
	});

	const renameFileMutation = createMutation({
		mutationFn: ({ fileId, newName }: { fileId: string; newName: string }) => renameFile(fileId, newName),
		onSuccess: (_, { newName }) => {
			const oldName = renameTarget?.name || 'File';
			queryClient.invalidateQueries({ queryKey: ['folder-contents', currentFolderId] });
			showRenameModal = false;
			renameTarget = null;
			showNotification('File renamed', 'success');
			activityStore.addActivity('file_renamed', newName, oldName);
		}
	});

	const renameFolderMutation = createMutation({
		mutationFn: ({ folderId, newName }: { folderId: string; newName: string }) => renameFolder(folderId, newName),
		onSuccess: (_, { newName }) => {
			const oldName = renameTarget?.name || 'Folder';
			queryClient.invalidateQueries({ queryKey: ['folder-contents'] });
			queryClient.invalidateQueries({ queryKey: ['folder-root-contents'] });
			showRenameModal = false;
			renameTarget = null;
			showNotification('Folder renamed', 'success');
			activityStore.addActivity('folder_renamed', newName, oldName);
		}
	});

	const deleteFileMutation = createMutation({
		mutationFn: (fileId: string) => deleteFile(fileId),
		onSuccess: (_, fileId) => {
			const fileName = deleteTarget?.name || 'File';
			queryClient.invalidateQueries({ queryKey: ['folder-contents', currentFolderId] });
			showDeleteModal = false;
			deleteTarget = null;
			showNotification('File deleted', 'success');
			activityStore.addActivity('file_deleted', fileName);
		}
	});

	const deleteFolderMutation = createMutation({
		mutationFn: (folderId: string) => deleteFolder(folderId),
		onSuccess: () => {
			const folderName = deleteTarget?.name || 'Folder';
			queryClient.invalidateQueries({ queryKey: ['folder-contents'] });
			queryClient.invalidateQueries({ queryKey: ['folder-root-contents'] });
			showDeleteModal = false;
			deleteTarget = null;
			showNotification('Folder deleted', 'success');
			activityStore.addActivity('folder_deleted', folderName);
		}
	});

	const moveFileMutation = createMutation({
		mutationFn: ({ fileId, targetFolderId }: { fileId: string; targetFolderId: string | null }) => moveFile(fileId, targetFolderId),
		onSuccess: () => {
			const fileName = moveTarget?.name || 'File';
			queryClient.invalidateQueries({ queryKey: ['folder-contents'] });
			queryClient.invalidateQueries({ queryKey: ['folder-root-contents'] });
			showMoveModal = false;
			moveTarget = null;
			showNotification('File moved', 'success');
			activityStore.addActivity('file_moved', fileName);
		}
	});

	const moveFolderMutation = createMutation({
		mutationFn: ({ folderId, targetFolderId }: { folderId: string; targetFolderId: string | null }) => moveFolder(folderId, targetFolderId),
		onSuccess: () => {
			const folderName = moveTarget?.name || 'Folder';
			queryClient.invalidateQueries({ queryKey: ['folder-contents'] });
			queryClient.invalidateQueries({ queryKey: ['folder-root-contents'] });
			showMoveModal = false;
			moveTarget = null;
			showNotification('Folder moved', 'success');
			activityStore.addActivity('folder_moved', folderName);
		}
	});

	// Filter and sort
	$: filteredFolders = $searchQuery
		? ($filesQuery.data?.folders || []).filter(f => f.name.toLowerCase().includes($searchQuery.toLowerCase()))
		: $filesQuery.data?.folders || [];

	$: filteredFiles = $searchQuery
		? ($filesQuery.data?.files || []).filter(f => f.name.toLowerCase().includes($searchQuery.toLowerCase()))
		: $filesQuery.data?.files || [];

	$: sortedFolders = [...filteredFolders].sort((a, b) => {
		if ($fileSortState.field === 'name') {
			return $fileSortState.order === 'asc' ? a.name.localeCompare(b.name) : b.name.localeCompare(a.name);
		}
		if ($fileSortState.field === 'modified_at') {
			return $fileSortState.order === 'asc' 
				? new Date(a.updated_at).getTime() - new Date(b.updated_at).getTime()
				: new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime();
		}
		return 0;
	});

	$: sortedFiles = [...filteredFiles].sort((a, b) => {
		if ($fileSortState.field === 'name') {
			return $fileSortState.order === 'asc' ? a.name.localeCompare(b.name) : b.name.localeCompare(a.name);
		}
		if ($fileSortState.field === 'modified_at') {
			return $fileSortState.order === 'asc'
				? new Date(a.modified_at).getTime() - new Date(b.modified_at).getTime()
				: new Date(b.modified_at).getTime() - new Date(a.modified_at).getTime();
		}
		if ($fileSortState.field === 'size') {
			return $fileSortState.order === 'asc' ? a.size - b.size : b.size - a.size;
		}
		if ($fileSortState.field === 'mime_type') {
			return $fileSortState.order === 'asc' ? a.mime_type.localeCompare(b.mime_type) : b.mime_type.localeCompare(a.mime_type);
		}
		return 0;
	});

	// Handlers
	function handleFolderSelect(folderId: string | null, path: Folder[]) {
		currentFolderId = folderId;
		folderPath = path;
	}

	function handleFolderClick(folder: Folder) {
		currentFolderId = folder.id;
		folderPath = [...folderPath, folder];
	}

	function handleBreadcrumbNavigate(event: CustomEvent<{ folderId: string | null }>) {
		const targetId = event.detail.folderId;
		if (targetId === null) {
			currentFolderId = null;
			folderPath = [];
		} else {
			const index = folderPath.findIndex(f => f.id === targetId);
			if (index !== -1) {
				currentFolderId = targetId;
				folderPath = folderPath.slice(0, index + 1);
			}
		}
	}

	function handleFileClick(file: File) {
		previewTarget = file;
		showFilePreviewModal = true;
	}

	function showNotification(message: string, type: 'success' | 'error' | 'info') {
		toastMessage = message;
		toastType = type;
		showToast = true;
	}

	async function handleFilesSelected(files: globalThis.File[]) {
		if (files.length === 0) return;

		const newTasks: UploadTask[] = files.map(file => ({
			id: `${file.name}-${Date.now()}-${Math.random()}`,
			fileName: file.name,
			size: file.size,
			status: 'pending' as const,
			progress: 0
		}));

		uploadTasks = [...uploadTasks, ...newTasks];

		for (let i = 0; i < files.length; i++) {
			const taskIndex = uploadTasks.findIndex(t => t.id === newTasks[i].id);
			if (taskIndex === -1) continue;

			uploadTasks[taskIndex] = { ...uploadTasks[taskIndex], status: 'uploading', progress: 50 };
			uploadTasks = [...uploadTasks];

			try {
				await $uploadMutation.mutateAsync(files[i]);
				uploadTasks[taskIndex] = { ...uploadTasks[taskIndex], status: 'success', progress: 100 };
				uploadTasks = [...uploadTasks];
			} catch (error) {
				const errorMessage = error instanceof Error ? error.message : 'Upload failed';
				uploadTasks[taskIndex] = { ...uploadTasks[taskIndex], status: 'error', error: errorMessage };
				uploadTasks = [...uploadTasks];
			}
		}

		const successCount = uploadTasks.filter(t => t.status === 'success').length;
		const errorCount = uploadTasks.filter(t => t.status === 'error').length;

		if (errorCount === 0) {
			showNotification(`Uploaded ${successCount} file(s)`, 'success');
		} else if (successCount === 0) {
			showNotification(`Failed to upload ${errorCount} file(s)`, 'error');
		} else {
			showNotification(`Uploaded ${successCount}, failed ${errorCount}`, 'info');
		}
	}

	function handleCloseProgress() {
		uploadTasks = [];
	}

	function toggleSelectionMode() {
		selectionMode = !selectionMode;
		if (!selectionMode) {
			selectionStore.clear();
		}
	}

	function handleSelectAll() {
		selectionStore.selectAll(sortedFiles, sortedFolders);
	}

	function handleDeselectAll() {
		selectionStore.deselectAll();
	}

	async function handleBulkDelete() {
		if (!$hasSelection) return;
		const fileIds = Array.from($selectionStore.selectedFileIds);
		const folderIds = Array.from($selectionStore.selectedFolderIds);

		if (!confirm(`Delete ${fileIds.length} file(s) and ${folderIds.length} folder(s)?`)) return;

		try {
			for (const fileId of fileIds) await deleteFile(fileId);
			for (const folderId of folderIds) await deleteFolder(folderId);
			selectionStore.clear();
			selectionMode = false;
			queryClient.invalidateQueries({ queryKey: ['folder-contents'] });
			queryClient.invalidateQueries({ queryKey: ['folder-root-contents'] });
			showNotification(`Deleted ${fileIds.length + folderIds.length} item(s)`, 'success');
		} catch (error) {
			showNotification('Failed to delete some items', 'error');
		}
	}

	// Rename handlers
	function handleRenameFile(file: File) {
		renameTarget = file;
		renameType = 'file';
		showRenameModal = true;
	}

	function handleRenameFolder(folder: Folder) {
		renameTarget = folder;
		renameType = 'folder';
		showRenameModal = true;
	}

	function handleRenameConfirm(event: CustomEvent<{ newName: string }>) {
		if (!renameTarget) return;
		if (renameType === 'file') {
			$renameFileMutation.mutate({ fileId: renameTarget.id, newName: event.detail.newName });
		} else {
			$renameFolderMutation.mutate({ folderId: renameTarget.id, newName: event.detail.newName });
		}
	}

	// Delete handlers
	function handleDeleteFile(file: File) {
		deleteTarget = file;
		deleteType = 'file';
		showDeleteModal = true;
	}

	function handleDeleteFolder(folder: Folder) {
		deleteTarget = folder;
		deleteType = 'folder';
		showDeleteModal = true;
	}

	function handleDeleteConfirm() {
		if (!deleteTarget) return;
		if (deleteType === 'file') {
			$deleteFileMutation.mutate(deleteTarget.id);
		} else {
			$deleteFolderMutation.mutate(deleteTarget.id);
		}
	}

	// Move handlers
	function handleMoveFile(file: File) {
		moveTarget = file;
		moveType = 'file';
		showMoveModal = true;
	}

	function handleMoveFolder(folder: Folder) {
		moveTarget = folder;
		moveType = 'folder';
		showMoveModal = true;
	}

	function handleMoveConfirm(event: CustomEvent<{ targetFolderId: string | null }>) {
		if (!moveTarget) return;
		if (moveType === 'file') {
			$moveFileMutation.mutate({ fileId: moveTarget.id, targetFolderId: event.detail.targetFolderId });
		} else {
			$moveFolderMutation.mutate({ folderId: moveTarget.id, targetFolderId: event.detail.targetFolderId });
		}
	}

	// Share handlers
	function handleShareFile(file: File) {
		shareTarget = file;
		shareType = 'file';
		showShareModal = true;
	}

	function handleShareFolder(folder: Folder) {
		shareTarget = folder;
		shareType = 'folder';
		showShareModal = true;
	}

	function handleShareNotification(e: CustomEvent<{ message: string; type: 'success' | 'error' | 'info' }>) {
		showNotification(e.detail.message, e.detail.type);
	}

	// Other handlers
	function handleVersionHistory(file: File) {
		versionHistoryTarget = file;
		showVersionHistoryModal = true;
	}

	async function handleDownloadFile(file: File) {
		try {
			const response = await downloadFile(file.id);
			let downloadUrl = response.url;
			if (downloadUrl.includes('/rustshare-files/')) {
				const path = downloadUrl.split('/rustshare-files/')[1];
				downloadUrl = `/storage/${path}`;
			}
			window.open(downloadUrl, '_blank');
			showNotification('Download started', 'success');
			activityStore.addActivity('file_downloaded', file.name);
		} catch (error) {
			showNotification(error instanceof Error ? error.message : 'Failed to download', 'error');
		}
	}

	function handleReplaceFile(file: File) {
		replaceFileTarget = file;
		showReplaceFileModal = true;
	}

	function handleReplaceSuccess() {
		queryClient.invalidateQueries({ queryKey: ['folder-contents', currentFolderId] });
		const fileName = replaceFileTarget?.name || 'File';
		showNotification(`${fileName} was modified`, 'success');
		if (replaceFileTarget) {
			activityStore.addActivity('file_modified', replaceFileTarget.name);
		}
		showReplaceFileModal = false;
		replaceFileTarget = null;
	}

	function handleCreateFolder(event: CustomEvent<{ name: string }>) {
		$createFolderMutation.mutate(event.detail.name);
	}

	function handleVersionRestored() {
		queryClient.invalidateQueries({ queryKey: ['folder-contents', currentFolderId] });
		showNotification('File version restored', 'success');
	}

	// Keyboard shortcuts
	function handleKeyDown(event: KeyboardEvent) {
		if (event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement) return;

		if (event.key === 'a' && (event.ctrlKey || event.metaKey)) {
			event.preventDefault();
			if (!selectionMode) selectionMode = true;
			handleSelectAll();
			return;
		}

		if (event.key === 'Escape') {
			if (selectionMode) {
				event.preventDefault();
				selectionStore.clear();
				selectionMode = false;
				return;
			}
			showRenameModal = false;
			showDeleteModal = false;
			showMoveModal = false;
			showShareModal = false;
			showCreateFolderModal = false;
			showVersionHistoryModal = false;
			showFilePreviewModal = false;
			showReplaceFileModal = false;
			return;
		}

		switch (event.key.toLowerCase()) {
			case 'u':
				event.preventDefault();
				document.getElementById('upload-file-input')?.click();
				break;
			case 'n':
				event.preventDefault();
				showCreateFolderModal = true;
				break;
		}
	}

	$: isUploading = uploadTasks.some(t => t.status === 'uploading' || t.status === 'pending');
	$: moveCurrentFolderId = moveType === 'file' 
		? (moveTarget as File | null)?.parent_folder_id 
		: (moveTarget as Folder | null)?.parent_folder_id;
	$: replicationStatuses = $replicationStore;
</script>

<svelte:head>
	<title>Files - RustShare</title>
</svelte:head>

<svelte:window on:keydown={handleKeyDown} />

<!-- Hidden file input for upload button -->
<input
	id="upload-file-input"
	type="file"
	class="hidden"
	multiple
	on:change={(e) => {
		const target = e.target as HTMLInputElement;
		if (target.files && target.files.length > 0) {
			handleFilesSelected(Array.from(target.files));
			target.value = '';
		}
	}}
/>

<DropZone on:filesDropped={(e) => handleFilesSelected(e.detail)} disabled={isUploading}>
	<FileExplorer
		folders={sortedFolders}
		files={sortedFiles}
		{currentFolderId}
		{folderPath}
		isLoading={$filesQuery.isLoading}
		error={$filesQuery.error}
		{replicationStatuses}
		{selectionMode}
		{isUploading}
		onFolderSelect={handleFolderSelect}
		onFolderClick={handleFolderClick}
		onFileClick={handleFileClick}
		onRefresh={() => $filesQuery.refetch()}
		onNewFolder={() => showCreateFolderModal = true}
		onUpload={() => document.getElementById('upload-file-input')?.click()}
		onToggleSelection={toggleSelectionMode}
		onSelectAll={handleSelectAll}
		onDeselectAll={handleDeselectAll}
		onBulkDelete={handleBulkDelete}
		onRenameFile={handleRenameFile}
		onDeleteFile={handleDeleteFile}
		onShareFile={handleShareFile}
		onVersionHistory={handleVersionHistory}
		onMoveFile={handleMoveFile}
		onDownloadFile={handleDownloadFile}
		onReplaceFile={handleReplaceFile}
		onRenameFolder={handleRenameFolder}
		onDeleteFolder={handleDeleteFolder}
		onShareFolder={handleShareFolder}
		onMoveFolder={handleMoveFolder}
		on:breadcrumbNavigate={handleBreadcrumbNavigate}
	/>
</DropZone>

<!-- Upload Progress -->
<UploadProgress tasks={uploadTasks} onClose={handleCloseProgress} />

<!-- Modals -->
<RenameModal
	open={showRenameModal}
	loading={$renameFileMutation.isPending || $renameFolderMutation.isPending}
	itemName={renameTarget?.name || ''}
	itemType={renameType}
	on:close={() => { showRenameModal = false; renameTarget = null; }}
	on:confirm={handleRenameConfirm}
/>

<DeleteConfirmation
	open={showDeleteModal}
	loading={$deleteFileMutation.isPending || $deleteFolderMutation.isPending}
	itemName={deleteTarget?.name || ''}
	itemType={deleteType}
	on:close={() => { showDeleteModal = false; deleteTarget = null; }}
	on:confirm={handleDeleteConfirm}
/>

<MoveModal
	open={showMoveModal}
	loading={$moveFileMutation.isPending || $moveFolderMutation.isPending}
	itemName={moveTarget?.name || ''}
	itemType={moveType}
	itemId={moveTarget?.id || null}
	currentFolderId={moveCurrentFolderId}
	on:close={() => { showMoveModal = false; moveTarget = null; }}
	on:confirm={handleMoveConfirm}
/>

<CreateFolderModal
	open={showCreateFolderModal}
	loading={$createFolderMutation.isPending}
	on:close={() => showCreateFolderModal = false}
	on:confirm={handleCreateFolder}
/>

<ShareModal
	open={showShareModal}
	resourceId={shareTarget?.id || ''}
	resourceName={shareTarget?.name || ''}
	resourceType={shareType}
	on:close={() => { showShareModal = false; shareTarget = null; }}
	on:notification={handleShareNotification}
/>

<VersionHistoryModal
	open={showVersionHistoryModal}
	fileId={versionHistoryTarget?.id || ''}
	fileName={versionHistoryTarget?.name || ''}
	on:close={() => { showVersionHistoryModal = false; versionHistoryTarget = null; }}
	on:restored={handleVersionRestored}
/>

<FilePreviewModal
	open={showFilePreviewModal}
	file={previewTarget}
	on:close={() => { showFilePreviewModal = false; previewTarget = null; }}
/>

<ReplaceFileModal
	open={showReplaceFileModal}
	file={replaceFileTarget}
	on:close={() => { showReplaceFileModal = false; replaceFileTarget = null; }}
	on:success={handleReplaceSuccess}
/>

<!-- Toast -->
{#if showToast}
	<Toast message={toastMessage} type={toastType} onClose={() => showToast = false} />
{/if}
