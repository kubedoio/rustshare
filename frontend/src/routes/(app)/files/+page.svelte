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
	import { createQuery, createMutation } from '@tanstack/svelte-query';
	import { truncateFilename } from '$lib/utils/format';
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
		listAllFiles
	} from '$lib/api/files';
	import { createNote } from '$lib/api/notes';
	import {
		createFolder,
		deleteFolder,
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
	import { extractFolderPaths, sortFolderPaths } from '$lib/utils/directoryUpload';
	import { queryClient } from '$lib/query-client';
	import { searchQuery } from '$lib/stores/search';
	import { fileSortState } from '$lib/stores/fileSort';
	import { selectionStore, selectionCount, hasSelection } from '$lib/stores/selection';
	import { activityStore } from '$lib/stores/activity';
	import { replicationStore, type ReplicationStatus } from '$lib/stores/replication';
	import { folderTreeStore } from '$lib/stores/folderTree';
	import type { File, Folder, FolderContents as ApiFolderContents } from '$lib/api/types';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';

	// Explorer types
	import type { ExplorerRoot, CollectionView } from '$lib/explorer';
	import { ROOT_CONFIG } from '$lib/explorer';

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
	import CreateFileModal from '$lib/components/modals/CreateFileModal.svelte';
	import UploadTargetModal from '$lib/components/modals/UploadTargetModal.svelte';
	import EditFileModal from '$lib/components/modals/EditFileModal.svelte';

	// Editors
	import { TextEditor, MarkdownEditor, ExcalidrawEditor } from '$lib/components/editors';
	import { detectEditorType } from '$lib/utils/editor';

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

	type WorkspaceMode = 'all' | 'photos' | 'recent' | 'starred' | 'deleted';

	let uploadTasks: UploadTask[] = [];
	let selectionMode = false;

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
	let showCreateFileModal = false;
	let showUploadTargetModal = false;
	let showEditFileModal = false;
	let createFileLoading = false;
	let uploadTargetFolderId: string | null = null;
	let editableFilesForModal: File[] = [];

	// Editor state
	let showTextEditor = false;
	let showMarkdownEditor = false;
	let showExcalidrawEditor = false;
	let editorTarget: File | null = null;

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

	// ============================================================================
	// EXPLORER STATE DERIVATIONS
	// ============================================================================

	// URL parameters
	$: urlFolderId = $page.url.searchParams.get('folder');
	$: urlFilter = $page.url.searchParams.get('filter');
	$: urlSort = $page.url.searchParams.get('sort');
	$: urlRoot = $page.url.searchParams.get('root') as ExplorerRoot | null;

	// Helper to check if a string looks like a valid UUID
	function isValidUuid(value: string | null): value is string {
		if (!value) return false;
		const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
		return uuidPattern.test(value);
	}

	// Current workspace mode
	$: workspaceMode = (urlFilter === 'photos'
		? 'photos'
		: urlFilter === 'starred'
			? 'starred'
			: urlFilter === 'deleted'
				? 'deleted'
				: urlSort === 'recent'
					? 'recent'
					: 'all') as WorkspaceMode;

	// Active root (my-files or shared)
	$: activeRoot = (urlRoot === 'shared' ? 'shared' : 'my-files') as ExplorerRoot;

	// Is in collection mode?
	$: isCollectionMode = workspaceMode === 'starred' || workspaceMode === 'recent' || workspaceMode === 'photos';

	// Is shared root view?
	$: isSharedRoot = activeRoot === 'shared' && !currentFolderId;

	// Current folder ID (null at root)
	$: currentFolderId = isCollectionMode 
		? null 
		: (isValidUuid(urlFolderId) ? urlFolderId : null);

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
	$: filesQuery = createQuery<ApiFolderContents>({
		queryKey: ['file-workspace', workspaceMode, currentFolderId, activeRoot],
		queryFn: async () => {
			if (workspaceMode === 'starred') return getStarredContents();
			if (workspaceMode === 'deleted') return getDeletedContents();
			
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
						folders: shares.filter(s => s.resource_type === 'folder').map(s => ({
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
						files: shares.filter(s => s.resource_type === 'file').map(s => ({
							id: s.resource_id,
							name: s.resource_name,
							path: s.resource_path,
							content_hash: '',
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
	$: myFilesFolderPath = currentFolderId && $folderTreeQuery.data && activeRoot === 'my-files'
		? buildFolderPathFromApiTree($folderTreeQuery.data, currentFolderId).slice(1)
		: [];
	$: sharedFolderPath =
		currentFolderId && activeRoot === 'shared' && $sharedFolderTreesQuery.data
			? findFolderPathInSharedTrees(currentFolderId, $sharedFolderTreesQuery.data)
			: [];

	// Build breadcrumb based on current state
	// Returns Folder-compatible objects for FileExplorer component
	$: breadcrumbPath = buildBreadcrumb();

	function buildBreadcrumb(): Folder[] {
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
	}

	function buildFolderPathFromApiTree(root: FolderTreeType, targetId: string): Folder[] {
		function search(node: FolderTreeType): Folder[] {
			if (node.folder.id === targetId) {
				return [node.folder];
			}
			if (node.subfolders) {
				for (const child of node.subfolders) {
					const path = search(child);
					if (path.length > 0) {
						return [node.folder, ...path];
					}
				}
			}
			return [];
		}
		return search(root);
	}

	function findFolderPathInSharedTrees(targetId: string, trees: FolderTreeType[]): Folder[] {
		for (const tree of trees) {
			const path = buildFolderPathFromApiTree(tree, targetId);
			if (path.length > 0) {
				return path;
			}
		}
		return [];
	}

	function permissionLevel(permission: 'View' | 'Edit' | 'Admin' | null | undefined): number {
		if (permission === 'Admin') return 3;
		if (permission === 'Edit') return 2;
		if (permission === 'View') return 1;
		return 0;
	}

	$: currentSharedFolderPermission =
		activeRoot === 'shared' ? ($filesQuery.data?.current_folder_permission ?? null) : null;
	$: hasSharedWritePermission = permissionLevel(currentSharedFolderPermission) >= 2;

	// ============================================================================
	// TITLE DERIVATION (Contextual Header)
	// ============================================================================

	$: workspaceTitle = isCollectionMode
		? (workspaceMode === 'photos'
			? 'Photos'
			: workspaceMode === 'recent'
				? 'Recent'
				: workspaceMode === 'starred'
					? 'Starred'
					: 'Deleted')
		: activeRoot === 'shared'
			? (currentFolderId ? breadcrumbPath[breadcrumbPath.length - 1]?.name : 'Shared')
			: (currentFolderId ? breadcrumbPath[breadcrumbPath.length - 1]?.name : 'My Files');

	$: workspaceDescription = isCollectionMode
		? (workspaceMode === 'photos'
			? 'Image files in the current workspace, without the folder noise.'
			: workspaceMode === 'recent'
				? 'The latest changes in this workspace, sorted by most recent first.'
				: workspaceMode === 'starred'
					? 'Pinned folders and files that need fast access without digging through the tree.'
					: 'Recently deleted items live here until you restore them or remove them permanently.')
		: activeRoot === 'shared'
			? (currentFolderId 
				? 'Shared folder contents.' 
				: 'Folders shared with you by other users.')
			: (currentFolderId 
				? 'Folder contents.' 
				: 'Folders and files, tuned for quick scanning instead of dashboard theater.');

	$: workspaceEmptyTitle = isCollectionMode
		? (workspaceMode === 'photos'
			? 'No photos in this view'
			: workspaceMode === 'recent'
				? 'No recent file activity'
				: workspaceMode === 'starred'
					? 'Nothing is starred yet'
					: 'Deleted items will show up here')
		: activeRoot === 'shared'
			? 'No shared folders'
			: 'No files yet';

	$: workspaceEmptyDescription = isCollectionMode
		? (workspaceMode === 'photos'
			? 'Upload an image into this folder and it will show up here.'
			: workspaceMode === 'recent'
				? 'Modify or upload a file and it will show up here.'
				: workspaceMode === 'starred'
					? 'Star a folder or file from its action menu and it will show up here.'
					: 'Deleting a folder or file moves it here instead of removing it immediately.')
		: activeRoot === 'shared'
			? 'Items shared with you will appear here.'
			: 'Upload your first file or create a folder to get started.';

	$: workspaceEmptyActionLabel = (!isCollectionMode && activeRoot === 'my-files')
		? 'Upload files'
		: null;

	// ============================================================================
	// UI STATE DERIVATIONS
	// ============================================================================

	$: showFolderTree = !isCollectionMode;
	$: showBreadcrumbs = !isCollectionMode;
	$: canCreateFolder =
		!isCollectionMode && (activeRoot === 'my-files' || (activeRoot === 'shared' && hasSharedWritePermission));
	$: canUpload =
		!isCollectionMode && (activeRoot === 'my-files' || (activeRoot === 'shared' && hasSharedWritePermission));
	$: allowSelectionMode = workspaceMode !== 'deleted';
	$: if (!allowSelectionMode && selectionMode) {
		selectionMode = false;
		selectionStore.clear();
	}

	// ============================================================================
	// SORTING & FILTERING
	// ============================================================================

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
		mutationFn: ({ title, content, parent_folder_id }: { title: string; content: string; parent_folder_id: string | null }) => 
			createNote({ title, content, parent_folder_id }),
		onSuccess: (data) => {
			goto(`/notes/${data.id}`);
		}
	});

	const createFolderMutation = createMutation({
		mutationFn: ({ name, parentFolderId }: { name: string; parentFolderId: string | null }) => createFolder(name, parentFolderId),
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
		mutationFn: ({ fileId, newName }: { fileId: string; newName: string }) => renameFile(fileId, newName),
		onSuccess: (_, { newName }) => {
			const oldName = renameTarget?.name || 'File';
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['all-files'] });
			showRenameModal = false;
			renameTarget = null;
			showNotification(`${truncateFilename(newName)} renamed`, 'success');
			activityStore.addActivity('file_renamed', newName, oldName);
		}
	});

	const renameFolderMutation = createMutation({
		mutationFn: ({ folderId, newName }: { folderId: string; newName: string }) => renameFolder(folderId, newName),
		onSuccess: (_, { folderId, newName }) => {
			const oldName = renameTarget?.name || 'Folder';
			folderTreeStore.updateFolderName(folderId, newName);
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['folder-tree'] });
			queryClient.invalidateQueries({ queryKey: ['all-files'] });
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
			if (deleteTarget && (currentFolderId === deleteTarget.id || breadcrumbPath.some(f => f.id === deleteTarget?.id))) {
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
			queryClient.invalidateQueries({ queryKey: ['all-files'] });
			showMoveModal = false;
			moveTarget = null;
			showNotification(`${truncateFilename(fileName)} moved`, 'success');
			activityStore.addActivity('file_moved', fileName);
		}
	});

	const moveFolderMutation = createMutation({
		mutationFn: ({ folderId, targetFolderId }: { folderId: string; targetFolderId: string | null }) => moveFolder(folderId, targetFolderId),
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
		mutationFn: ({ fileId, fileName }: { fileId: string; fileName: string }) => restoreFileFromTrash(fileId),
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
		mutationFn: ({ fileId, fileName }: { fileId: string; fileName: string }) => permanentlyDeleteFile(fileId),
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

	// ============================================================================
	// NAVIGATION HANDLERS
	// ============================================================================

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
			goto(`/notes/${file.id}`);
			return;
		}
		previewTarget = file;
		showFilePreviewModal = true;
	}

	// ============================================================================
	// OTHER HANDLERS (unchanged from original)
	// ============================================================================

	function handleEditFile(event: CustomEvent<{ file: File }> | File) {
		const file = event instanceof CustomEvent ? event.detail.file : event;
		if (detectEditorType(file.name, file.mime_type) === 'markdown') {
			goto(`/notes/${file.id}`);
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

			try {
				await $uploadMutation.mutateAsync({
					file: files[i],
					folderId: uploadTargetFolderId ?? currentFolderId,
					onProgress: (progress) => {
						const currentTaskIndex = uploadTasks.findIndex(t => t.id === newTasks[i].id);
						if (currentTaskIndex !== -1) {
							uploadTasks[currentTaskIndex].status = 'uploading';
							uploadTasks[currentTaskIndex].progress = progress;
							uploadTasks = [...uploadTasks];
						}
					}
				});
				
				const finalTaskIndex = uploadTasks.findIndex(t => t.id === newTasks[i].id);
				if (finalTaskIndex !== -1) {
					uploadTasks[finalTaskIndex].status = 'success';
					uploadTasks[finalTaskIndex].progress = 100;
					uploadTasks = [...uploadTasks];
				}
			} catch (error) {
				const errorMessage = error instanceof Error ? error.message : 'Upload failed';
				const errorTaskIndex = uploadTasks.findIndex(t => t.id === newTasks[i].id);
				if (errorTaskIndex !== -1) {
					uploadTasks[errorTaskIndex].status = 'error';
					uploadTasks[errorTaskIndex].error = errorMessage;
					uploadTasks = [...uploadTasks];
				}
			}
		}

		const successCount = uploadTasks.filter(t => t.status === 'success').length;
		const errorCount = uploadTasks.filter(t => t.status === 'error').length;

		if (errorCount === 0) {
			if (successCount === 1) {
				const uploadedFile = uploadTasks.find(t => t.status === 'success');
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

	async function handleDirectoryUpload(files: globalThis.File[]) {
		if (!canUpload || files.length === 0) return;

		const items = files.map((file) => ({
			file,
			relativePath: (file as globalThis.File & { webkitRelativePath?: string }).webkitRelativePath || file.name
		}));

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
			const parentId = parentPath ? (folderIdMap.get(parentPath) ?? null) : currentFolderId;

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
		for (const file of files) {
			const relativePath = (file as globalThis.File & { webkitRelativePath?: string }).webkitRelativePath || file.name;
			const lastSlash = relativePath.lastIndexOf('/');

			if (lastSlash > 0) {
				const folderPath = relativePath.slice(0, lastSlash);
				if (failedFolderPaths.has(folderPath)) continue;
				const parentId = folderIdMap.get(folderPath) ?? null;
				filesToUpload.push({ file, parentFolderId: parentId });
			} else {
				filesToUpload.push({ file, parentFolderId: currentFolderId });
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
			} catch (error) {
				const errorMessage = error instanceof Error ? error.message : 'Upload failed';
				const errorTaskIndex = uploadTasks.findIndex((t) => t.id === taskId);
				if (errorTaskIndex !== -1) {
					uploadTasks[errorTaskIndex].status = 'error';
					uploadTasks[errorTaskIndex].error = errorMessage;
					uploadTasks = [...uploadTasks];
				}
			}
		}

		const successCount = newTasks.filter((t) => t.status === 'success').length;
		const errorCount = newTasks.filter((t) => t.status === 'error').length;

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
			queryClient.invalidateQueries({ queryKey: ['all-files'] });
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

	async function handleMoveConfirm(event: CustomEvent<{ targetFolderId: string | null }>) {
		if (bulkMoveFileIds.length > 0) {
			bulkMoveLoading = true;

			try {
				for (const fileId of bulkMoveFileIds) {
					await moveFile(fileId, event.detail.targetFolderId);
				}

				queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
				queryClient.invalidateQueries({ queryKey: ['all-files'] });
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
			showNotification(`${truncateFilename(file.name)} download started`, 'success');
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
		queryClient.invalidateQueries({ queryKey: ['all-files'] });
		const fileName = replaceFileTarget?.name || 'File';
		showNotification(`${truncateFilename(fileName)} was updated`, 'success');
		if (replaceFileTarget) {
			activityStore.addActivity('file_modified', replaceFileTarget.name);
		}
		showReplaceFileModal = false;
		replaceFileTarget = null;
	}

	function handleCreateFolderConfirm(event: CustomEvent<{ name: string; parentFolderId: string | null }>) {
		$createFolderMutation.mutate({ 
			name: event.detail.name, 
			parentFolderId: event.detail.parentFolderId 
		});
	}

	async function handleCreateFileConfirm(event: CustomEvent<{ targetFolderId: string | null; fileType: string; fileName: string }>) {
		const { targetFolderId, fileType, fileName } = event.detail;
		createFileLoading = true;
		
		try {
			if (fileType === 'md') {
				const note = await $createNoteMutation.mutateAsync({ 
					title: fileName.replace(/\.md$/i, ''), 
					content: '', 
					parent_folder_id: targetFolderId 
				});
				showCreateFileModal = false;
				goto(`/notes/${note.id}`);
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

	function handleUploadTargetConfirm(event: CustomEvent<{ targetFolderId: string | null }>) {
		uploadTargetFolderId = event.detail.targetFolderId;
		showUploadTargetModal = false;
		
		setTimeout(() => {
			document.getElementById('upload-file-input')?.click();
		}, 100);
	}

	function handleEditFileSelect(event: CustomEvent<{ file: File }>) {
		const file = event.detail.file;
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
		if (!confirm(`Permanently delete ${file.name}? This cannot be undone.`)) return;
		$permanentlyDeleteFileMutation.mutate({ fileId: file.id, fileName: file.name });
	}

	function handlePermanentDeleteFolder(folder: Folder) {
		if (!confirm(`Permanently delete ${folder.name} and everything inside it? This cannot be undone.`)) return;
		$permanentlyDeleteFolderMutation.mutate(folder.id);
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
			$createNoteMutation.mutate({ title: 'Untitled Note', content: '', parent_folder_id: currentFolderId });
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
			editableFilesForModal = sortedFiles.filter(f => {
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

	$: isUploading = uploadTasks.some(t => t.status === 'uploading' || t.status === 'pending');
	$: moveCurrentFolderId = moveType === 'file' 
		? (moveTarget as File | null)?.parent_folder_id 
		: (moveTarget as Folder | null)?.parent_folder_id;
	$: replicationStatuses = $replicationStore;
</script>

<svelte:head>
	<title>{workspaceTitle} - RustShare</title>
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
			const files = Array.from(target.files);
			const isDirectory = files.some((f) => (f as globalThis.File & { webkitRelativePath?: string }).webkitRelativePath);
			if (isDirectory) {
				handleDirectoryUpload(files);
			} else {
				handleFilesSelected(files);
			}
			target.value = '';
		}
	}}
/>

<DropZone
	on:filesDropped={(e) => handleFilesSelected(e.detail)}
	on:directoryDropped={(e) => handleDirectoryUpload(e.detail)}
	disabled={!canUpload || isUploading}
>
	<FileExplorer
		folders={sortedFolders}
		files={sortedFiles}
		folderPath={breadcrumbPath}
		rootLabel={activeRoot === 'shared' ? 'Shared' : 'My Files'}
		title={workspaceTitle}
		description={workspaceDescription}
		emptyTitle={workspaceEmptyTitle}
		emptyDescription={workspaceEmptyDescription}
		emptyActionLabel={workspaceEmptyActionLabel}
		{workspaceMode}
		{showBreadcrumbs}
		{canCreateFolder}
		{canUpload}
		{allowSelectionMode}
		isLoading={$filesQuery.isLoading}
		error={$filesQuery.error}
		{replicationStatuses}
		{selectionMode}
		{isUploading}
		{isSharedRoot}
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
		onEditFile={handleEditFile}
		onRenameFolder={handleRenameFolderInline}
		onDeleteFolder={handleDeleteFolder}
		onToggleFolderStar={handleToggleFolderStar}
		onRestoreFolder={handleRestoreFolder}
		onPermanentDeleteFolder={handlePermanentDeleteFolder}
		onShareFolder={handleShareFolder}
		onMoveFolder={handleMoveFolderWithFallback}
		onbreadcrumbNavigate={handleBreadcrumbNavigate}
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
		currentFolderId={currentFolderId}
		on:close={() => showCreateFolderModal = false}
		on:confirm={handleCreateFolderConfirm}
	/>
{/if}

{#if showCreateFileModal}
	<CreateFileModal
		open={showCreateFileModal}
		loading={createFileLoading}
		currentFolderId={currentFolderId}
		on:close={() => showCreateFileModal = false}
		on:confirm={handleCreateFileConfirm}
	/>
{/if}

{#if showUploadTargetModal}
	<UploadTargetModal
		open={showUploadTargetModal}
		currentFolderId={currentFolderId}
		on:close={() => showUploadTargetModal = false}
		on:confirm={handleUploadTargetConfirm}
	/>
{/if}

{#if showEditFileModal}
	<EditFileModal
		open={showEditFileModal}
		files={editableFilesForModal}
		on:close={() => showEditFileModal = false}
		on:select={handleEditFileSelect}
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
		on:edit={handleEditFile}
	/>
{/if}

<!-- Editors -->
{#if showTextEditor && editorTarget}
	<TextEditor
		open={showTextEditor}
		file={editorTarget}
		on:close={handleEditorClose}
		on:saved={handleEditorSaved}
	/>
{/if}

{#if showMarkdownEditor && editorTarget}
	<MarkdownEditor
		open={showMarkdownEditor}
		file={editorTarget}
		on:close={handleEditorClose}
		on:saved={handleEditorSaved}
	/>
{/if}

{#if showExcalidrawEditor && editorTarget}
	<ExcalidrawEditor
		open={showExcalidrawEditor}
		file={editorTarget}
		on:close={handleEditorClose}
		on:saved={handleEditorSaved}
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
