<script lang="ts">
	import type { File as FileType, Folder } from '$lib/api/types';
	import type { ReplicationStatus } from '$lib/stores/replication';

	import FileBrowserPane from './FileBrowserPane.svelte';

	export let folders: Folder[] = [];
	export let files: FileType[] = [];
	export let folderPath: Folder[] = [];
	export let title = 'All files';
	export let description = '';
	export let emptyTitle = 'No files yet';
	export let emptyDescription = 'Upload your first file to get started';
	export let emptyActionLabel: string | null = 'Upload files';
	export let workspaceMode: 'all' | 'photos' | 'recent' | 'starred' | 'deleted' = 'all';
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


</script>

<div class="flex h-full min-h-0 bg-base-100">


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
