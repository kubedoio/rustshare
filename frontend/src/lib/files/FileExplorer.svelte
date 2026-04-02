<script lang="ts">
	import type { File as FileType, Folder } from '$lib/api/types';
	import type { ReplicationStatus } from '$lib/stores/replication';
	import type { FolderNode } from '$lib/stores/folderTree';
	import FolderTree from './FolderTree.svelte';
	import FileBrowserPane from './FileBrowserPane.svelte';

	export let folders: Folder[] = [];
	export let files: FileType[] = [];
	export let currentFolderId: string | null = null;
	export let folderPath: Folder[] = [];
	export let title = 'All files';
	export let description = '';
	export let emptyTitle = 'No files yet';
	export let emptyDescription = 'Upload your first file to get started';
	export let emptyActionLabel: string | null = 'Upload files';
	export let workspaceMode: 'all' | 'photos' | 'recent' | 'starred' | 'deleted' = 'all';
	export let showFolderTree = true;
	export let showBreadcrumbs = true;
	export let canCreateFolder = true;
	export let canUpload = true;
	export let allowSelectionMode = true;
	export let isLoading: boolean = false;
	export let error: Error | null = null;
	export let replicationStatuses: Record<string, ReplicationStatus> = {};
	export let selectionMode: boolean = false;
	export let isUploading: boolean = false;

	// Event handlers
	export let onFolderSelect: (folderId: string | null, path: Folder[]) => void;
	export let onFolderClick: (folder: Folder) => void;
	export let onFileClick: (file: FileType) => void;
	export let onRefresh: () => void;
	
	// Action handlers
	export let onNewFolder: () => void;
	export let onUpload: () => void;
	export let onToggleSelection: () => void;
	export let onSelectAll: () => void;
	export let onDeselectAll: () => void;
	export let onBulkDelete: () => void;
	export let onBulkDownload: () => void;
	export let onBulkMove: () => void;

	// Item action handlers - support both modal (no args) and inline/direct (with args)
	export let onRenameFile: ((file: FileType) => void) | ((file: FileType, newName: string) => void);
	export let onDeleteFile: (file: FileType) => void;
	export let onToggleFileStar: (file: FileType) => void;
	export let onRestoreFile: (file: FileType) => void;
	export let onPermanentDeleteFile: (file: FileType) => void;
	export let onShareFile: (file: FileType) => void;
	export let onVersionHistory: (file: FileType) => void;
	export let onMoveFile: ((file: FileType) => void) | ((file: FileType, targetFolderId: string | null) => void);
	export let onDownloadFile: (file: FileType) => void;
	export let onReplaceFile: (file: FileType) => void;
	
	export let onRenameFolder: ((folder: Folder) => void) | ((folder: Folder, newName: string) => void);
	export let onDeleteFolder: (folder: Folder) => void;
	export let onToggleFolderStar: (folder: Folder) => void;
	export let onRestoreFolder: (folder: Folder) => void;
	export let onPermanentDeleteFolder: (folder: Folder) => void;
	export let onShareFolder: (folder: Folder) => void;
	export let onMoveFolder: ((folder: Folder) => void) | ((folder: Folder, targetFolderId: string | null) => void);

	export let onEditFile: (file: FileType) => void;

	function handleFolderSelectFromTree(folderId: string | null, path: FolderNode[]) {
		const folderPath = path.map(node => ({
			id: node.id,
			name: node.name,
			path: node.path,
			parent_folder_id: node.parent_folder_id,
			owner_id: node.owner_id || '',
			created_at: node.created_at,
			updated_at: node.updated_at
		}));
		onFolderSelect(folderId, folderPath);
	}

	// Wrapper handlers for tree that accept FolderNode
	function handleRenameFolderFromTree(folderNode: import('$lib/stores/folderTree').FolderNode, newName: string) {
		// Find the matching folder in the current folders list or create a compatible object
		const folder = folders.find(f => f.id === folderNode.id);
		if (folder) {
			(onRenameFolder as (folder: Folder, newName: string) => void)(folder, newName);
		}
	}

	function handleDeleteFolderFromTree(folderNode: import('$lib/stores/folderTree').FolderNode) {
		const folder = folders.find(f => f.id === folderNode.id);
		if (folder) {
			onDeleteFolder(folder);
		}
	}

	function handleShareFolderFromTree(folderNode: import('$lib/stores/folderTree').FolderNode) {
		const folder = folders.find(f => f.id === folderNode.id);
		if (folder) {
			onShareFolder(folder);
		}
	}

	function handleMoveFolderFromTree(folderNode: import('$lib/stores/folderTree').FolderNode, targetFolderId: string | null) {
		const folder = folders.find(f => f.id === folderNode.id);
		if (folder) {
			(onMoveFolder as (folder: Folder, targetFolderId: string | null) => void)(folder, targetFolderId);
		}
	}

	function handleMoveFileToTree(fileId: string, targetFolderId: string | null) {
		const file = files.find(f => f.id === fileId);
		if (file) {
			(onMoveFile as (file: FileType, targetFolderId: string | null) => void)(file, targetFolderId);
		}
	}

	function handleMoveFolderToTree(folderId: string, targetFolderId: string | null) {
		const folder = folders.find(f => f.id === folderId);
		if (folder) {
			(onMoveFolder as (folder: Folder, targetFolderId: string | null) => void)(folder, targetFolderId);
		}
	}
</script>

<div class="flex h-full min-h-0 bg-base-100">
	<!-- Folder Tree Sidebar - Hidden on mobile, shown on xl screens -->
	{#if showFolderTree}
		<div class="hidden h-full w-64 flex-shrink-0 overflow-hidden border-r border-base-300/80 bg-base-100/70 xl:block">
			<FolderTree 
				selectedFolderId={currentFolderId}
				onSelectFolder={handleFolderSelectFromTree}
				onRenameFolder={handleRenameFolderFromTree}
				onDeleteFolder={handleDeleteFolderFromTree}
				onShareFolder={handleShareFolderFromTree}
				onMoveFolder={handleMoveFolderFromTree}
				onCreateSubfolder={(parentId) => onNewFolder()}
				onMoveFile={handleMoveFileToTree}
				onMoveFolderDirect={handleMoveFolderToTree}
			/>
		</div>
	{/if}

	<!-- File Browser Pane -->
	<div class="flex-1 flex min-h-0 min-w-0 flex-col overflow-hidden">
		<FileBrowserPane
			{folders}
			{files}
			{folderPath}
			{title}
			{description}
			{emptyTitle}
			{emptyDescription}
			{emptyActionLabel}
			{workspaceMode}
			{showBreadcrumbs}
			{canCreateFolder}
			{canUpload}
			{allowSelectionMode}
			{isLoading}
			{error}
			{replicationStatuses}
			{selectionMode}
			{isUploading}
			{onRefresh}
			{onNewFolder}
			{onUpload}
			{onToggleSelection}
			{onSelectAll}
			{onDeselectAll}
			{onBulkDelete}
			{onBulkDownload}
			{onBulkMove}
			{onFolderClick}
			{onFileClick}
			{onRenameFile}
			{onDeleteFile}
			{onToggleFileStar}
			{onRestoreFile}
			{onPermanentDeleteFile}
			{onShareFile}
			{onVersionHistory}
			{onMoveFile}
			{onDownloadFile}
			{onReplaceFile}
			{onEditFile}
			{onRenameFolder}
			{onDeleteFolder}
			{onToggleFolderStar}
			{onRestoreFolder}
			{onPermanentDeleteFolder}
			{onShareFolder}
			{onMoveFolder}
		/>
	</div>
</div>
