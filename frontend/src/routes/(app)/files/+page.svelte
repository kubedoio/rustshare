<script lang="ts">
	/**
	 * ==============================================================================
	 * UNIFIED FILES PAGE
	 * ==============================================================================
	 *
	 * Refactored file explorer supporting both "My Files" and "Shared" roots.
	 *
	 * URL Patterns:
	 * - /files                    → My Files root
	 * - /files?root=shared        → Shared root
	 * - /files?folder=<id>        → Specific folder (in my-files root)
	 * - /files?filter=starred     → Starred collection
	 * - /files?filter=recent      → Recent collection
	 * - /files?filter=photos      → Photos collection
	 */

	import { onMount } from 'svelte';
	import { createQuery, createMutation } from '$lib/query-compat';
	import { truncateFilename, formatFileSize } from '$lib/utils/format';
	import { filterUserVisibleEntries } from '$lib/utils/artifactVisibility';
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
		uploadFile,
		listAllFiles,
		getTrashSummary,
		emptyTrash,
		getFile
	} from '$lib/api/files';
	import { createNote } from '$lib/api/notes';
	import { askHref } from '$lib/api/ask';
	import {
		createFolder,
		deleteFolder,
		downloadFolder,
		getFolderContents,
		getFolderTree,
		getSharedFolderContents,
		getSharedFolderTree,
		moveFolder,
		permanentlyDeleteFolder,
		renameFolder,
		restoreFolderFromTrash,
		setFolderStarred,
		type FolderTree as FolderTreeType
	} from '$lib/api/folders';
	import { listReceivedShares } from '$lib/api/shares';
	import type { ReceivedShare } from '$lib/api/types';
	import {
		extractFolderPaths,
		sortFolderPaths,
		type DirectoryUploadItem
	} from '$lib/utils/directoryUpload';
	import { queryClient } from '$lib/query-client';
	import { searchQuery } from '$lib/stores/search';
	import { fileSortState, setSortField, setPageSize } from '$lib/stores/fileSort';
	import { selectionStore, selectionCount, hasSelection } from '$lib/stores/selection';
	import { activityStore } from '$lib/stores/activity';

	import { folderTreeStore } from '$lib/stores/folderTree';
	import type { File, Folder, FolderContents as ApiFolderContents } from '$lib/api/types';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { navigateToNote } from '$lib/navigation/artifactRoutes';

	// Explorer types
	import type { ExplorerRoot, CollectionView } from '$lib/explorer';
	import { ROOT_CONFIG } from '$lib/explorer';
	import { findFolderPathInTree, findFolderPathInSharedTrees } from '$lib/explorer/breadcrumbs';

	// Components
	import FileBrowserToolbar from '$lib/explorer/FileBrowserToolbar.svelte';
	import FileBrowserContent from '$lib/explorer/FileBrowserContent.svelte';
	import DropZone from '$lib/components/files/DropZone.svelte';
	import UploadProgress from '$lib/components/files/UploadProgress.svelte';
	import PaginationControls from '$lib/components/common/PaginationControls.svelte';
	import Toast from '$lib/components/common/Toast.svelte';

	import { detectEditorType } from '$lib/utils/editor';
	import FileModals from './FileModals.svelte';
	import FileEditorPane from './FileEditorPane.svelte';
	import EmptyTrashModal from '$lib/components/modals/EmptyTrashModal.svelte';
	import ConfirmModal from '$lib/components/common/ConfirmModal.svelte';
	import OfflineBanner from '$lib/components/common/OfflineBanner.svelte';

	// ============================================================================
	// STATE
	// ============================================================================

	type UploadTask = {
		id: string;
		fileName: string;
		size: number;
		status: 'pending' | 'uploading' | 'success' | 'error';
		progress: number;
		error?: string;
		previewUrl?: string;
	};

	type WorkspaceMode = 'all' | 'photos' | 'recent' | 'starred' | 'deleted' | 'week';

	let uploadTasks = $state<UploadTask[]>([]);
	let selectionMode = $state(false);

	// Modal states
	let showRenameModal = $state(false);
	let showDeleteModal = $state(false);
	let showMoveModal = $state(false);
	let showShareModal = $state(false);
	let showCreateFolderModal = $state(false);
	let showVersionHistoryModal = $state(false);
	let showFilePreviewModal = $state(false);
	let showReplaceFileModal = $state(false);
	let bulkMoveFileIds = $state<string[]>([]);
	let bulkMoveLoading = $state(false);
	let showCreateFileModal = $state(false);
	let showUploadTargetModal = $state(false);
	let showEditFileModal = $state(false);
	let createFileLoading = $state(false);
	let uploadTargetFolderId = $state<string | null>(null);
	let editableFilesForModal = $state<File[]>([]);

	// Trash state
	let showEmptyTrashModal = $state(false);
	let trashSummary = $state({ file_count: 0, folder_count: 0, total_size: 0 });
	let emptyingTrash = $state(false);

	// Confirm modal state
	let showConfirmModal = $state(false);
	let confirmTitle = $state('');
	let confirmMessage = $state('');
	let confirmDanger = $state(false);
	let confirmOnConfirm = $state(() => {});

	// Editor state
	let showTextEditor = $state(false);
	let showMarkdownEditor = $state(false);
	let showExcalidrawEditor = $state(false);
	let editorTarget = $state<File | null>(null);

	// Targets
	let renameTarget = $state<File | Folder | null>(null);
	let renameType = $state<'file' | 'folder'>('file');
	let deleteTarget = $state<File | Folder | null>(null);
	let deleteType = $state<'file' | 'folder'>('file');
	let moveTarget = $state<File | Folder | null>(null);
	let moveType = $state<'file' | 'folder'>('file');
	let shareTarget = $state<File | Folder | null>(null);
	let shareType = $state<'file' | 'folder'>('file');
	let versionHistoryTarget = $state<File | null>(null);
	let previewTarget = $state<File | null>(null);
	let replaceFileTarget = $state<File | null>(null);

	// Toast
	let showToast = $state(false);
	let toastMessage = $state('');
	let toastType = $state<'success' | 'error' | 'info'>('info');

	// ============================================================================
	// EXPLORER STATE DERIVATIONS
	// ============================================================================

	// URL parameters
	let urlFolderId = $derived($page.url.searchParams.get('folder'));
	let urlFilter = $derived($page.url.searchParams.get('filter'));
	let urlRoot = $derived($page.url.searchParams.get('root') as ExplorerRoot | null);

	// Helper to check if a string looks like a valid UUID
	function isValidUuid(value: string | null): value is string {
		if (!value) return false;
		const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
		return uuidPattern.test(value);
	}

	// Current workspace mode
	let workspaceMode = $derived(
		(urlFilter === 'photos'
			? 'photos'
			: urlFilter === 'starred'
				? 'starred'
				: urlFilter === 'deleted'
					? 'deleted'
					: urlFilter === 'recent'
						? 'recent'
						: 'all') as WorkspaceMode
	);

	// Active root (my-files or shared)
	let activeRoot = $derived((urlRoot === 'shared' ? 'shared' : 'my-files') as ExplorerRoot);

	// Is in collection mode?
	let isCollectionMode = $derived(
		workspaceMode === 'starred' ||
			workspaceMode === 'recent' ||
			workspaceMode === 'week' ||
			workspaceMode === 'photos' ||
			workspaceMode === 'deleted'
	);

	// Current folder ID (null at root)
	let currentFolderId = $derived(
		isCollectionMode ? null : isValidUuid(urlFolderId) ? urlFolderId : null
	);

	// Is shared root view?
	let isSharedRoot = $derived(activeRoot === 'shared' && !currentFolderId);

	// ============================================================================
	// QUERIES
	// ============================================================================

	// Query for folder tree (used for breadcrumb path in my-files)
	const folderTreeQuery = createQuery<FolderTreeType>({
		queryKey: ['folder-tree'],
		queryFn: () => getFolderTree(),
		staleTime: 0
	});

	// Query for received shares (for building shared folder tree)
	const receivedSharesQuery = createQuery<ReceivedShare[]>({
		queryKey: ['received-shares'],
		queryFn: () => listReceivedShares(),
		enabled: true
	});

	const sharedFolderTreesQuery = createQuery<FolderTreeType[]>({
		queryKey: [
			'shared-folder-trees',
			($receivedSharesQuery.data || [])
				.filter((share) => share.resource_type === 'folder')
				.map((share) => share.resource_id)
				.sort()
				.join(',')
		],
		queryFn: async () => {
			const folderShares = ($receivedSharesQuery.data || []).filter(
				(share) => share.resource_type === 'folder'
			);
			return Promise.all(folderShares.map((share) => getSharedFolderTree(share.resource_id)));
		},
		enabled:
			activeRoot === 'shared' &&
			!!$receivedSharesQuery.data &&
			$receivedSharesQuery.data.some((share) => share.resource_type === 'folder')
	});

	// Query for the active workspace view
	async function fetchWorkspaceContents() {
		if (workspaceMode === 'starred') return getStarredContents();
		if (workspaceMode === 'deleted') return getDeletedContents();
		if (workspaceMode === 'recent') {
			const allFiles = await listAllFiles();
			return { folders: [], files: allFiles.slice(0, 30) };
		}
		if (workspaceMode === 'week') {
			const allFiles = await listAllFiles();
			const weekAgo = new Date(Date.now() - 7 * 24 * 60 * 60 * 1000);
			const weekFiles = allFiles
				.filter((f) => new Date(f.modified_at) >= weekAgo)
				.toSorted((a, b) => new Date(b.modified_at).getTime() - new Date(a.modified_at).getTime());
			return { folders: [], files: weekFiles };
		}

		// For shared root
		if (activeRoot === 'shared') {
			if (currentFolderId) {
				// Inside a specific shared folder
				return getSharedFolderContents(currentFolderId);
			} else {
				// At shared root - show list of received shares
				const shares = await listReceivedShares();
				// Transform shares into FolderContents format
				return {
					folders: shares
						.filter((s) => s.resource_type === 'folder')
						.map((s) => ({
							id: s.resource_id,
							name: s.resource_name,
							path: s.resource_path,
							parent_folder_id: null,
							owner_id: s.shared_by,
							created_at: s.created_at,
							updated_at: s.created_at,
							is_shared: true,
							share_count: 1,
							effective_permission: s.permission
						})),
					files: shares
						.filter((s) => s.resource_type === 'file')
						.map((s) => ({
							id: s.resource_id,
							name: s.resource_name,
							path: s.resource_path,
							size: 0,
							mime_type: 'application/octet-stream',
							parent_folder_id: null,
							owner_id: s.shared_by,
							current_version: 1,
							created_at: s.created_at,
							modified_at: s.created_at,
							is_shared: true,
							share_count: 1,
							effective_permission: s.permission
						}))
				};
			}
		}

		// Default my-files behavior
		return getFolderContents(currentFolderId);
	}

	const filesQuery = createQuery<ApiFolderContents>({
		queryKey: ['file-workspace', workspaceMode, currentFolderId, activeRoot],
		queryFn: fetchWorkspaceContents
	});

	$effect(() => {
		filesQuery.setOptions({
			queryKey: ['file-workspace', workspaceMode, currentFolderId, activeRoot],
			queryFn: fetchWorkspaceContents
		});
	});

	// All files query (for storage stats)
	const allFilesQuery = createQuery({
		queryKey: ['all-files'],
		queryFn: () => listAllFiles()
	});

	// ============================================================================
	// BREADCRUMB & PATH DERIVATION
	// ============================================================================

	// Derive folderPath from the full API folder tree (for my-files)
	let myFilesFolderPath = $derived(
		currentFolderId && $folderTreeQuery.data && activeRoot === 'my-files'
			? findFolderPathInTree($folderTreeQuery.data, currentFolderId).slice(1)
			: []
	);
	let sharedFolderPath = $derived(
		currentFolderId && activeRoot === 'shared' && $sharedFolderTreesQuery.data
			? findFolderPathInSharedTrees(currentFolderId, $sharedFolderTreesQuery.data)
			: []
	);

	// Build breadcrumb based on current state
	// Returns Folder-compatible objects for FileExplorer component
	let breadcrumbPath = $derived(
		((): Folder[] => {
			// In collection mode, return empty (no breadcrumb)
			if (isCollectionMode) {
				return [];
			}

			if (activeRoot === 'my-files') {
				// Return my-files path (already Folder objects)
				return myFilesFolderPath;
			} else if (activeRoot === 'shared' && currentFolderId) {
				return sharedFolderPath;
			}

			return [];
		})()
	);

	function permissionLevel(permission: 'View' | 'Edit' | 'Admin' | null | undefined): number {
		if (permission === 'Admin') return 3;
		if (permission === 'Edit') return 2;
		if (permission === 'View') return 1;
		return 0;
	}

	let currentSharedFolderPermission = $derived(
		activeRoot === 'shared' ? ($filesQuery.data?.current_folder_permission ?? null) : null
	);
	let hasSharedWritePermission = $derived(permissionLevel(currentSharedFolderPermission) >= 2);

	// ============================================================================
	// TITLE DERIVATION (Contextual Header)
	// ============================================================================

	let workspaceTitle = $derived(
		isCollectionMode
			? workspaceMode === 'photos'
				? 'Photos'
				: workspaceMode === 'recent'
					? 'Recent'
					: workspaceMode === 'week'
						? 'Updated This Week'
						: workspaceMode === 'starred'
							? 'Starred'
							: 'Trash'
			: activeRoot === 'shared'
				? currentFolderId
					? breadcrumbPath[breadcrumbPath.length - 1]?.name
					: 'Shared'
				: currentFolderId
					? breadcrumbPath[breadcrumbPath.length - 1]?.name
					: 'My Files'
	);

	let workspaceDescription = $derived(
		isCollectionMode
			? workspaceMode === 'photos'
				? 'Image files in the current workspace, without the folder noise.'
				: workspaceMode === 'recent'
					? 'The latest created files in this workspace, sorted by newest first.'
					: workspaceMode === 'week'
						? 'Files and artifacts updated within the last 7 days, sorted by latest first.'
						: workspaceMode === 'starred'
							? 'Pinned folders and files that need fast access without digging through the tree.'
							: 'Recently deleted items live here until you restore them or remove them permanently.'
			: activeRoot === 'shared'
				? currentFolderId
					? 'Shared folder contents.'
					: 'Folders shared with you by other users.'
				: currentFolderId
					? 'Folder contents.'
					: 'Browse and organize files, folders, and workspace artifacts.'
	);

	let workspaceEmptyTitle = $derived(
		isCollectionMode
			? workspaceMode === 'photos'
				? 'No photos in this view'
				: workspaceMode === 'recent'
					? 'No files created yet'
					: workspaceMode === 'week'
						? 'No updates this week'
						: workspaceMode === 'starred'
							? 'Nothing is starred yet'
							: 'Deleted items will show up here'
			: activeRoot === 'shared'
				? 'No shared folders'
				: 'No files yet'
	);

	let workspaceEmptyDescription = $derived(
		isCollectionMode
			? workspaceMode === 'photos'
				? 'Upload an image into this folder and it will show up here.'
				: workspaceMode === 'recent'
					? 'Create or upload a file and it will show up here.'
					: workspaceMode === 'week'
						? 'Files updated in the last 7 days will appear here.'
						: workspaceMode === 'starred'
							? 'Star a folder or file from its action menu and it will show up here.'
							: 'Deleting a folder or file moves it here instead of removing it immediately.'
			: activeRoot === 'shared'
				? 'Items shared with you will appear here.'
				: 'This folder is empty. Upload a file or create a folder to start organizing your workspace.'
	);

	let workspaceEmptyActionLabel = $derived(
		!isCollectionMode && activeRoot === 'my-files' ? 'Upload files' : null
	);

	// ============================================================================
	// UI STATE DERIVATIONS
	// ============================================================================

	let showFolderTree = $derived(!isCollectionMode);
	let showBreadcrumbs = $derived(!isCollectionMode);
	let canCreateFolder = $derived(
		!isCollectionMode &&
			(activeRoot === 'my-files' || (activeRoot === 'shared' && hasSharedWritePermission))
	);
	let canUpload = $derived(
		!isCollectionMode &&
			(activeRoot === 'my-files' || (activeRoot === 'shared' && hasSharedWritePermission))
	);
	let allowSelectionMode = $derived(workspaceMode !== 'deleted');

	$effect(() => {
		if (!allowSelectionMode && selectionMode) {
			selectionMode = false;
			selectionStore.clear();
		}
	});

	// ============================================================================
	// SORTING & FILTERING
	// ============================================================================

	let activeSortField = $derived(
		workspaceMode === 'recent'
			? 'created_at'
			: workspaceMode === 'week'
				? 'modified_at'
				: $fileSortState.field
	);
	let activeSortOrder = $derived(
		workspaceMode === 'recent' || workspaceMode === 'week' ? 'desc' : $fileSortState.order
	);
	let searchTerm = $derived($searchQuery.trim().toLowerCase());

	function matchesSearch(name: string) {
		return searchTerm.length === 0 || name.toLowerCase().includes(searchTerm);
	}

	let baseFolders = $derived(
		filterUserVisibleEntries(
			($filesQuery.data?.folders || []).filter((folder) => matchesSearch(folder.name))
		)
	);
	let baseFiles = $derived(
		filterUserVisibleEntries(
			($filesQuery.data?.files || []).filter((file) => matchesSearch(file.name))
		)
	);

	let filteredFolders = $derived(
		workspaceMode === 'all' || workspaceMode === 'starred' || workspaceMode === 'deleted'
			? baseFolders
			: []
	);
	let filteredFiles = $derived(
		workspaceMode === 'photos'
			? baseFiles.filter((file) => file.mime_type.startsWith('image/'))
			: baseFiles
	);

	let sortedFolders = $derived(
		[...filteredFolders].sort((a, b) => {
			if (activeSortField === 'name') {
				return activeSortOrder === 'asc'
					? a.name.localeCompare(b.name)
					: b.name.localeCompare(a.name);
			}
			if (activeSortField === 'modified_at') {
				return activeSortOrder === 'asc'
					? new Date(a.updated_at).getTime() - new Date(b.updated_at).getTime()
					: new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime();
			}
			return 0;
		})
	);

	let sortedFiles = $derived(
		[...filteredFiles].sort((a, b) => {
			if (activeSortField === 'name') {
				return activeSortOrder === 'asc'
					? a.name.localeCompare(b.name)
					: b.name.localeCompare(a.name);
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
				return activeSortOrder === 'asc'
					? a.mime_type.localeCompare(b.mime_type)
					: b.mime_type.localeCompare(a.mime_type);
			}
			return 0;
		})
	);

	// ============================================================================
	// PAGINATION
	// ============================================================================

	let currentPage = $state(1);

	$effect(() => {
		// Reset to page 1 when page size or sort changes
		const _ = $fileSortState.pageSize;
		const __ = activeSortField;
		const ___ = activeSortOrder;
		currentPage = 1;
	});

	let pageSize = $derived($fileSortState.pageSize);
	let startIndex = $derived((currentPage - 1) * pageSize);
	let folderEndIndex = $derived(Math.min(startIndex + pageSize, sortedFolders.length));
	let paginatedFolders = $derived(sortedFolders.slice(startIndex, folderEndIndex));
	let fileStartIndex = $derived(Math.max(0, startIndex - sortedFolders.length));
	let fileEndIndex = $derived(Math.max(0, startIndex + pageSize - sortedFolders.length));
	let paginatedFiles = $derived(sortedFiles.slice(fileStartIndex, fileEndIndex));
	let totalItems = $derived(sortedFolders.length + sortedFiles.length);
	let totalPages = $derived(Math.max(1, Math.ceil(totalItems / pageSize)));

	// ============================================================================
	// MUTATIONS
	// ============================================================================

	const uploadMutation = createMutation({
		mutationFn: ({
			file,
			folderId,
			onProgress
		}: {
			file: globalThis.File;
			folderId?: string | null;
			onProgress?: (progress: number) => void;
		}) => uploadFile(folderId === undefined ? currentFolderId : folderId, file, onProgress),
		onSuccess: (_, { file }) => {
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['all-files'] });
			activityStore.addActivity('file_uploaded', file.name);
		}
	});

	const createNoteMutation = createMutation({
		mutationFn: ({
			title,
			content,
			parent_folder_id
		}: {
			title: string;
			content: string;
			parent_folder_id: string | null;
		}) => createNote({ title, content, parent_folder_id }),
		onSuccess: (data) => {
			activityStore.addActivity('note_created', data.name || 'Untitled Note', {
				artifactId: data.id,
				applicationId: 'io.elembra.notes'
			});
			navigateToNote(data.id, getFilesReturnUrl());
		}
	});

	const createFolderMutation = createMutation({
		mutationFn: ({ name, parentFolderId }: { name: string; parentFolderId: string | null }) =>
			createFolder(name, parentFolderId),
		onSuccess: (folder, { parentFolderId }) => {
			folderTreeStore.addFolder(folder, parentFolderId);
			if (parentFolderId) {
				folderTreeStore.setExpanded(parentFolderId, true);
			}
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['folder-tree'] });
			queryClient.invalidateQueries({ queryKey: ['all-files'] });
			showCreateFolderModal = false;
			showNotification('Folder created', 'success');
			activityStore.addActivity('folder_created', folder.name);
		},
		onError: (error) => {
			showNotification(error instanceof Error ? error.message : 'Failed to create folder', 'error');
		}
	});

	const renameFileMutation = createMutation({
		mutationFn: ({ fileId, newName }: { fileId: string; newName: string }) =>
			renameFile(fileId, newName),
		onSuccess: (_, { newName }) => {
			const oldName = renameTarget?.name || 'File';
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['all-files'] });
			showRenameModal = false;
			renameTarget = null;
			showNotification(`${truncateFilename(newName)} renamed`, 'success');
			activityStore.addActivity('file_renamed', newName, { details: oldName });
		}
	});

	const renameFolderMutation = createMutation({
		mutationFn: ({ folderId, newName }: { folderId: string; newName: string }) =>
			renameFolder(folderId, newName),
		onSuccess: (_, { folderId, newName }) => {
			const oldName = renameTarget?.name || 'Folder';
			folderTreeStore.updateFolderName(folderId, newName);
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['folder-tree'] });
			queryClient.invalidateQueries({ queryKey: ['all-files'] });
			showRenameModal = false;
			renameTarget = null;
			showNotification('Folder renamed', 'success');
			activityStore.addActivity('folder_renamed', newName, { details: oldName });
		}
	});

	const deleteFileMutation = createMutation({
		mutationFn: (fileId: string) => deleteFile(fileId),
		onSuccess: (_, fileId) => {
			const fileName = deleteTarget?.name || 'File';
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['all-files'] });
			showDeleteModal = false;
			deleteTarget = null;
			showNotification(`${truncateFilename(fileName)} moved to deleted`, 'success');
			activityStore.addActivity('file_deleted', fileName);
		}
	});

	const deleteFolderMutation = createMutation({
		mutationFn: (folderId: string) => deleteFolder(folderId),
		onSuccess: (_, folderId) => {
			const folderName = deleteTarget?.name || 'Folder';
			folderTreeStore.removeFolder(folderId);
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['folder-tree'] });
			queryClient.invalidateQueries({ queryKey: ['all-files'] });
			if (
				deleteTarget &&
				(currentFolderId === deleteTarget.id ||
					breadcrumbPath.some((f) => f.id === deleteTarget?.id))
			) {
				goto('/files', { replaceState: true });
			}
			showDeleteModal = false;
			deleteTarget = null;
			showNotification('Folder moved to deleted', 'success');
			activityStore.addActivity('folder_deleted', folderName);
		}
	});

	const moveFileMutation = createMutation({
		mutationFn: ({ fileId, targetFolderId }: { fileId: string; targetFolderId: string | null }) =>
			moveFile(fileId, targetFolderId),
		onSuccess: () => {
			const fileName = moveTarget?.name || 'File';
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['folder-tree'] });
			queryClient.invalidateQueries({ queryKey: ['all-files'] });
			showMoveModal = false;
			moveTarget = null;
			showNotification(`${truncateFilename(fileName)} moved`, 'success');
			activityStore.addActivity('file_moved', fileName);
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
			const folderName = moveTarget?.name || 'Folder';
			folderTreeStore.moveFolder(folderId, targetFolderId);
			if (targetFolderId) {
				folderTreeStore.setExpanded(targetFolderId, true);
			}
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['folder-tree'] });
			queryClient.invalidateQueries({ queryKey: ['all-files'] });
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
			queryClient.invalidateQueries({ queryKey: ['all-files'] });
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
			queryClient.invalidateQueries({ queryKey: ['all-files'] });
			showNotification(
				variables.starred ? 'Folder added to starred' : 'Folder removed from starred',
				'success'
			);
		}
	});

	const restoreFileMutation = createMutation({
		mutationFn: ({ fileId, fileName }: { fileId: string; fileName: string }) =>
			restoreFileFromTrash(fileId),
		onSuccess: (_, { fileName }) => {
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['all-files'] });
			showNotification(`${truncateFilename(fileName)} restored`, 'success');
		}
	});

	const restoreFolderMutation = createMutation({
		mutationFn: (folderId: string) => restoreFolderFromTrash(folderId),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['all-files'] });
			showNotification('Folder restored', 'success');
		}
	});

	const permanentlyDeleteFileMutation = createMutation({
		mutationFn: ({ fileId, fileName }: { fileId: string; fileName: string }) =>
			permanentlyDeleteFile(fileId),
		onSuccess: (_, { fileName }) => {
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['all-files'] });
			showNotification(`${truncateFilename(fileName)} deleted permanently`, 'success');
		}
	});

	const permanentlyDeleteFolderMutation = createMutation({
		mutationFn: (folderId: string) => permanentlyDeleteFolder(folderId),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['all-files'] });
			showNotification('Folder deleted permanently', 'success');
		}
	});

	const emptyTrashMutation = createMutation({
		mutationFn: () => emptyTrash(),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['all-files'] });
			showNotification('Trash emptied successfully', 'success');
			showEmptyTrashModal = false;
		},
		onError: (error: Error) => {
			showNotification(error.message || 'Failed to empty trash', 'error');
			emptyingTrash = false;
		}
	});

	// ============================================================================
	// NAVIGATION HANDLERS
	// ============================================================================

	function getFilesReturnUrl() {
		return $page.url.pathname + $page.url.search;
	}

	function handleFolderClick(folder: Folder) {
		if (activeRoot === 'shared') {
			// For shared folders, use the shared root navigation
			goto(`/files?folder=${folder.id}&root=shared`, { replaceState: true });
		} else {
			// Default my-files navigation
			goto(`/files?folder=${folder.id}`, { replaceState: true });
		}
	}

	function handleBreadcrumbNavigate(event: CustomEvent<{ folderId: string | null }>) {
		const targetId = event.detail.folderId;
		if (targetId === null) {
			// Navigate to root of current root
			if (activeRoot === 'shared') {
				goto('/files?root=shared', { replaceState: true });
			} else {
				goto('/files', { replaceState: true });
			}
		} else {
			if (activeRoot === 'shared') {
				goto(`/files?folder=${targetId}&root=shared`, { replaceState: true });
			} else {
				goto(`/files?folder=${targetId}`, { replaceState: true });
			}
		}
	}

	function handleFileClick(file: File) {
		if (workspaceMode === 'deleted') return;
		if (detectEditorType(file.name, file.mime_type) === 'markdown') {
			navigateToNote(file.id, getFilesReturnUrl());
			return;
		}
		previewTarget = file;
		showFilePreviewModal = true;
	}

	// Deep-link support: ?preview=<id> (Ask citations, dashboard, activity,
	// Topbar search). Resolve the file and open it the same way an in-page click
	// would. The param is stripped so the note editor's return link and later
	// in-page navigation don't re-trigger this; the guard compares against the
	// cleaned URL so re-clicking the same result still re-opens it.
	let lastHandledPreviewUrl: string | null = null;

	$effect(() => {
		const url = $page.url;
		const previewId = url.searchParams.get('preview');
		if (!previewId) return;
		const rawUrl = url.pathname + url.search;
		if (rawUrl === lastHandledPreviewUrl) return;

		const params = new URLSearchParams(url.search);
		params.delete('preview');
		const cleanUrl = `${url.pathname}${params.size ? `?${params.toString()}` : ''}`;
		lastHandledPreviewUrl = cleanUrl;
		if (cleanUrl !== rawUrl) goto(cleanUrl, { replaceState: true });
		if (!isValidUuid(previewId)) return;

		getFile(previewId)
			.then((file) => {
				if (detectEditorType(file.name, file.mime_type) === 'markdown') {
					navigateToNote(file.id, cleanUrl);
				} else {
					previewTarget = file;
					showFilePreviewModal = true;
				}
			})
			.catch(() => {
				toastMessage = 'That file could not be opened.';
				showToast = true;
				toastType = 'error';
			});
	});

	// ============================================================================
	// OTHER HANDLERS (unchanged from original)
	// ============================================================================

	function handleEditFile(event: { file: File } | File) {
		const file = 'file' in event ? event.file : event;
		if (detectEditorType(file.name, file.mime_type) === 'markdown') {
			navigateToNote(file.id, getFilesReturnUrl());
			return;
		}
		editorTarget = file;
		showFilePreviewModal = false;

		const editorType = detectEditorType(file.name, file.mime_type);
		switch (editorType) {
			case 'excalidraw':
				showExcalidrawEditor = true;
				break;
			case 'text':
			default:
				showTextEditor = true;
				break;
		}
	}

	function handleEditorClose() {
		showTextEditor = false;
		showMarkdownEditor = false;
		showExcalidrawEditor = false;
		editorTarget = null;
	}

	function handleEditorSaved(event?: any) {
		const targetId = event?.detail?.file?.id || editorTarget?.id;
		queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
		queryClient.invalidateQueries({ queryKey: ['all-files'] });
		if (targetId) {
			queryClient.removeQueries({ queryKey: ['file', targetId] });
			queryClient.removeQueries({ queryKey: ['file-versions', targetId] });
		}
		if (editorTarget) {
			showNotification(`${truncateFilename(editorTarget.name)} saved`, 'success');
		} else {
			showNotification('File saved', 'success');
		}
		handleEditorClose();
	}

	function showNotification(message: string, type: 'success' | 'error' | 'info') {
		toastMessage = message;
		toastType = type;
		showToast = true;
	}

	async function handleFilesSelected(files: globalThis.File[]) {
		if (!canUpload) return;
		if (files.length === 0) return;

		const newTasks: UploadTask[] = files.map((file) => ({
			id: `${file.name}-${Date.now()}-${Math.random()}`,
			fileName: file.name,
			size: file.size,
			status: 'pending' as const,
			progress: 0
		}));

		uploadTasks = [...uploadTasks, ...newTasks];

		let successCount = 0;
		let errorCount = 0;

		for (let i = 0; i < files.length; i++) {
			const taskIndex = uploadTasks.findIndex((t) => t.id === newTasks[i].id);
			if (taskIndex === -1) continue;

			try {
				await $uploadMutation.mutateAsync({
					file: files[i],
					folderId: uploadTargetFolderId ?? currentFolderId,
					onProgress: (progress) => {
						const currentTaskIndex = uploadTasks.findIndex((t) => t.id === newTasks[i].id);
						if (currentTaskIndex !== -1) {
							uploadTasks[currentTaskIndex].status = 'uploading';
							uploadTasks[currentTaskIndex].progress = progress;
							uploadTasks = [...uploadTasks];
						}
					}
				});

				const finalTaskIndex = uploadTasks.findIndex((t) => t.id === newTasks[i].id);
				if (finalTaskIndex !== -1) {
					uploadTasks[finalTaskIndex].status = 'success';
					uploadTasks[finalTaskIndex].progress = 100;
					uploadTasks = [...uploadTasks];
				}
				successCount++;
			} catch (error) {
				const errorMessage = error instanceof Error ? error.message : 'Upload failed';
				const errorTaskIndex = uploadTasks.findIndex((t) => t.id === newTasks[i].id);
				if (errorTaskIndex !== -1) {
					uploadTasks[errorTaskIndex].status = 'error';
					uploadTasks[errorTaskIndex].error = errorMessage;
					uploadTasks = [...uploadTasks];
				}
				errorCount++;
			}
		}

		if (errorCount === 0) {
			if (successCount === 1) {
				const uploadedFile = uploadTasks.find((t) => t.id === newTasks[0].id);
				const filename = uploadedFile ? uploadedFile.fileName : '';
				showNotification(`${truncateFilename(filename)} uploaded`, 'success');
			} else {
				showNotification(`${successCount} item(s) uploaded`, 'success');
			}
		} else if (successCount === 0) {
			showNotification(`Failed to upload ${errorCount} file(s)`, 'error');
		} else {
			showNotification(`Uploaded ${successCount}, failed ${errorCount}`, 'info');
		}

		// Reset upload target folder after uploads complete
		uploadTargetFolderId = null;
	}

	async function handleDirectoryUpload(items: DirectoryUploadItem[]) {
		if (!canUpload || items.length === 0) return;

		const baseFolderId = uploadTargetFolderId ?? currentFolderId;

		const folderPaths = extractFolderPaths(items);
		const sortedPaths = sortFolderPaths(folderPaths);

		const folderIdMap = new Map<string, string>();
		const failedFolderPaths = new Set<string>();

		for (const path of sortedPaths) {
			const parentPath = path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '';
			if (parentPath && failedFolderPaths.has(parentPath)) {
				failedFolderPaths.add(path);
				continue;
			}

			const folderName = path.slice(path.lastIndexOf('/') + 1);
			if (parentPath && !folderIdMap.has(parentPath)) {
				failedFolderPaths.add(path);
				continue;
			}
			const parentId = parentPath ? folderIdMap.get(parentPath)! : baseFolderId;

			try {
				const contents = await getFolderContents(parentId);
				const existing = contents.folders.find((f) => f.name === folderName);

				if (existing) {
					folderIdMap.set(path, existing.id);
				} else {
					const created = await createFolder(folderName, parentId);
					folderIdMap.set(path, created.id);
					folderTreeStore.addFolder(created, parentId);
					if (parentId) {
						folderTreeStore.setExpanded(parentId, true);
					}
				}
			} catch (error) {
				showNotification(`Failed to create folder "${path}"`, 'error');
				failedFolderPaths.add(path);
			}
		}

		const filesToUpload: { file: globalThis.File; parentFolderId: string | null }[] = [];
		for (const { file, relativePath } of items) {
			const lastSlash = relativePath.lastIndexOf('/');

			if (lastSlash > 0) {
				const folderPath = relativePath.slice(0, lastSlash);
				if (failedFolderPaths.has(folderPath)) continue;
				const parentId = folderIdMap.get(folderPath) ?? null;
				filesToUpload.push({ file, parentFolderId: parentId });
			} else {
				filesToUpload.push({ file, parentFolderId: baseFolderId });
			}
		}

		if (filesToUpload.length === 0) {
			showNotification('No files could be uploaded', 'error');
			return;
		}

		const newTasks: UploadTask[] = filesToUpload.map(({ file }) => ({
			id: `${file.name}-${Date.now()}-${Math.random()}`,
			fileName: file.name,
			size: file.size,
			status: 'pending' as const,
			progress: 0
		}));

		uploadTasks = [...uploadTasks, ...newTasks];

		let successCount = 0;
		let errorCount = 0;

		for (let i = 0; i < filesToUpload.length; i++) {
			const { file, parentFolderId } = filesToUpload[i];
			const taskId = newTasks[i].id;
			const taskIndex = uploadTasks.findIndex((t) => t.id === taskId);
			if (taskIndex === -1) continue;

			try {
				await $uploadMutation.mutateAsync({
					file,
					folderId: parentFolderId,
					onProgress: (progress) => {
						const currentTaskIndex = uploadTasks.findIndex((t) => t.id === taskId);
						if (currentTaskIndex !== -1) {
							uploadTasks[currentTaskIndex].status = 'uploading';
							uploadTasks[currentTaskIndex].progress = progress;
							uploadTasks = [...uploadTasks];
						}
					}
				});

				const finalTaskIndex = uploadTasks.findIndex((t) => t.id === taskId);
				if (finalTaskIndex !== -1) {
					uploadTasks[finalTaskIndex].status = 'success';
					uploadTasks[finalTaskIndex].progress = 100;
					uploadTasks = [...uploadTasks];
				}
				successCount++;
			} catch (error) {
				const errorMessage = error instanceof Error ? error.message : 'Upload failed';
				const errorTaskIndex = uploadTasks.findIndex((t) => t.id === taskId);
				if (errorTaskIndex !== -1) {
					uploadTasks[errorTaskIndex].status = 'error';
					uploadTasks[errorTaskIndex].error = errorMessage;
					uploadTasks = [...uploadTasks];
				}
				errorCount++;
			}
		}

		if (errorCount === 0) {
			showNotification(`${successCount} item(s) uploaded`, 'success');
		} else if (successCount === 0) {
			showNotification(`Failed to upload ${errorCount} file(s)`, 'error');
		} else {
			showNotification(`Uploaded ${successCount}, failed ${errorCount}`, 'info');
		}

		queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
		queryClient.invalidateQueries({ queryKey: ['folder-tree'] });
		queryClient.invalidateQueries({ queryKey: ['all-files'] });
		uploadTargetFolderId = null;
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
		selectionStore.selectAll(paginatedFiles, paginatedFolders);
	}

	function handleDeselectAll() {
		selectionStore.deselectAll();
	}

	async function handleBulkDownload() {
		const selectedFileIds = new Set($selectionStore.selectedFileIds);
		const selectedFolderIds = new Set($selectionStore.selectedFolderIds);
		const selectedFiles = sortedFiles.filter((file) => selectedFileIds.has(file.id));
		const selectedFolders = sortedFolders.filter((folder) => selectedFolderIds.has(folder.id));

		if (selectedFiles.length === 0 && selectedFolders.length === 0) {
			showNotification('Select at least one item to download', 'info');
			return;
		}

		let fileSuccessCount = 0;
		let folderSuccessCount = 0;

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
				fileSuccessCount += 1;
			} catch (error) {
				console.error('Failed to download selected file:', file.name, error);
			}
		}

		for (const folder of selectedFolders) {
			try {
				const blob = await downloadFolder(folder.id);
				const url = URL.createObjectURL(blob);
				const a = document.createElement('a');
				a.href = url;
				a.download = `${folder.name}.zip`;
				document.body.appendChild(a);
				a.click();
				document.body.removeChild(a);
				URL.revokeObjectURL(url);
				activityStore.addActivity('folder_downloaded', folder.name);
				folderSuccessCount += 1;
			} catch (error) {
				console.error('Failed to download selected folder:', folder.name, error);
			}
		}

		const totalSuccess = fileSuccessCount + folderSuccessCount;
		const totalSelected = selectedFiles.length + selectedFolders.length;

		if (totalSuccess === 0) {
			showNotification('Failed to start the selected downloads', 'error');
			return;
		}

		const parts: string[] = [];
		if (fileSuccessCount > 0) {
			parts.push(`Started ${fileSuccessCount} file download${fileSuccessCount === 1 ? '' : 's'}`);
		}
		if (folderSuccessCount > 0) {
			parts.push(
				`Started ${folderSuccessCount} folder download${folderSuccessCount === 1 ? '' : 's'}`
			);
		}
		if (totalSuccess < totalSelected) {
			parts.push(`${totalSelected - totalSuccess} failed`);
		}

		showNotification(parts.join(', '), totalSuccess < totalSelected ? 'info' : 'success');
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
			showNotification(
				'Bulk move currently supports files only. Deselect folders and try again.',
				'info'
			);
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

		confirmTitle = 'Delete Items';
		confirmMessage = `Delete ${fileIds.length} file(s) and ${folderIds.length} folder(s)?`;
		confirmDanger = true;
		confirmOnConfirm = async () => {
			try {
				for (const fileId of fileIds) await deleteFile(fileId);
				for (const folderId of folderIds) await deleteFolder(folderId);
				selectionStore.clear();
				selectionMode = false;
				queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
				queryClient.invalidateQueries({ queryKey: ['all-files'] });
				showNotification(`Deleted ${fileIds.length + folderIds.length} item(s)`, 'success');
			} catch (error) {
				showNotification('Failed to delete some items', 'error');
			}
		};
		showConfirmModal = true;
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

	function handleRenameFileInline(file: File, newName: string) {
		$renameFileMutation.mutate({ fileId: file.id, newName });
	}

	function handleRenameFolderInline(folder: Folder, newName: string) {
		$renameFolderMutation.mutate({ folderId: folder.id, newName });
	}

	function handleRenameConfirm(event: { newName: string }) {
		if (!renameTarget) return;
		if (renameType === 'file') {
			$renameFileMutation.mutate({ fileId: renameTarget.id, newName: event.newName });
		} else {
			$renameFolderMutation.mutate({ folderId: renameTarget.id, newName: event.newName });
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

	function handleMoveFileWithFallback(file: File, targetFolderId: string | null) {
		if (targetFolderId === null) {
			moveTarget = file;
			moveType = 'file';
			showMoveModal = true;
		} else {
			$moveFileMutation.mutate({ fileId: file.id, targetFolderId });
		}
	}

	function handleMoveFolderWithFallback(folder: Folder, targetFolderId: string | null) {
		if (targetFolderId === null) {
			moveTarget = folder;
			moveType = 'folder';
			showMoveModal = true;
		} else {
			$moveFolderMutation.mutate({ folderId: folder.id, targetFolderId });
		}
	}

	async function handleMoveConfirm(event: { targetFolderId: string | null }) {
		if (bulkMoveFileIds.length > 0) {
			bulkMoveLoading = true;

			try {
				for (const fileId of bulkMoveFileIds) {
					await moveFile(fileId, event.targetFolderId);
				}

				queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
				queryClient.invalidateQueries({ queryKey: ['all-files'] });
				selectionStore.clear();
				selectionMode = false;
				showNotification(
					`Moved ${bulkMoveFileIds.length} selected file${bulkMoveFileIds.length === 1 ? '' : 's'}`,
					'success'
				);
			} catch (error) {
				showNotification(
					error instanceof Error ? error.message : 'Failed to move selected files',
					'error'
				);
			} finally {
				bulkMoveLoading = false;
				bulkMoveFileIds = [];
				showMoveModal = false;
			}

			return;
		}

		if (!moveTarget) return;
		if (moveType === 'file') {
			$moveFileMutation.mutate({ fileId: moveTarget.id, targetFolderId: event.targetFolderId });
		} else {
			$moveFolderMutation.mutate({ folderId: moveTarget.id, targetFolderId: event.targetFolderId });
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

	function handleShareNotification(e: { message: string; type: 'success' | 'error' | 'info' }) {
		showNotification(e.message, e.type);
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
			showNotification(`${truncateFilename(file.name)} download started`, 'success');
			activityStore.addActivity('file_downloaded', file.name);
		} catch (error) {
			showNotification(error instanceof Error ? error.message : 'Failed to download', 'error');
		}
	}

	async function handleDownloadFolder(folder: Folder) {
		if (workspaceMode === 'deleted') return;
		try {
			const blob = await downloadFolder(folder.id);
			const url = URL.createObjectURL(blob);
			const a = document.createElement('a');
			a.href = url;
			a.download = `${folder.name}.zip`;
			document.body.appendChild(a);
			a.click();
			document.body.removeChild(a);
			URL.revokeObjectURL(url);
			showNotification(`${folder.name}.zip download started`, 'success');
			activityStore.addActivity('folder_downloaded', folder.name);
		} catch (error) {
			showNotification(
				error instanceof Error ? error.message : 'Failed to download folder',
				'error'
			);
		}
	}

	function handleReplaceFile(file: File) {
		replaceFileTarget = file;
		showReplaceFileModal = true;
	}

	function handleReplaceSuccess() {
		queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
		queryClient.invalidateQueries({ queryKey: ['all-files'] });
		const fileName = replaceFileTarget?.name || 'File';
		showNotification(`${truncateFilename(fileName)} was updated`, 'success');
		if (replaceFileTarget) {
			activityStore.addActivity('file_modified', replaceFileTarget.name);
		}
		showReplaceFileModal = false;
		replaceFileTarget = null;
	}

	function handleCreateFolderConfirm(event: { name: string; parentFolderId: string | null }) {
		$createFolderMutation.mutate({
			name: event.name,
			parentFolderId: event.parentFolderId
		});
	}

	async function handleCreateFileConfirm(event: {
		targetFolderId: string | null;
		fileType: string;
		fileName: string;
	}) {
		const { targetFolderId, fileType, fileName } = event;
		createFileLoading = true;

		try {
			if (fileType === 'md') {
				const note = await $createNoteMutation.mutateAsync({
					title: fileName.replace(/\.md$/i, ''),
					content: '',
					parent_folder_id: targetFolderId
				});
				showCreateFileModal = false;
				navigateToNote(note.id, getFilesReturnUrl());
			} else {
				showNotification(`File creation for ${fileType} not yet implemented`, 'info');
				showCreateFileModal = false;
			}
		} catch (error) {
			showNotification(error instanceof Error ? error.message : 'Failed to create file', 'error');
		} finally {
			createFileLoading = false;
		}
	}

	function handleUploadTargetConfirm(event: { targetFolderId: string | null }) {
		uploadTargetFolderId = event.targetFolderId;
		showUploadTargetModal = false;

		setTimeout(() => {
			document.getElementById('upload-file-input')?.click();
		}, 100);
	}

	function handleEditFileSelect(event: { file: File }) {
		const file = event.file;
		showEditFileModal = false;
		handleEditFile(file);
	}

	function handleVersionRestored() {
		queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
		queryClient.invalidateQueries({ queryKey: ['all-files'] });
		showNotification('Version restored', 'success');
	}

	function handleToggleFileStar(file: File) {
		$fileStarMutation.mutate({ fileId: file.id, starred: !file.starred_at });
	}

	function handleSetColor(_file: File, _color: string | null) {
		// The child tile/row already calls setFileColor and only notifies us
		// so the query can be invalidated. Avoid a second API request here.
		queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
		queryClient.invalidateQueries({ queryKey: ['all-files'] });
	}

	function handleToggleFolderStar(folder: Folder) {
		$folderStarMutation.mutate({ folderId: folder.id, starred: !folder.starred_at });
	}

	function handleRestoreFile(file: File) {
		$restoreFileMutation.mutate({ fileId: file.id, fileName: file.name });
	}

	function handleRestoreFolder(folder: Folder) {
		$restoreFolderMutation.mutate(folder.id);
	}

	function handlePermanentDeleteFile(file: File) {
		confirmTitle = 'Permanently Delete File';
		confirmMessage = `Permanently delete ${file.name}? This cannot be undone.`;
		confirmDanger = true;
		confirmOnConfirm = () => {
			$permanentlyDeleteFileMutation.mutate({ fileId: file.id, fileName: file.name });
		};
		showConfirmModal = true;
	}

	function handlePermanentDeleteFolder(folder: Folder) {
		confirmTitle = 'Permanently Delete Folder';
		confirmMessage = `Permanently delete ${folder.name} and everything inside it? This cannot be undone.`;
		confirmDanger = true;
		confirmOnConfirm = () => {
			$permanentlyDeleteFolderMutation.mutate(folder.id);
		};
		showConfirmModal = true;
	}

	async function openEmptyTrashModal() {
		try {
			const summary = await getTrashSummary();
			trashSummary = summary;
			showEmptyTrashModal = true;
		} catch (error) {
			showNotification('Failed to load trash summary', 'error');
		}
	}

	function handleEmptyTrash() {
		emptyingTrash = true;
		$emptyTrashMutation.mutate(undefined, {
			onSettled: () => {
				emptyingTrash = false;
			}
		});
	}

	// Keyboard shortcuts and event listeners
	onMount(() => {
		const handleCreateFolderEvent = () => {
			if (canCreateFolder) showCreateFolderModal = true;
		};
		const handleCreateDocumentEvent = () => {
			editorTarget = null;
			showMarkdownEditor = true;
		};
		const handleCreateNoteEvent = () => {
			$createNoteMutation.mutate({
				title: 'Untitled Note',
				content: '',
				parent_folder_id: currentFolderId
			});
		};
		const handleCreateFileEvent = () => {
			showCreateFileModal = true;
		};
		const handleCreateCanvasEvent = () => {
			editorTarget = null;
			showExcalidrawEditor = true;
		};
		const handleUploadEvent = () => {
			if (canUpload) showUploadTargetModal = true;
		};

		const handleEditFileEvent = () => {
			editableFilesForModal = sortedFiles.filter((f) => {
				const name = f.name.toLowerCase();
				return name.endsWith('.md') || name.endsWith('.txt') || name.endsWith('.excalidraw');
			});
			showEditFileModal = true;
		};

		window.addEventListener('create-folder-requested', handleCreateFolderEvent);
		window.addEventListener('create-document-requested', handleCreateDocumentEvent);
		window.addEventListener('create-file-requested', handleCreateFileEvent);
		window.addEventListener('create-canvas-requested', handleCreateCanvasEvent);
		window.addEventListener('upload-requested', handleUploadEvent);
		window.addEventListener('create-note-requested', handleCreateNoteEvent);
		window.addEventListener('edit-file-requested', handleEditFileEvent);

		return () => {
			window.removeEventListener('create-folder-requested', handleCreateFolderEvent);
			window.removeEventListener('create-document-requested', handleCreateDocumentEvent);
			window.removeEventListener('create-file-requested', handleCreateFileEvent);
			window.removeEventListener('create-canvas-requested', handleCreateCanvasEvent);
			window.removeEventListener('upload-requested', handleUploadEvent);
			window.removeEventListener('create-note-requested', handleCreateNoteEvent);
			window.removeEventListener('edit-file-requested', handleEditFileEvent);
		};
	});

	function handleKeyDown(event: KeyboardEvent) {
		if (event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement)
			return;

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
			showCreateFileModal = false;
			showUploadTargetModal = false;
			showEditFileModal = false;
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

	let isUploading = $derived(
		uploadTasks.some((t) => t.status === 'uploading' || t.status === 'pending')
	);
	let moveCurrentFolderId = $derived(
		moveType === 'file'
			? (moveTarget as File | null)?.parent_folder_id
			: (moveTarget as Folder | null)?.parent_folder_id
	);
</script>

<svelte:head>
	<title>{workspaceTitle} - RustShare</title>
</svelte:head>

<svelte:window onkeydown={handleKeyDown} />

<!-- Hidden file input for upload button -->
<input
	id="upload-file-input"
	type="file"
	class="hidden"
	multiple
	onchange={(e) => {
		const target = e.target as HTMLInputElement;
		if (target.files && target.files.length > 0) {
			const files = Array.from(target.files);
			const isDirectory = files.some(
				(f) => (f as globalThis.File & { webkitRelativePath?: string }).webkitRelativePath
			);
			if (isDirectory) {
				handleDirectoryUpload(
					files.map((file) => ({
						file,
						relativePath:
							(file as globalThis.File & { webkitRelativePath?: string }).webkitRelativePath ||
							file.name
					}))
				);
			} else {
				handleFilesSelected(files);
			}
			target.value = '';
		}
	}}
/>

<OfflineBanner />

<DropZone
	onFilesDropped={handleFilesSelected}
	onDirectoryDropped={handleDirectoryUpload}
	disabled={!canUpload || isUploading}
>
	<div class="flex h-full min-h-0 flex-col bg-base-100">
		{#if workspaceMode === 'deleted'}
			<div class="mb-3 flex items-center justify-between px-1">
				<div class="text-sm text-base-content/60">
					Items in trash are automatically deleted based on your <a
						href="/settings"
						class="text-brand-500 hover:underline">settings</a
					>.
				</div>
				<button
					type="button"
					class="flex items-center gap-2 rounded-lg border border-error/30 bg-error/10 px-3 py-1.5 text-sm font-medium text-error transition-colors hover:bg-error/20"
					onclick={openEmptyTrashModal}
				>
					<svg
						xmlns="http://www.w3.org/2000/svg"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						stroke-width="2"
						stroke-linecap="round"
						stroke-linejoin="round"
						class="h-4 w-4"
						><path d="M3 6h18" /><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" /><path
							d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"
						/></svg
					>
					Empty Trash
				</button>
			</div>
		{/if}

		<FileBrowserToolbar
			title={workspaceTitle}
			description={workspaceDescription}
			breadcrumbItems={breadcrumbPath}
			rootLabel={activeRoot === 'shared' ? 'Shared' : 'My Files'}
			{showBreadcrumbs}
			{canCreateFolder}
			{canUpload}
			{allowSelectionMode}
			{selectionMode}
			onToggleSelection={toggleSelectionMode}
			onSelectAll={handleSelectAll}
			onDeselectAll={handleDeselectAll}
			onBulkDelete={handleBulkDelete}
			onBulkDownload={handleBulkDownload}
			onBulkMove={handleBulkMove}
			onNewFolder={() => (showCreateFolderModal = true)}
			onUpload={() => document.getElementById('upload-file-input')?.click()}
			onAsk={currentFolderId && activeRoot === 'my-files'
				? () =>
						goto(
							askHref({
								type: 'folder',
								resourceRef: `elembra://io.elembra.files/folder/${currentFolderId}`
							})
						)
				: undefined}
			onBreadcrumbNavigate={handleBreadcrumbNavigate}
			{isUploading}
		/>

		<FileBrowserContent
			folders={paginatedFolders}
			files={paginatedFiles}
			{workspaceMode}
			{isSharedRoot}
			isLoading={$filesQuery.isLoading}
			error={$filesQuery.error}
			emptyTitle={workspaceEmptyTitle}
			emptyDescription={workspaceEmptyDescription}
			emptyActionLabel={workspaceEmptyActionLabel}
			{selectionMode}
			activeSortField={$fileSortState.field}
			activeSortOrder={$fileSortState.order}
			onSort={setSortField}
			onRefresh={() => $filesQuery.refetch()}
			onFolderClick={handleFolderClick}
			onFileClick={handleFileClick}
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
			onEditFile={handleEditFile}
			onSetColor={handleSetColor}
			onRenameFolder={handleRenameFolderInline}
			onDeleteFolder={handleDeleteFolder}
			onToggleFolderStar={handleToggleFolderStar}
			onRestoreFolder={handleRestoreFolder}
			onPermanentDeleteFolder={handlePermanentDeleteFolder}
			onShareFolder={handleShareFolder}
			onMoveFolder={handleMoveFolderWithFallback}
			onDownloadFolder={handleDownloadFolder}
		>
			{#snippet pagination()}
				<div class="flex justify-center">
					<PaginationControls
						{currentPage}
						{totalPages}
						pageSize={$fileSortState.pageSize}
						onPageChange={(page) => (currentPage = page)}
						onPageSizeChange={setPageSize}
					/>
				</div>
			{/snippet}
		</FileBrowserContent>
	</div>
</DropZone>

<!-- Upload Progress -->
<UploadProgress tasks={uploadTasks} onClose={handleCloseProgress} />

<FileModals
	{currentFolderId}
	moveCurrentFolderId={moveType === 'file'
		? ((moveTarget as File | null)?.parent_folder_id ?? null)
		: ((moveTarget as Folder | null)?.parent_folder_id ?? null)}
	{showRenameModal}
	{renameTarget}
	{renameType}
	renameLoading={$renameFileMutation.isPending || $renameFolderMutation.isPending}
	onRenameClose={() => {
		showRenameModal = false;
		renameTarget = null;
	}}
	onRenameConfirm={handleRenameConfirm}
	{showDeleteModal}
	{deleteTarget}
	{deleteType}
	deleteLoading={$deleteFileMutation.isPending || $deleteFolderMutation.isPending}
	onDeleteClose={() => {
		showDeleteModal = false;
		deleteTarget = null;
	}}
	onDeleteConfirm={handleDeleteConfirm}
	{showMoveModal}
	{moveTarget}
	{moveType}
	moveLoading={$moveFileMutation.isPending || $moveFolderMutation.isPending}
	{bulkMoveFileIds}
	{bulkMoveLoading}
	onMoveClose={() => {
		showMoveModal = false;
		moveTarget = null;
		bulkMoveFileIds = [];
	}}
	onMoveConfirm={handleMoveConfirm}
	{showCreateFolderModal}
	createFolderLoading={$createFolderMutation.isPending}
	onCreateFolderClose={() => (showCreateFolderModal = false)}
	onCreateFolderConfirm={handleCreateFolderConfirm}
	{showCreateFileModal}
	{createFileLoading}
	onCreateFileClose={() => (showCreateFileModal = false)}
	onCreateFileConfirm={handleCreateFileConfirm}
	{showUploadTargetModal}
	onUploadTargetClose={() => (showUploadTargetModal = false)}
	onUploadTargetConfirm={handleUploadTargetConfirm}
	{showEditFileModal}
	{editableFilesForModal}
	onEditFileClose={() => (showEditFileModal = false)}
	onEditFileSelect={handleEditFileSelect}
	{showShareModal}
	{shareTarget}
	{shareType}
	onShareClose={() => {
		showShareModal = false;
		shareTarget = null;
	}}
	onShareNotification={handleShareNotification}
	{showVersionHistoryModal}
	{versionHistoryTarget}
	onVersionHistoryClose={() => {
		showVersionHistoryModal = false;
		versionHistoryTarget = null;
	}}
	onVersionRestored={handleVersionRestored}
	{showFilePreviewModal}
	{previewTarget}
	onFilePreviewClose={() => {
		showFilePreviewModal = false;
		previewTarget = null;
	}}
	onEditFile={handleEditFile}
	{showReplaceFileModal}
	{replaceFileTarget}
	onReplaceFileClose={() => {
		showReplaceFileModal = false;
		replaceFileTarget = null;
	}}
	onReplaceSuccess={handleReplaceSuccess}
/>

<EmptyTrashModal
	open={showEmptyTrashModal}
	loading={emptyingTrash}
	fileCount={trashSummary.file_count}
	folderCount={trashSummary.folder_count}
	totalSize={trashSummary.total_size}
	onClose={() => (showEmptyTrashModal = false)}
	onConfirm={handleEmptyTrash}
/>

<FileEditorPane
	{showTextEditor}
	{showMarkdownEditor}
	{showExcalidrawEditor}
	{editorTarget}
	onEditorClose={handleEditorClose}
	onEditorSaved={handleEditorSaved}
/>

<!-- Toast -->
{#if showToast}
	<Toast message={toastMessage} type={toastType} onClose={() => (showToast = false)} />
{/if}

<ConfirmModal
	open={showConfirmModal}
	title={confirmTitle}
	message={confirmMessage}
	confirmLabel="Confirm"
	cancelLabel="Cancel"
	danger={confirmDanger}
	onConfirm={() => {
		showConfirmModal = false;
		confirmOnConfirm();
	}}
	onCancel={() => (showConfirmModal = false)}
/>
