<script lang="ts">
	import { onMount } from 'svelte';
	import { createQuery, createMutation } from '@tanstack/svelte-query';
	import {
		listAllFiles,
		downloadFile,
		uploadFile,
		renameFile,
		deleteFile,
		moveFile
	} from '$lib/api/files';
	import {
		getFolderContents,
		createFolder,
		renameFolder,
		deleteFolder,
		moveFolder
	} from '$lib/api/folders';
	import { queryClient } from '$lib/query-client';
	import FileGrid from '$lib/components/files/FileGrid.svelte';
	import FileList from '$lib/components/files/FileList.svelte';
	import FileGridSkeleton from '$lib/components/files/FileGridSkeleton.svelte';
	import UploadButton from '$lib/components/files/UploadButton.svelte';
	import UploadProgress from '$lib/components/files/UploadProgress.svelte';
	import DropZone from '$lib/components/files/DropZone.svelte';
	import Toast from '$lib/components/common/Toast.svelte';
	import RenameModal from '$lib/components/modals/RenameModal.svelte';
	import DeleteConfirmation from '$lib/components/modals/DeleteConfirmation.svelte';
	import MoveModal from '$lib/components/modals/MoveModal.svelte';
	import ShareModal from '$lib/components/modals/ShareModal.svelte';
	import CreateFolderModal from '$lib/components/modals/CreateFolderModal.svelte';
	import VersionHistoryModal from '$lib/components/modals/VersionHistoryModal.svelte';
	import FilePreviewModal from '$lib/components/modals/FilePreviewModal.svelte';
	import ReplaceFileModal from '$lib/components/modals/ReplaceFileModal.svelte';
	import Breadcrumbs from '$lib/components/layout/Breadcrumbs.svelte';
	import { showKeyboardShortcuts } from '$lib/stores/ui';
	import { replicationStore, type ReplicationStatus } from '$lib/stores/replication';
	import { searchQuery } from '$lib/stores/search';
	import { fileSortState, setSortField, setViewMode, type SortField } from '$lib/stores/fileSort';
	import { selectionStore, selectionCount, hasSelection } from '$lib/stores/selection';
	import { activityStore } from '$lib/stores/activity';
	import type { File, Folder } from '$lib/api/types';
	import type { WebSocketEvent } from '$lib/websocket/events';

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
	let replicationStatuses: Record<string, ReplicationStatus> = {};
	let showToast = false;
	let toastMessage = '';
	let toastType: 'success' | 'error' | 'info' = 'info';
	let selectionMode = false;

	// Current folder navigation state
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
	let renameTarget: File | Folder | null = null;
	let renameType: 'file' | 'folder' = 'file';
	let deleteTarget: File | Folder | null = null;
	let deleteType: 'file' | 'folder' = 'file';
	let moveTarget: File | Folder | null = null;
	let moveType: 'file' | 'folder' = 'file';

	// Compute current folder ID for move modal
	$: moveCurrentFolderId =
		moveType === 'file'
			? (moveTarget as File | null)?.parent_folder_id
			: (moveTarget as Folder | null)?.parent_folder_id;
	let shareTarget: File | Folder | null = null;
	let shareType: 'file' | 'folder' = 'file';
	let versionHistoryTarget: File | null = null;
	let previewTarget: File | null = null;
	let replaceFileTarget: File | null = null;

	// Query for folder contents (or root contents if at root)
	// Use $: to make the query reactive to currentFolderId changes
	$: filesQuery = createQuery({
		queryKey: ['folder-contents', currentFolderId],
		queryFn: async () => {
			// Use getFolderContents for both root and folders
			return getFolderContents(currentFolderId);
		}
	});

	// Upload mutation
	const uploadMutation = createMutation({
		mutationFn: async (file: globalThis.File) => {
			return uploadFile(currentFolderId, file);
		},
		onSuccess: (_, file) => {
			queryClient.invalidateQueries({ queryKey: ['folder-contents', currentFolderId] });
			activityStore.addActivity('file_uploaded', file.name);
		}
	});

	// Create folder mutation
	const createFolderMutation = createMutation({
		mutationFn: async (name: string) => {
			return createFolder(name, currentFolderId);
		},
		onSuccess: (folder) => {
			queryClient.invalidateQueries({ queryKey: ['folder-contents', currentFolderId] });
			queryClient.invalidateQueries({ queryKey: ['folder-tree'] });
			showCreateFolderModal = false;
			showNotification('Folder created successfully', 'success');
			activityStore.addActivity('folder_created', folder.name);
		},
		onError: (error) => {
			showNotification(error instanceof Error ? error.message : 'Failed to create folder', 'error');
		}
	});

	// Rename file mutation
	const renameFileMutation = createMutation({
		mutationFn: async ({ fileId, newName }: { fileId: string; newName: string }) => {
			return renameFile(fileId, newName);
		},
		onSuccess: (_, { newName }) => {
			const oldName = renameTarget?.name || 'File';
			queryClient.invalidateQueries({ queryKey: ['folder-contents', currentFolderId] });
			showRenameModal = false;
			renameTarget = null;
			showNotification('File renamed successfully', 'success');
			activityStore.addActivity('file_renamed', newName, oldName);
		},
		onError: (error) => {
			showNotification(error instanceof Error ? error.message : 'Failed to rename file', 'error');
		}
	});

	// Rename folder mutation
	const renameFolderMutation = createMutation({
		mutationFn: async ({ folderId, newName }: { folderId: string; newName: string }) => {
			return renameFolder(folderId, newName);
		},
		onSuccess: (_, { newName }) => {
			const oldName = renameTarget?.name || 'Folder';
			queryClient.invalidateQueries({ queryKey: ['folder-contents', currentFolderId] });
			queryClient.invalidateQueries({ queryKey: ['folder-tree'] });
			showRenameModal = false;
			renameTarget = null;
			showNotification('Folder renamed successfully', 'success');
			activityStore.addActivity('folder_renamed', newName, oldName);
		},
		onError: (error) => {
			showNotification(error instanceof Error ? error.message : 'Failed to rename folder', 'error');
		}
	});

	// Delete file mutation
	const deleteFileMutation = createMutation({
		mutationFn: async (fileId: string) => {
			return deleteFile(fileId);
		},
		onSuccess: (_, fileId) => {
			const fileName = deleteTarget?.name || 'File';
			queryClient.invalidateQueries({ queryKey: ['folder-contents', currentFolderId] });
			showDeleteModal = false;
			deleteTarget = null;
			showNotification('File deleted successfully', 'success');
			activityStore.addActivity('file_deleted', fileName);
		},
		onError: (error) => {
			showNotification(error instanceof Error ? error.message : 'Failed to delete file', 'error');
		}
	});

	// Delete folder mutation
	const deleteFolderMutation = createMutation({
		mutationFn: async (folderId: string) => {
			return deleteFolder(folderId);
		},
		onSuccess: () => {
			const folderName = deleteTarget?.name || 'Folder';
			queryClient.invalidateQueries({ queryKey: ['folder-contents', currentFolderId] });
			queryClient.invalidateQueries({ queryKey: ['folder-tree'] });
			showDeleteModal = false;
			deleteTarget = null;
			showNotification('Folder deleted successfully', 'success');
			activityStore.addActivity('folder_deleted', folderName);
		},
		onError: (error) => {
			showNotification(error instanceof Error ? error.message : 'Failed to delete folder', 'error');
		}
	});

	// Move file mutation
	const moveFileMutation = createMutation({
		mutationFn: async ({
			fileId,
			targetFolderId
		}: {
			fileId: string;
			targetFolderId: string | null;
		}) => {
			return moveFile(fileId, targetFolderId);
		},
		onSuccess: (_, { targetFolderId }) => {
			const fileName = moveTarget?.name || 'File';
			queryClient.invalidateQueries({ queryKey: ['folder-contents', currentFolderId] });
			queryClient.invalidateQueries({ queryKey: ['folder-contents', targetFolderId] });
			showMoveModal = false;
			moveTarget = null;
			showNotification('File moved successfully', 'success');
			activityStore.addActivity('file_moved', fileName);
		},
		onError: (error) => {
			showNotification(error instanceof Error ? error.message : 'Failed to move file', 'error');
		}
	});

	// Move folder mutation
	const moveFolderMutation = createMutation({
		mutationFn: async ({
			folderId,
			targetFolderId
		}: {
			folderId: string;
			targetFolderId: string | null;
		}) => {
			return moveFolder(folderId, targetFolderId);
		},
		onSuccess: (_, { targetFolderId }) => {
			const folderName = moveTarget?.name || 'Folder';
			queryClient.invalidateQueries({ queryKey: ['folder-contents', currentFolderId] });
			queryClient.invalidateQueries({ queryKey: ['folder-contents', targetFolderId] });
			queryClient.invalidateQueries({ queryKey: ['folder-tree'] });
			showMoveModal = false;
			moveTarget = null;
			showNotification('Folder moved successfully', 'success');
			activityStore.addActivity('folder_moved', folderName);
		},
		onError: (error) => {
			showNotification(error instanceof Error ? error.message : 'Failed to move folder', 'error');
		}
	});

	function handleFolderClick(folder: Folder) {
		currentFolderId = folder.id;
		folderPath = [...folderPath, folder];
	}

	function handleBreadcrumbNavigate(event: CustomEvent<{ folderId: string | null }>) {
		const targetId = event.detail.folderId;

		if (targetId === null) {
			// Navigate to root
			currentFolderId = null;
			folderPath = [];
		} else {
			// Navigate to a folder in the path
			const index = folderPath.findIndex((f) => f.id === targetId);
			if (index !== -1) {
				currentFolderId = targetId;
				folderPath = folderPath.slice(0, index + 1);
			}
		}
	}

	async function handleFileClick(file: File) {
		// Show preview modal instead of direct download
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

		// Helper function to generate preview URL for images
		async function generatePreview(file: globalThis.File): Promise<string | undefined> {
			if (!file.type.startsWith('image/')) return undefined;

			return new Promise((resolve) => {
				const reader = new FileReader();
				reader.onload = (e) => {
					const img = new Image();
					img.onload = () => {
						// Create canvas for thumbnail
						const canvas = document.createElement('canvas');
						const ctx = canvas.getContext('2d');
						if (!ctx) {
							resolve(undefined);
							return;
						}

						// Calculate thumbnail size (max 96px)
						const maxSize = 96;
						let width = img.width;
						let height = img.height;

						if (width > height) {
							if (width > maxSize) {
								height = (height * maxSize) / width;
								width = maxSize;
							}
						} else {
							if (height > maxSize) {
								width = (width * maxSize) / height;
								height = maxSize;
							}
						}

						canvas.width = width;
						canvas.height = height;
						ctx.drawImage(img, 0, 0, width, height);

						resolve(canvas.toDataURL('image/jpeg', 0.8));
					};
					img.onerror = () => resolve(undefined);
					img.src = e.target?.result as string;
				};
				reader.onerror = () => resolve(undefined);
				reader.readAsDataURL(file);
			});
		}

		// Create upload tasks with preview generation
		const newTasks: UploadTask[] = await Promise.all(
			files.map(async (file) => ({
				id: `${file.name}-${Date.now()}-${Math.random()}`,
				fileName: file.name,
				size: file.size,
				status: 'pending' as const,
				progress: 0,
				previewUrl: await generatePreview(file)
			}))
		);

		uploadTasks = [...uploadTasks, ...newTasks];

		// Upload files sequentially
		for (let i = 0; i < files.length; i++) {
			const file = files[i];
			const taskIndex = uploadTasks.findIndex((t) => t.id === newTasks[i].id);

			if (taskIndex === -1) continue;

			// Update status to uploading
			uploadTasks[taskIndex] = {
				...uploadTasks[taskIndex],
				status: 'uploading',
				progress: 50
			};
			uploadTasks = [...uploadTasks];

			try {
				await $uploadMutation.mutateAsync(file);

				// Mark as success
				uploadTasks[taskIndex] = {
					...uploadTasks[taskIndex],
					status: 'success',
					progress: 100
				};
				uploadTasks = [...uploadTasks];
			} catch (error) {
				// Mark as error
				const errorMessage = error instanceof Error ? error.message : 'Upload failed';
				uploadTasks[taskIndex] = {
					...uploadTasks[taskIndex],
					status: 'error',
					progress: 0,
					error: errorMessage
				};
				uploadTasks = [...uploadTasks];
			}
		}

		// Show completion notification
		const successCount = uploadTasks.filter((t) => t.status === 'success').length;
		const errorCount = uploadTasks.filter((t) => t.status === 'error').length;

		if (errorCount === 0) {
			showNotification(`Successfully uploaded ${successCount} file(s)`, 'success');
		} else if (successCount === 0) {
			showNotification(`Failed to upload ${errorCount} file(s)`, 'error');
		} else {
			showNotification(`Uploaded ${successCount} file(s), ${errorCount} failed`, 'info');
		}
	}

	function handleCloseProgress() {
		uploadTasks = [];
	}

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
			$renameFileMutation.mutate({
				fileId: renameTarget.id,
				newName: event.detail.newName
			});
		} else {
			$renameFolderMutation.mutate({
				folderId: renameTarget.id,
				newName: event.detail.newName
			});
		}
	}

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

	function handleVersionHistory(file: File) {
		console.log('[Page] handleVersionHistory called with file:', file);
		versionHistoryTarget = file;
		showVersionHistoryModal = true;
		console.log('[Page] showVersionHistoryModal set to:', showVersionHistoryModal);
		console.log('[Page] versionHistoryTarget set to:', versionHistoryTarget);
	}

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
			$moveFileMutation.mutate({
				fileId: moveTarget.id,
				targetFolderId: event.detail.targetFolderId
			});
		} else {
			$moveFolderMutation.mutate({
				folderId: moveTarget.id,
				targetFolderId: event.detail.targetFolderId
			});
		}
	}

	async function handleDownloadFile(file: File) {
		try {
			const response = await downloadFile(file.id);
			// Convert MinIO URL to /storage/ path (same pattern as thumbnails)
			// MinIO returns: http://rustfs:9000/rustshare-files/path/to/file
			// We want: /storage/path/to/file
			let downloadUrl = response.url;
			if (downloadUrl.includes('/rustshare-files/')) {
				const path = downloadUrl.split('/rustshare-files/')[1];
				downloadUrl = `/storage/${path}`;
			}

			// Trigger download
			window.open(downloadUrl, '_blank');
			showNotification('Download started', 'success');
			activityStore.addActivity('file_downloaded', file.name);
		} catch (error) {
			showNotification(error instanceof Error ? error.message : 'Failed to download file', 'error');
		}
	}

	function handleVersionRestored() {
		// Refresh the file list after version restore
		queryClient.invalidateQueries({ queryKey: ['folder-contents', currentFolderId] });
		showNotification('File version restored successfully', 'success');
	}

	function handleShareNotification(
		e: CustomEvent<{ message: string; type: 'success' | 'error' | 'info' }>
	) {
		showNotification(e.detail.message, e.detail.type);
	}

	function handleReplaceFile(file: File) {
		replaceFileTarget = file;
		showReplaceFileModal = true;
	}

	function handleReplaceSuccess() {
		// Refresh the file list after file replacement
		queryClient.invalidateQueries({ queryKey: ['folder-contents', currentFolderId] });

		// Truncate file name if longer than 10 characters
		const fileName = replaceFileTarget?.name || 'File';
		const displayName = fileName.length > 10 ? fileName.substring(0, 10) + '...' : fileName;

		showNotification(`${displayName} was modified`, 'success');

		if (replaceFileTarget) {
			activityStore.addActivity('file_modified', replaceFileTarget.name);
		}

		// Close the modal
		showReplaceFileModal = false;
		replaceFileTarget = null;
	}

	function handleDeleteConfirm() {
		if (!deleteTarget) return;

		if (deleteType === 'file') {
			$deleteFileMutation.mutate(deleteTarget.id);
		} else {
			$deleteFolderMutation.mutate(deleteTarget.id);
		}
	}

	function handleCreateFolder(event: CustomEvent<{ name: string }>) {
		$createFolderMutation.mutate(event.detail.name);
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

		const confirmed = confirm(
			`Delete ${fileIds.length} file(s) and ${folderIds.length} folder(s)?`
		);

		if (!confirmed) return;

		try {
			// Delete files
			for (const fileId of fileIds) {
				await deleteFile(fileId);
			}
			// Delete folders
			for (const folderId of folderIds) {
				await deleteFolder(folderId);
			}

			selectionStore.clear();
			selectionMode = false;
			queryClient.invalidateQueries({ queryKey: ['folder-contents'] });
			showNotification(`Deleted ${fileIds.length + folderIds.length} item(s)`, 'success');
		} catch (error) {
			console.error('Bulk delete error:', error);
			showNotification('Failed to delete some items', 'error');
		}
	}

	// Filter files and folders based on search query
	$: filteredFolders = $searchQuery
		? ($filesQuery.data?.folders || []).filter((folder) =>
				folder.name.toLowerCase().includes($searchQuery.toLowerCase())
			)
		: $filesQuery.data?.folders || [];

	$: filteredFiles = $searchQuery
		? ($filesQuery.data?.files || []).filter((file) =>
				file.name.toLowerCase().includes($searchQuery.toLowerCase())
			)
		: $filesQuery.data?.files || [];

	// Sort files and folders
	$: sortedFolders = (() => {
		const folders = [...filteredFolders];
		folders.sort((a, b) => {
			if ($fileSortState.field === 'name') {
				return $fileSortState.order === 'asc'
					? a.name.localeCompare(b.name)
					: b.name.localeCompare(a.name);
			} else if ($fileSortState.field === 'modified_at') {
				const aTime = new Date(a.updated_at).getTime();
				const bTime = new Date(b.updated_at).getTime();
				return $fileSortState.order === 'asc' ? aTime - bTime : bTime - aTime;
			}
			return 0;
		});
		return folders;
	})();

	$: sortedFiles = (() => {
		const files = [...filteredFiles];
		files.sort((a, b) => {
			if ($fileSortState.field === 'name') {
				return $fileSortState.order === 'asc'
					? a.name.localeCompare(b.name)
					: b.name.localeCompare(a.name);
			} else if ($fileSortState.field === 'modified_at') {
				const aTime = new Date(a.modified_at).getTime();
				const bTime = new Date(b.modified_at).getTime();
				return $fileSortState.order === 'asc' ? aTime - bTime : bTime - aTime;
			} else if ($fileSortState.field === 'size') {
				return $fileSortState.order === 'asc' ? a.size - b.size : b.size - a.size;
			} else if ($fileSortState.field === 'mime_type') {
				return $fileSortState.order === 'asc'
					? a.mime_type.localeCompare(b.mime_type)
					: b.mime_type.localeCompare(a.mime_type);
			}
			return 0;
		});
		return files;
	})();

	$: replicationStatuses = $replicationStore;

	// Handle real-time updates from WebSocket
	function handleWebSocketEvent(event: WebSocketEvent) {
		console.log('Received WebSocket event:', event);

		// Invalidate queries to refetch data based on event type
		switch (event.type) {
			case 'FileUploaded':
			case 'FileModified':
			case 'FileRenamed':
			case 'FileMoved':
			case 'FileDeleted':
			case 'FileRestored':
			case 'FolderCreated':
			case 'FolderRenamed':
			case 'FolderMoved':
			case 'FolderDeleted':
				// Refresh current folder contents
				queryClient.invalidateQueries({ queryKey: ['folder-contents', currentFolderId] });
				break;
		}
	}

	$: isUploading = uploadTasks.some((t) => t.status === 'uploading' || t.status === 'pending');
	$: isRenameLoading =
		renameType === 'file' ? $renameFileMutation.isPending : $renameFolderMutation.isPending;
	$: isDeleteLoading =
		deleteType === 'file' ? $deleteFileMutation.isPending : $deleteFolderMutation.isPending;
	$: isMoveLoading =
		moveType === 'file' ? $moveFileMutation.isPending : $moveFolderMutation.isPending;

	// WebSocket is managed by auth.ts and websocket/manager.ts
	// Event handlers are registered globally, no setup needed here

	// Keyboard shortcuts handler
	function handleKeyDown(event: KeyboardEvent) {
		// Ignore if typing in input field
		if (event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement) {
			return;
		}

		// Ctrl+A: Select all
		if (event.key === 'a' && (event.ctrlKey || event.metaKey)) {
			event.preventDefault();
			if (!selectionMode) {
				selectionMode = true;
			}
			handleSelectAll();
			return;
		}

		// Escape: Clear selection or close modals
		if (event.key === 'Escape') {
			if (selectionMode) {
				event.preventDefault();
				selectionStore.clear();
				selectionMode = false;
				return;
			}
			// Close any open modal
			showRenameModal = false;
			showDeleteModal = false;
			showMoveModal = false;
			showShareModal = false;
			showCreateFolderModal = false;
			showVersionHistoryModal = false;
			showFilePreviewModal = false;
			showKeyboardShortcuts.set(false);
			return;
		}

		switch (event.key.toLowerCase()) {
			case '?':
				event.preventDefault();
				showKeyboardShortcuts.set(true);
				break;
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
</script>

<svelte:head>
	<title>My Files - RustShare</title>
</svelte:head>

<svelte:window on:keydown={handleKeyDown} />

<DropZone on:filesDropped={(e) => handleFilesSelected(e.detail)} disabled={isUploading}>
	<div class="space-y-4">
		<!-- Breadcrumb Navigation -->
		<Breadcrumbs
			{folderPath}
			on:navigate={handleBreadcrumbNavigate}
		/>

		<div class="gap-2 flex items-center justify-between">
			<h1 class="text-xl lg:text-2xl font-bold truncate">
				{currentFolderId ? folderPath[folderPath.length - 1]?.name || 'My Files' : 'My Files'}
			</h1>
			<div class="gap-2 flex">
				{#if selectionMode}
					<!-- Selection mode toolbar -->
					<div class="gap-2 bg-base-200 rounded-lg px-3 py-2 flex items-center">
						<span class="text-sm font-medium">{$selectionCount} selected</span>
						<button class="btn btn-ghost btn-xs" on:click={handleSelectAll}> Select All </button>
						<button class="btn btn-ghost btn-xs" on:click={handleDeselectAll}> Clear </button>
						<button
							class="btn btn-error btn-xs"
							on:click={handleBulkDelete}
							disabled={!$hasSelection}
						>
							Delete
						</button>
						<button class="btn btn-ghost btn-xs" on:click={toggleSelectionMode}> Cancel </button>
					</div>
				{:else}
					<!-- Normal toolbar -->
					<button
						class="btn btn-ghost btn-sm lg:btn-md"
						on:click={toggleSelectionMode}
						title="Select multiple items"
					>
						<svg
							xmlns="http://www.w3.org/2000/svg"
							fill="none"
							viewBox="0 0 24 24"
							stroke-width="1.5"
							stroke="currentColor"
							class="w-4 h-4 lg:w-5 lg:h-5"
						>
							<path
								stroke-linecap="round"
								stroke-linejoin="round"
								d="M9 12.75L11.25 15 15 9.75M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
							/>
						</svg>
						<span class="sm:inline hidden">Select</span>
					</button>

					<!-- Sort dropdown -->
					<div class="dropdown dropdown-end">
						<button type="button" class="btn btn-ghost btn-sm lg:btn-md">
							<svg
								xmlns="http://www.w3.org/2000/svg"
								fill="none"
								viewBox="0 0 24 24"
								stroke-width="1.5"
								stroke="currentColor"
								class="w-4 h-4 lg:w-5 lg:h-5"
							>
								<path
									stroke-linecap="round"
									stroke-linejoin="round"
									d="M3 7.5L7.5 3m0 0L12 7.5M7.5 3v13.5m13.5 0L16.5 21m0 0L12 16.5m4.5 4.5V7.5"
								/>
							</svg>
							<span class="sm:inline hidden">Sort</span>
							{#if $fileSortState.order === 'desc'}
								<svg
									xmlns="http://www.w3.org/2000/svg"
									fill="none"
									viewBox="0 0 24 24"
									stroke-width="1.5"
									stroke="currentColor"
									class="w-3 h-3"
								>
									<path
										stroke-linecap="round"
										stroke-linejoin="round"
										d="M19.5 13.5L12 21m0 0l-7.5-7.5M12 21V3"
									/>
								</svg>
							{:else}
								<svg
									xmlns="http://www.w3.org/2000/svg"
									fill="none"
									viewBox="0 0 24 24"
									stroke-width="1.5"
									stroke="currentColor"
									class="w-3 h-3"
								>
									<path
										stroke-linecap="round"
										stroke-linejoin="round"
										d="M4.5 10.5L12 3m0 0l7.5 7.5M12 3v18"
									/>
								</svg>
							{/if}
						</button>
						<ul class="dropdown-content menu p-2 shadow bg-base-100 rounded-box w-52 z-[1]">
							<li>
								<button
									type="button"
									class:active={$fileSortState.field === 'name'}
									on:click={() => setSortField('name')}
								>
									Name
									{#if $fileSortState.field === 'name'}
										<span class="ml-auto">{$fileSortState.order === 'asc' ? '↑' : '↓'}</span>
									{/if}
								</button>
							</li>
							<li>
								<button
									type="button"
									class:active={$fileSortState.field === 'modified_at'}
									on:click={() => setSortField('modified_at')}
								>
									Date Modified
									{#if $fileSortState.field === 'modified_at'}
										<span class="ml-auto">{$fileSortState.order === 'asc' ? '↑' : '↓'}</span>
									{/if}
								</button>
							</li>
							<li>
								<button
									type="button"
									class:active={$fileSortState.field === 'size'}
									on:click={() => setSortField('size')}
								>
									Size
									{#if $fileSortState.field === 'size'}
										<span class="ml-auto">{$fileSortState.order === 'asc' ? '↑' : '↓'}</span>
									{/if}
								</button>
							</li>
							<li>
								<button
									type="button"
									class:active={$fileSortState.field === 'mime_type'}
									on:click={() => setSortField('mime_type')}
								>
									Type
									{#if $fileSortState.field === 'mime_type'}
										<span class="ml-auto">{$fileSortState.order === 'asc' ? '↑' : '↓'}</span>
									{/if}
								</button>
							</li>
						</ul>
					</div>

					<!-- View mode toggle -->
					<div class="btn-group">
						<button
							class="btn btn-sm lg:btn-md"
							class:btn-active={$fileSortState.viewMode === 'grid'}
							on:click={() => setViewMode('grid')}
							title="Grid view"
						>
							<svg
								xmlns="http://www.w3.org/2000/svg"
								fill="none"
								viewBox="0 0 24 24"
								stroke-width="1.5"
								stroke="currentColor"
								class="w-4 h-4 lg:w-5 lg:h-5"
							>
								<path
									stroke-linecap="round"
									stroke-linejoin="round"
									d="M3.75 6A2.25 2.25 0 016 3.75h2.25A2.25 2.25 0 0110.5 6v2.25a2.25 2.25 0 01-2.25 2.25H6a2.25 2.25 0 01-2.25-2.25V6zM3.75 15.75A2.25 2.25 0 016 13.5h2.25a2.25 2.25 0 012.25 2.25V18a2.25 2.25 0 01-2.25 2.25H6A2.25 2.25 0 013.75 18v-2.25zM13.5 6a2.25 2.25 0 012.25-2.25H18A2.25 2.25 0 0120.25 6v2.25A2.25 2.25 0 0118 10.5h-2.25a2.25 2.25 0 01-2.25-2.25V6zM13.5 15.75a2.25 2.25 0 012.25-2.25H18a2.25 2.25 0 012.25 2.25V18A2.25 2.25 0 0118 20.25h-2.25A2.25 2.25 0 0113.5 18v-2.25z"
								/>
							</svg>
						</button>
						<button
							class="btn btn-sm lg:btn-md"
							class:btn-active={$fileSortState.viewMode === 'list'}
							on:click={() => setViewMode('list')}
							title="List view"
						>
							<svg
								xmlns="http://www.w3.org/2000/svg"
								fill="none"
								viewBox="0 0 24 24"
								stroke-width="1.5"
								stroke="currentColor"
								class="w-4 h-4 lg:w-5 lg:h-5"
							>
								<path
									stroke-linecap="round"
									stroke-linejoin="round"
									d="M3.75 12h16.5m-16.5 3.75h16.5M3.75 19.5h16.5M5.625 4.5h12.75a1.875 1.875 0 010 3.75H5.625a1.875 1.875 0 010-3.75z"
								/>
							</svg>
						</button>
					</div>

					<button
						class="btn btn-outline btn-sm lg:btn-md"
						on:click={() => (showCreateFolderModal = true)}
						disabled={isUploading || selectionMode}
					>
						<svg
							xmlns="http://www.w3.org/2000/svg"
							fill="none"
							viewBox="0 0 24 24"
							stroke-width="1.5"
							stroke="currentColor"
							class="w-4 h-4 lg:w-5 lg:h-5"
						>
							<path
								stroke-linecap="round"
								stroke-linejoin="round"
								d="M12 10.5v6m3-3H9m4.06-7.19l-2.12-2.12a1.5 1.5 0 00-1.061-.44H4.5A2.25 2.25 0 002.25 6v12a2.25 2.25 0 002.25 2.25h15A2.25 2.25 0 0021.75 18V9a2.25 2.25 0 00-2.25-2.25h-5.379a1.5 1.5 0 01-1.06-.44z"
							/>
						</svg>
						<span class="sm:inline hidden">New Folder</span>
					</button>
					<UploadButton
						on:filesSelected={(e) => handleFilesSelected(e.detail)}
						disabled={isUploading || selectionMode}
					/>
				{/if}
			</div>
		</div>

		{#if $filesQuery.isLoading}
			<FileGridSkeleton count={8} />
		{:else if $filesQuery.isError}
			<div class="alert alert-error">
				<svg
					xmlns="http://www.w3.org/2000/svg"
					fill="none"
					viewBox="0 0 24 24"
					class="w-6 h-6 shrink-0 stroke-current"
				>
					<path
						stroke-linecap="round"
						stroke-linejoin="round"
						stroke-width="2"
						d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z"
					></path>
				</svg>
				<span>Failed to load files: {$filesQuery.error?.message}</span>
			</div>
		{:else if $filesQuery.data}
			{#if $searchQuery && filteredFiles.length === 0 && filteredFolders.length === 0}
				<div class="py-16 flex flex-col items-center justify-center text-center">
					<svg
						xmlns="http://www.w3.org/2000/svg"
						fill="none"
						viewBox="0 0 24 24"
						stroke-width="1.5"
						stroke="currentColor"
						class="w-16 h-16 text-base-content/30 mb-4"
					>
						<path
							stroke-linecap="round"
							stroke-linejoin="round"
							d="M21 21l-5.197-5.197m0 0A7.5 7.5 0 105.196 5.196a7.5 7.5 0 0010.607 10.607z"
						/>
					</svg>
					<h3 class="text-lg font-semibold mb-2">No results found</h3>
					<p class="text-base-content/70 mb-4">
						No files or folders match "{$searchQuery}"
					</p>
					<button class="btn btn-sm" on:click={() => searchQuery.set('')}> Clear search </button>
				</div>
			{:else if $fileSortState.viewMode === 'grid'}
				<FileGrid
					folders={sortedFolders}
					files={sortedFiles}
					{replicationStatuses}
					{selectionMode}
					onFolderClick={handleFolderClick}
					onFileClick={handleFileClick}
					onRenameFolder={handleRenameFolder}
					onDeleteFolder={handleDeleteFolder}
					onShareFolder={handleShareFolder}
					onMoveFolder={handleMoveFolder}
					onRenameFile={handleRenameFile}
					onDeleteFile={handleDeleteFile}
					onMoveFile={handleMoveFile}
					onDownloadFile={handleDownloadFile}
					onReplaceFile={handleReplaceFile}
					onShareFile={handleShareFile}
					onVersionHistory={handleVersionHistory}
				/>
			{:else}
				<FileList
					folders={sortedFolders}
					files={sortedFiles}
					{replicationStatuses}
					{selectionMode}
					onFolderClick={handleFolderClick}
					onFileClick={handleFileClick}
					onRenameFolder={handleRenameFolder}
					onDeleteFolder={handleDeleteFolder}
					onShareFolder={handleShareFolder}
					onMoveFolder={handleMoveFolder}
					onRenameFile={handleRenameFile}
					onDeleteFile={handleDeleteFile}
					onMoveFile={handleMoveFile}
					onDownloadFile={handleDownloadFile}
					onReplaceFile={handleReplaceFile}
					onShareFile={handleShareFile}
					onVersionHistory={handleVersionHistory}
				/>
			{/if}
		{/if}
	</div>
</DropZone>

<!-- Upload Progress Panel -->
<UploadProgress tasks={uploadTasks} onClose={handleCloseProgress} />

<!-- Modals -->
<RenameModal
	open={showRenameModal}
	loading={isRenameLoading}
	itemName={renameTarget?.name || ''}
	itemType={renameType}
	on:close={() => {
		showRenameModal = false;
		renameTarget = null;
	}}
	on:confirm={handleRenameConfirm}
/>

<DeleteConfirmation
	open={showDeleteModal}
	loading={isDeleteLoading}
	itemName={deleteTarget?.name || ''}
	itemType={deleteType}
	on:close={() => {
		showDeleteModal = false;
		deleteTarget = null;
	}}
	on:confirm={handleDeleteConfirm}
/>

<MoveModal
	open={showMoveModal}
	loading={isMoveLoading}
	itemName={moveTarget?.name || ''}
	itemType={moveType}
	itemId={moveTarget?.id || null}
	currentFolderId={moveCurrentFolderId}
	on:close={() => {
		showMoveModal = false;
		moveTarget = null;
	}}
	on:confirm={handleMoveConfirm}
/>

<CreateFolderModal
	open={showCreateFolderModal}
	loading={$createFolderMutation.isPending}
	on:close={() => {
		showCreateFolderModal = false;
	}}
	on:confirm={handleCreateFolder}
/>

<ShareModal
	open={showShareModal}
	resourceId={shareTarget?.id || ''}
	resourceName={shareTarget?.name || ''}
	resourceType={shareType}
	on:close={() => {
		showShareModal = false;
		shareTarget = null;
	}}
	on:notification={handleShareNotification}
/>

<VersionHistoryModal
	open={showVersionHistoryModal}
	fileId={versionHistoryTarget?.id || ''}
	fileName={versionHistoryTarget?.name || ''}
	on:close={() => {
		showVersionHistoryModal = false;
		versionHistoryTarget = null;
	}}
	on:restored={handleVersionRestored}
/>

<FilePreviewModal
	open={showFilePreviewModal}
	file={previewTarget}
	on:close={() => {
		showFilePreviewModal = false;
		previewTarget = null;
	}}
/>

<ReplaceFileModal
	open={showReplaceFileModal}
	file={replaceFileTarget}
	on:close={() => {
		showReplaceFileModal = false;
		replaceFileTarget = null;
	}}
	on:success={handleReplaceSuccess}
/>

<!-- Toast Notifications -->
{#if showToast}
	<Toast message={toastMessage} type={toastType} onClose={() => (showToast = false)} />
{/if}
