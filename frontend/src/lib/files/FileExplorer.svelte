<script lang="ts">
	import type { File as FileType, Folder } from '$lib/api/types';
	import type { ReplicationStatus } from '$lib/stores/replication';
	import type { SortField, SortOrder } from '$lib/stores/fileSort';

	import FileBrowserPane from './FileBrowserPane.svelte';

	interface Props {
		folders?: Folder[];
		files?: FileType[];
		folderPath?: Folder[];
		rootLabel?: string;
		title?: string;
		description?: string;
		emptyTitle?: string;
		emptyDescription?: string;
		emptyActionLabel?: string | null;
		workspaceMode?: 'all' | 'photos' | 'recent' | 'starred' | 'deleted' | 'week';
		showBreadcrumbs?: boolean;
		canCreateFolder?: boolean;
		canUpload?: boolean;
		allowSelectionMode?: boolean;
		isLoading?: boolean;
		error?: Error | null;
	
		selectionMode?: boolean;
		isUploading?: boolean;
		isSharedRoot?: boolean;
		activeSortField?: SortField;
		activeSortOrder?: SortOrder;
		onSort?: (field: SortField) => void;
		onFolderClick: (folder: Folder) => void;
		onFileClick: (file: FileType) => void;
		onRefresh: () => void;
		onNewFolder: () => void;
		onUpload: () => void;
		onToggleSelection: () => void;
		onSelectAll: () => void;
		onDeselectAll: () => void;
		onBulkDelete: () => void;
		onBulkDownload: () => void;
		onBulkMove: () => void;
		onRenameFile: ((file: FileType) => void) | ((file: FileType, newName: string) => void);
		onDeleteFile: (file: FileType) => void;
		onToggleFileStar: (file: FileType) => void;
		onRestoreFile: (file: FileType) => void;
		onPermanentDeleteFile: (file: FileType) => void;
		onShareFile: (file: FileType) => void;
		onVersionHistory: (file: FileType) => void;
		onMoveFile:
			| ((file: FileType) => void)
			| ((file: FileType, targetFolderId: string | null) => void);
		onDownloadFile: (file: FileType) => void;
		onReplaceFile: (file: FileType) => void;
		onRenameFolder: ((folder: Folder) => void) | ((folder: Folder, newName: string) => void);
		onDeleteFolder: (folder: Folder) => void;
		onToggleFolderStar: (folder: Folder) => void;
		onRestoreFolder: (folder: Folder) => void;
		onPermanentDeleteFolder: (folder: Folder) => void;
		onShareFolder: (folder: Folder) => void;
		onMoveFolder:
			| ((folder: Folder) => void)
			| ((folder: Folder, targetFolderId: string | null) => void);
		onEditFile: (file: FileType) => void;
		onbreadcrumbNavigate?: (event: CustomEvent<{ folderId: string | null }>) => void;
		pagination?: import('svelte').Snippet;
	}

	let {
		folders = [],
		files = [],
		folderPath = [],
		rootLabel = 'My Files',
		title = 'All files',
		description = '',
		emptyTitle = 'No files yet',
		emptyDescription = 'Upload your first file to get started',
		emptyActionLabel = 'Upload files',
		workspaceMode = 'all',
		showBreadcrumbs = true,
		canCreateFolder = true,
		canUpload = true,
		allowSelectionMode = true,
		isLoading = false,
		error = null,
	
		selectionMode = false,
		isUploading = false,
		isSharedRoot = false,
		activeSortField = 'name',
		activeSortOrder = 'asc',
		onSort = () => {},
		onFolderClick,
		onFileClick,
		onRefresh,
		onNewFolder,
		onUpload,
		onToggleSelection,
		onSelectAll,
		onDeselectAll,
		onBulkDelete,
		onBulkDownload,
		onBulkMove,
		onRenameFile,
		onDeleteFile,
		onToggleFileStar,
		onRestoreFile,
		onPermanentDeleteFile,
		onShareFile,
		onVersionHistory,
		onMoveFile,
		onDownloadFile,
		onReplaceFile,
		onRenameFolder,
		onDeleteFolder,
		onToggleFolderStar,
		onRestoreFolder,
		onPermanentDeleteFolder,
		onShareFolder,
		onMoveFolder,
		onEditFile,
		onbreadcrumbNavigate = () => {},
		pagination
	}: Props = $props();
</script>

<div class="flex h-full min-h-0 bg-base-100">
	<!-- File Browser Pane -->
	<div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
		<FileBrowserPane
			{folders}
			{files}
			{folderPath}
			{rootLabel}
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

			{selectionMode}
			{isUploading}
			{isSharedRoot}
			{activeSortField}
			{activeSortOrder}
			{onSort}
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
			{onbreadcrumbNavigate}
		>
			{#snippet pagination()}
				{@render pagination?.()}
			{/snippet}
		</FileBrowserPane>
	</div>
</div>
