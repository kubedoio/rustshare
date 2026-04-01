<script lang="ts">
	import { onMount } from 'svelte';
	import { createQuery, createMutation } from '@tanstack/svelte-query';
	import {
		deleteFile,
		downloadFile,
		getDeletedContents,
		getStarredContents,
		moveFile,
		permanentlyDeleteFile,
		renameFile,
		restoreFileFromTrash,
		setFileStarred,
		uploadFile
	} from '$lib/api/files';
	import {
		createFolder,
		deleteFolder,
		getFolderContents,
		moveFolder,
		permanentlyDeleteFolder,
		renameFolder,
		restoreFolderFromTrash,
		setFolderStarred
	} from '$lib/api/folders';
	import { queryClient } from '$lib/query-client';
	import { searchQuery } from '$lib/stores/search';
	import { fileSortState } from '$lib/stores/fileSort';
	import { selectionStore, selectionCount, hasSelection } from '$lib/stores/selection';
	import { activityStore } from '$lib/stores/activity';
	import { replicationStore, type ReplicationStatus } from '$lib/stores/replication';
	import { folderTreeStore, type FolderNode } from '$lib/stores/folderTree';
	import type { File, Folder } from '$lib/api/types';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';

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

	// Derive folderPath from folder tree store based on currentFolderId
	$: folderPath = currentFolderId ? buildFolderPathFromTree($folderTreeStore.rootFolders, currentFolderId) : [];

	// Modal states
	let showRenameModal = false;
	let showDeleteModal = false;
	let showMoveModal = false;
	let showShareModal = false;
	let showCreateFolderModal = false;
	let showVersionHistoryModal = false;
	let showFilePreviewModal = false;
	let showReplaceFileModal = false;
	let bulkMoveFileIds: string[] = [];
	let bulkMoveLoading = false;

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

	type WorkspaceMode = 'all' | 'photos' | 'recent' | 'starred' | 'deleted';

	// Toast
	let showToast = false;
	let toastMessage = '';
	let toastType: 'success' | 'error' | 'info' = 'info';

	// Query for the active workspace view
	$: filesQuery = createQuery({
		queryKey: ['file-workspace', workspaceMode, currentFolderId],
		queryFn: () => {
			if (workspaceMode === 'starred') return getStarredContents();
			if (workspaceMode === 'deleted') return getDeletedContents();
			return getFolderContents(currentFolderId);
		}
	});

	// Mutations
	const uploadMutation = createMutation({
		mutationFn: (file: globalThis.File) => uploadFile(currentFolderId, file),
		onSuccess: (_, file) => {
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			activityStore.addActivity('file_uploaded', file.name);
		}
	});

	const createFolderMutation = createMutation({
		mutationFn: (name: string) => createFolder(name, currentFolderId),
		onSuccess: (folder) => {
			// Immediately update UI - add folder to tree
			folderTreeStore.addFolder(folder, currentFolderId);
			// Expand parent folder so new folder is visible
			if (currentFolderId) {
				folderTreeStore.setExpanded(currentFolderId, true);
			}
			// Refresh queries
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['folder-tree'] });
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
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			showRenameModal = false;
			renameTarget = null;
			showNotification('File renamed', 'success');
			activityStore.addActivity('file_renamed', newName, oldName);
		}
	});

	const renameFolderMutation = createMutation({
		mutationFn: ({ folderId, newName }: { folderId: string; newName: string }) => renameFolder(folderId, newName),
		onSuccess: (_, { folderId, newName }) => {
			const oldName = renameTarget?.name || 'Folder';
			// Immediately update UI
			folderTreeStore.updateFolderName(folderId, newName);
			// Refresh queries
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['folder-tree'] });
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
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			showDeleteModal = false;
			deleteTarget = null;
			showNotification('File moved to deleted', 'success');
			activityStore.addActivity('file_deleted', fileName);
		}
	});

	const deleteFolderMutation = createMutation({
		mutationFn: (folderId: string) => deleteFolder(folderId),
		onSuccess: (_, folderId) => {
			const folderName = deleteTarget?.name || 'Folder';
			// Immediately update UI
			folderTreeStore.removeFolder(folderId);
			// Refresh queries
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['folder-tree'] });
			// If we deleted the current folder or one in its path, go to root
			if (deleteTarget && (currentFolderId === deleteTarget.id || folderPath.some(f => f.id === deleteTarget?.id))) {
				currentFolderId = null;
				goto('/files', { replaceState: true });
			}
			showDeleteModal = false;
			deleteTarget = null;
			showNotification('Folder moved to deleted', 'success');
			activityStore.addActivity('folder_deleted', folderName);
		}
	});

	const moveFileMutation = createMutation({
		mutationFn: ({ fileId, targetFolderId }: { fileId: string; targetFolderId: string | null }) => moveFile(fileId, targetFolderId),
		onSuccess: () => {
			const fileName = moveTarget?.name || 'File';
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['folder-tree'] });
			showMoveModal = false;
			moveTarget = null;
			showNotification('File moved', 'success');
			activityStore.addActivity('file_moved', fileName);
		}
	});

	const moveFolderMutation = createMutation({
		mutationFn: ({ folderId, targetFolderId }: { folderId: string; targetFolderId: string | null }) => moveFolder(folderId, targetFolderId),
		onSuccess: (_, { folderId, targetFolderId }) => {
			const folderName = moveTarget?.name || 'Folder';
			// Immediately update UI - move folder in tree
			folderTreeStore.moveFolder(folderId, targetFolderId);
			// Expand destination folder so moved folder is visible
			if (targetFolderId) {
				folderTreeStore.setExpanded(targetFolderId, true);
			}
			// Refresh queries
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['folder-tree'] });
			showMoveModal = false;
			moveTarget = null;
			showNotification('Folder moved', 'success');
			activityStore.addActivity('folder_moved', folderName);
		}
	});

	const fileStarMutation = createMutation({
		mutationFn: ({ fileId, starred }: { fileId: string; starred: boolean }) =>
			setFileStarred(fileId, starred),
		onSuccess: (_, variables) => {
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			showNotification(
				variables.starred ? 'File added to starred' : 'File removed from starred',
				'success'
			);
		}
	});

	const folderStarMutation = createMutation({
		mutationFn: ({ folderId, starred }: { folderId: string; starred: boolean }) =>
			setFolderStarred(folderId, starred),
		onSuccess: (_, variables) => {
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			showNotification(
				variables.starred ? 'Folder added to starred' : 'Folder removed from starred',
				'success'
			);
		}
	});

	const restoreFileMutation = createMutation({
		mutationFn: (fileId: string) => restoreFileFromTrash(fileId),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			showNotification('File restored', 'success');
		}
	});

	const restoreFolderMutation = createMutation({
		mutationFn: (folderId: string) => restoreFolderFromTrash(folderId),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			showNotification('Folder restored', 'success');
		}
	});

	const permanentlyDeleteFileMutation = createMutation({
		mutationFn: (fileId: string) => permanentlyDeleteFile(fileId),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			showNotification('File deleted permanently', 'success');
		}
	});

	const permanentlyDeleteFolderMutation = createMutation({
		mutationFn: (folderId: string) => permanentlyDeleteFolder(folderId),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			showNotification('Folder deleted permanently', 'success');
		}
	});

	$: urlFilter = $page.url.searchParams.get('filter');
	$: urlSort = $page.url.searchParams.get('sort');
	$: urlFolderId = $page.url.searchParams.get('folder');
	
	// Helper to check if a string looks like a valid UUID
	function isValidUuid(value: string | null): value is string {
		if (!value) return false;
		// UUID v4 regex pattern
		const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
		return uuidPattern.test(value);
	}
	
	// Sync currentFolderId with URL folder param
	// Only accept valid UUIDs, ignore special values like 'shared', 'starred', etc.
	$: if (isValidUuid(urlFolderId) && urlFolderId !== currentFolderId) {
		currentFolderId = urlFolderId;
	} else if ((!urlFolderId || !isValidUuid(urlFolderId)) && currentFolderId && !$page.url.searchParams.has('filter') && !$page.url.searchParams.has('sort')) {
		// Only clear if we're on a plain /files page without filters
		currentFolderId = null;
	}
	
	$: workspaceMode = urlFilter === 'photos'
		? 'photos'
		: urlFilter === 'starred'
			? 'starred'
			: urlFilter === 'deleted'
				? 'deleted'
				: urlSort === 'recent'
					? 'recent'
					: 'all';

	$: activeSortField = workspaceMode === 'recent' ? 'modified_at' : $fileSortState.field;
	$: activeSortOrder = workspaceMode === 'recent' ? 'desc' : $fileSortState.order;
	$: searchTerm = $searchQuery.trim().toLowerCase();

	function matchesSearch(name: string) {
		return searchTerm.length === 0 || name.toLowerCase().includes(searchTerm);
	}

	$: baseFolders = ($filesQuery.data?.folders || []).filter((folder) => matchesSearch(folder.name));
	$: baseFiles = ($filesQuery.data?.files || []).filter((file) => matchesSearch(file.name));

	$: filteredFolders =
		workspaceMode === 'all' || workspaceMode === 'starred' || workspaceMode === 'deleted'
			? baseFolders
			: [];
	$: filteredFiles = workspaceMode === 'photos'
		? baseFiles.filter((file) => file.mime_type.startsWith('image/'))
		: baseFiles;

	$: sortedFolders = [...filteredFolders].sort((a, b) => {
		if (activeSortField === 'name') {
			return activeSortOrder === 'asc' ? a.name.localeCompare(b.name) : b.name.localeCompare(a.name);
		}
		if (activeSortField === 'modified_at') {
			return activeSortOrder === 'asc' 
				? new Date(a.updated_at).getTime() - new Date(b.updated_at).getTime()
				: new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime();
		}
		return 0;
	});

	$: sortedFiles = [...filteredFiles].sort((a, b) => {
		if (activeSortField === 'name') {
			return activeSortOrder === 'asc' ? a.name.localeCompare(b.name) : b.name.localeCompare(a.name);
		}
		if (activeSortField === 'modified_at') {
			return activeSortOrder === 'asc'
				? new Date(a.modified_at).getTime() - new Date(b.modified_at).getTime()
				: new Date(b.modified_at).getTime() - new Date(a.modified_at).getTime();
		}
		if (activeSortField === 'size') {
			return activeSortOrder === 'asc' ? a.size - b.size : b.size - a.size;
		}
		if (activeSortField === 'mime_type') {
			return activeSortOrder === 'asc' ? a.mime_type.localeCompare(b.mime_type) : b.mime_type.localeCompare(a.mime_type);
		}
		return 0;
	});

	$: workspaceTitle =
		workspaceMode === 'photos'
			? 'Photos'
			: workspaceMode === 'recent'
				? 'Recent'
				: workspaceMode === 'starred'
					? 'Starred'
					: workspaceMode === 'deleted'
						? 'Deleted'
						: 'All files';

	$: workspaceDescription =
		workspaceMode === 'photos'
			? 'Image files in the current workspace, without the folder noise.'
			: workspaceMode === 'recent'
				? 'The latest changes in this workspace, sorted by most recent first.'
				: workspaceMode === 'starred'
					? 'Pinned folders and files that need fast access without digging through the tree.'
					: workspaceMode === 'deleted'
						? 'Recently deleted items live here until you restore them or remove them permanently.'
						: 'Folders and files, tuned for quick scanning instead of dashboard theater.';

	$: workspaceEmptyTitle =
		workspaceMode === 'photos'
			? 'No photos in this view'
			: workspaceMode === 'recent'
				? 'No recent file activity'
				: workspaceMode === 'starred'
					? 'Nothing is starred yet'
					: workspaceMode === 'deleted'
						? 'Deleted items will show up here'
						: 'No files yet';

	$: workspaceEmptyDescription =
		workspaceMode === 'photos'
			? 'Upload an image into this folder and it will show up here.'
			: workspaceMode === 'recent'
				? 'Modify or upload a file and it will show up here.'
				: workspaceMode === 'starred'
					? 'Star a folder or file from its action menu and it will show up here.'
					: workspaceMode === 'deleted'
						? 'Deleting a folder or file moves it here instead of removing it immediately.'
						: 'Upload your first file or create a folder to get started.';

	$: workspaceEmptyActionLabel =
		workspaceMode === 'all' || workspaceMode === 'photos' || workspaceMode === 'recent'
			? 'Upload files'
			: null;

	$: showFolderTree = workspaceMode === 'all';
	$: showBreadcrumbs = workspaceMode === 'all';
	$: canCreateFolder = workspaceMode === 'all';
	$: canUpload = workspaceMode === 'all' || workspaceMode === 'photos' || workspaceMode === 'recent';
	$: allowSelectionMode = workspaceMode !== 'deleted';
	$: if (!allowSelectionMode && selectionMode) {
		selectionMode = false;
		selectionStore.clear();
	}

	// Build folder path from tree structure
	function buildFolderPathFromTree(folders: FolderNode[], targetId: string): Folder[] {
		for (const folder of folders) {
			if (folder.id === targetId) {
				return [{
					id: folder.id,
					name: folder.name,
					path: folder.path,
					parent_folder_id: folder.parent_folder_id,
					owner_id: '',
					created_at: '',
					updated_at: ''
				}];
			}
			if (folder.children && folder.children.length > 0) {
				const path = buildFolderPathFromTree(folder.children, targetId);
				if (path.length > 0) {
					return [{
						id: folder.id,
						name: folder.name,
						path: folder.path,
						parent_folder_id: folder.parent_folder_id,
						owner_id: '',
						created_at: '',
						updated_at: ''
					}, ...path];
				}
			}
		}
		return [];
	}

	// Handlers
	function handleFolderSelect(folderId: string | null, path: FolderNode[]) {
		currentFolderId = folderId;
		if (folderId) {
			goto(`/files?folder=${folderId}`, { replaceState: true });
		} else {
			goto('/files', { replaceState: true });
		}
	}

	function handleFolderClick(folder: Folder) {
		currentFolderId = folder.id;
		goto(`/files?folder=${folder.id}`, { replaceState: true });
	}

	function handleBreadcrumbNavigate(event: CustomEvent<{ folderId: string | null }>) {
		const targetId = event.detail.folderId;
		if (targetId === null) {
			currentFolderId = null;
			goto('/files', { replaceState: true });
		} else {
			currentFolderId = targetId;
			goto(`/files?folder=${targetId}`, { replaceState: true });
		}
	}

	function handleFileClick(file: File) {
		if (workspaceMode === 'deleted') return;
		previewTarget = file;
		showFilePreviewModal = true;
	}

	function showNotification(message: string, type: 'success' | 'error' | 'info') {
		toastMessage = message;
		toastType = type;
		showToast = true;
	}

	async function handleFilesSelected(files: globalThis.File[]) {
		if (!canUpload) return;
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

	async function handleBulkDownload() {
		const selectedFileIds = new Set($selectionStore.selectedFileIds);
		const selectedFiles = sortedFiles.filter((file) => selectedFileIds.has(file.id));
		const skippedFolderCount = $selectionStore.selectedFolderIds.size;

		if (selectedFiles.length === 0) {
			showNotification(
				skippedFolderCount > 0
					? 'Bulk download is available for files only right now'
					: 'Select at least one file to download',
				'info'
			);
			return;
		}

		let successCount = 0;

		for (const file of selectedFiles) {
			try {
				const response = await downloadFile(file.id);
				let downloadUrl = response.url;
				if (downloadUrl.includes('/rustshare-files/')) {
					const path = downloadUrl.split('/rustshare-files/')[1];
					downloadUrl = `/storage/${path}`;
				}
				window.open(downloadUrl, '_blank');
				activityStore.addActivity('file_downloaded', file.name);
				successCount += 1;
			} catch (error) {
				console.error('Failed to download selected file:', file.name, error);
			}
		}

		if (successCount === 0) {
			showNotification('Failed to start the selected downloads', 'error');
			return;
		}

		const parts = [`Started ${successCount} download${successCount === 1 ? '' : 's'}`];
		if (skippedFolderCount > 0) {
			parts.push(`skipped ${skippedFolderCount} folder${skippedFolderCount === 1 ? '' : 's'}`);
		}

		showNotification(parts.join(', '), skippedFolderCount > 0 ? 'info' : 'success');
	}

	function handleBulkMove() {
		const selectedFileIds = Array.from($selectionStore.selectedFileIds);

		if (selectedFileIds.length === 0) {
			showNotification(
				$selectionStore.selectedFolderIds.size > 0
					? 'Bulk move currently supports files only. Deselect folders and try again.'
					: 'Select at least one file to move',
				'info'
			);
			return;
		}

		if ($selectionStore.selectedFolderIds.size > 0) {
			showNotification('Bulk move currently supports files only. Deselect folders and try again.', 'info');
			return;
		}

		bulkMoveFileIds = selectedFileIds;
		moveTarget = null;
		moveType = 'file';
		showMoveModal = true;
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
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			showNotification(`Deleted ${fileIds.length + folderIds.length} item(s)`, 'success');
		} catch (error) {
			showNotification('Failed to delete some items', 'error');
		}
	}

	// Rename handlers - support both modal and inline
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

	function handleRenameFileInline(file: File, newName: string) {
		$renameFileMutation.mutate({ fileId: file.id, newName });
	}

	function handleRenameFolderInline(folder: Folder, newName: string) {
		$renameFolderMutation.mutate({ folderId: folder.id, newName });
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

	// Move handlers - support both modal and direct
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

	function handleMoveFileWithFallback(file: File, targetFolderId: string | null) {
		if (targetFolderId === null) {
			// Open modal for user to select destination
			moveTarget = file;
			moveType = 'file';
			showMoveModal = true;
		} else {
			// Direct move (e.g., from drag-and-drop)
			$moveFileMutation.mutate({ fileId: file.id, targetFolderId });
		}
	}

	function handleMoveFolderWithFallback(folder: Folder, targetFolderId: string | null) {
		if (targetFolderId === null) {
			// Open modal for user to select destination
			moveTarget = folder;
			moveType = 'folder';
			showMoveModal = true;
		} else {
			// Direct move (e.g., from drag-and-drop)
			$moveFolderMutation.mutate({ folderId: folder.id, targetFolderId });
		}
	}

	async function handleMoveConfirm(event: CustomEvent<{ targetFolderId: string | null }>) {
		if (bulkMoveFileIds.length > 0) {
			bulkMoveLoading = true;

			try {
				for (const fileId of bulkMoveFileIds) {
					await moveFile(fileId, event.detail.targetFolderId);
				}

				queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
				selectionStore.clear();
				selectionMode = false;
				showNotification(`Moved ${bulkMoveFileIds.length} selected file${bulkMoveFileIds.length === 1 ? '' : 's'}`, 'success');
			} catch (error) {
				showNotification(error instanceof Error ? error.message : 'Failed to move selected files', 'error');
			} finally {
				bulkMoveLoading = false;
				bulkMoveFileIds = [];
				showMoveModal = false;
			}

			return;
		}

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
		if (workspaceMode === 'deleted') return;
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
		queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
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
		queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
		showNotification('File version restored', 'success');
	}

	function handleToggleFileStar(file: File) {
		$fileStarMutation.mutate({ fileId: file.id, starred: !file.starred_at });
	}

	function handleToggleFolderStar(folder: Folder) {
		$folderStarMutation.mutate({ folderId: folder.id, starred: !folder.starred_at });
	}

	function handleRestoreFile(file: File) {
		$restoreFileMutation.mutate(file.id);
	}

	function handleRestoreFolder(folder: Folder) {
		$restoreFolderMutation.mutate(folder.id);
	}

	function handlePermanentDeleteFile(file: File) {
		if (!confirm(`Permanently delete ${file.name}? This cannot be undone.`)) return;
		$permanentlyDeleteFileMutation.mutate(file.id);
	}

	function handlePermanentDeleteFolder(folder: Folder) {
		if (!confirm(`Permanently delete ${folder.name} and everything inside it? This cannot be undone.`)) return;
		$permanentlyDeleteFolderMutation.mutate(folder.id);
	}

	// Listen for create folder event from sidebar
	onMount(() => {
		const handleCreateFolderEvent = () => {
			if (canCreateFolder) {
				showCreateFolderModal = true;
			}
		};
		window.addEventListener('create-folder-requested', handleCreateFolderEvent);
		return () => {
			window.removeEventListener('create-folder-requested', handleCreateFolderEvent);
		};
	});

	// Keyboard shortcuts
	function handleKeyDown(event: KeyboardEvent) {
		if (event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement) return;

		if (event.key === 'a' && (event.ctrlKey || event.metaKey)) {
			if (!allowSelectionMode) return;
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
				if (!canUpload) return;
				event.preventDefault();
				document.getElementById('upload-file-input')?.click();
				break;
			case 'n':
				if (!canCreateFolder) return;
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

<DropZone on:filesDropped={(e) => handleFilesSelected(e.detail)} disabled={!canUpload || isUploading}>
	<FileExplorer
		folders={sortedFolders}
		files={sortedFiles}
		{currentFolderId}
		{folderPath}
		title={workspaceTitle}
		description={workspaceDescription}
		emptyTitle={workspaceEmptyTitle}
		emptyDescription={workspaceEmptyDescription}
		emptyActionLabel={workspaceEmptyActionLabel}
		{workspaceMode}
		{showFolderTree}
		{showBreadcrumbs}
		{canCreateFolder}
		{canUpload}
		{allowSelectionMode}
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
		onBulkDownload={handleBulkDownload}
		onBulkMove={handleBulkMove}
		onRenameFile={handleRenameFileInline}
		onDeleteFile={handleDeleteFile}
		onToggleFileStar={handleToggleFileStar}
		onRestoreFile={handleRestoreFile}
		onPermanentDeleteFile={handlePermanentDeleteFile}
		onShareFile={handleShareFile}
		onVersionHistory={handleVersionHistory}
		onMoveFile={handleMoveFileWithFallback}
		onDownloadFile={handleDownloadFile}
		onReplaceFile={handleReplaceFile}
		onRenameFolder={handleRenameFolderInline}
		onDeleteFolder={handleDeleteFolder}
		onToggleFolderStar={handleToggleFolderStar}
		onRestoreFolder={handleRestoreFolder}
		onPermanentDeleteFolder={handlePermanentDeleteFolder}
		onShareFolder={handleShareFolder}
		onMoveFolder={handleMoveFolderWithFallback}
		on:breadcrumbNavigate={handleBreadcrumbNavigate}
	/>
</DropZone>

<!-- Upload Progress -->
<UploadProgress tasks={uploadTasks} onClose={handleCloseProgress} />

<!-- Modals -->
{#if showRenameModal}
	<RenameModal
		open={showRenameModal}
		loading={$renameFileMutation.isPending || $renameFolderMutation.isPending}
		itemName={renameTarget?.name || ''}
		itemType={renameType}
		on:close={() => { showRenameModal = false; renameTarget = null; }}
		on:confirm={handleRenameConfirm}
	/>
{/if}

{#if showDeleteModal}
	<DeleteConfirmation
		open={showDeleteModal}
		loading={$deleteFileMutation.isPending || $deleteFolderMutation.isPending}
		itemName={deleteTarget?.name || ''}
		itemType={deleteType}
		on:close={() => { showDeleteModal = false; deleteTarget = null; }}
		on:confirm={handleDeleteConfirm}
	/>
{/if}

{#if showMoveModal}
	<MoveModal
		open={showMoveModal}
		loading={$moveFileMutation.isPending || $moveFolderMutation.isPending || bulkMoveLoading}
		itemName={bulkMoveFileIds.length > 0 ? `${bulkMoveFileIds.length} selected file${bulkMoveFileIds.length === 1 ? '' : 's'}` : moveTarget?.name || ''}
		itemType={moveType}
		itemId={bulkMoveFileIds.length > 0 ? null : moveTarget?.id || null}
		currentFolderId={bulkMoveFileIds.length > 0 ? currentFolderId : moveCurrentFolderId}
		on:close={() => { showMoveModal = false; moveTarget = null; bulkMoveFileIds = []; }}
		on:confirm={handleMoveConfirm}
	/>
{/if}

{#if showCreateFolderModal}
	<CreateFolderModal
		open={showCreateFolderModal}
		loading={$createFolderMutation.isPending}
		on:close={() => showCreateFolderModal = false}
		on:confirm={handleCreateFolder}
	/>
{/if}

{#if showShareModal}
	<ShareModal
		open={showShareModal}
		resourceId={shareTarget?.id || ''}
		resourceName={shareTarget?.name || ''}
		resourceType={shareType}
		on:close={() => { showShareModal = false; shareTarget = null; }}
		on:notification={handleShareNotification}
	/>
{/if}

{#if showVersionHistoryModal && versionHistoryTarget}
	<VersionHistoryModal
		open={showVersionHistoryModal}
		fileId={versionHistoryTarget.id}
		fileName={versionHistoryTarget.name}
		on:close={() => { showVersionHistoryModal = false; versionHistoryTarget = null; }}
		on:restored={handleVersionRestored}
	/>
{/if}

{#if showFilePreviewModal && previewTarget}
	<FilePreviewModal
		open={showFilePreviewModal}
		file={previewTarget}
		on:close={() => { showFilePreviewModal = false; previewTarget = null; }}
	/>
{/if}

{#if showReplaceFileModal && replaceFileTarget}
	<ReplaceFileModal
		open={showReplaceFileModal}
		file={replaceFileTarget}
		on:close={() => { showReplaceFileModal = false; replaceFileTarget = null; }}
		on:success={handleReplaceSuccess}
	/>
{/if}

<!-- Toast -->
{#if showToast}
	<Toast message={toastMessage} type={toastType} onClose={() => showToast = false} />
{/if}
