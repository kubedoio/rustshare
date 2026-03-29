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
	export let showFolderTree = true;
	export let showBreadcrumbs = true;
	export let canCreateFolder = true;
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

	// Item action handlers
	export let onRenameFile: (file: FileType) => void;
	export let onDeleteFile: (file: FileType) => void;
	export let onShareFile: (file: FileType) => void;
	export let onVersionHistory: (file: FileType) => void;
	export let onMoveFile: (file: FileType) => void;
	export let onDownloadFile: (file: FileType) => void;
	export let onReplaceFile: (file: FileType) => void;
	
	export let onRenameFolder: (folder: Folder) => void;
	export let onDeleteFolder: (folder: Folder) => void;
	export let onShareFolder: (folder: Folder) => void;
	export let onMoveFolder: (folder: Folder) => void;

	function handleFolderSelectFromTree(folderId: string | null, path: FolderNode[]) {
		const folderPath = path.map(node => ({
			id: node.id,
			name: node.name,
			path: node.path,
			parent_folder_id: node.parent_folder_id,
			owner_id: node.owner_id,
			created_at: node.created_at,
			updated_at: node.updated_at
		}));
		onFolderSelect(folderId, folderPath);
	}
</script>

<div class="flex h-full min-h-0 bg-base-100">
	<!-- Folder Tree Sidebar -->
	{#if showFolderTree}
		<div class="hidden h-full w-64 flex-shrink-0 overflow-hidden border-r border-base-300/80 bg-base-100/70 xl:block">
			<FolderTree 
				selectedFolderId={currentFolderId}
				onSelectFolder={handleFolderSelectFromTree}
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
			{showBreadcrumbs}
			{canCreateFolder}
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
			{onShareFile}
			{onVersionHistory}
			{onMoveFile}
			{onDownloadFile}
			{onReplaceFile}
			{onRenameFolder}
			{onDeleteFolder}
			{onShareFolder}
			{onMoveFolder}
		/>
	</div>
</div>
