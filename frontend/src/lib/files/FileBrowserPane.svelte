<script lang="ts">
	import type { File as FileType, Folder } from '$lib/api/types';
	import type { ReplicationStatus } from '$lib/stores/replication';
	import { viewMode } from '$lib/stores/fileBrowserUi';
	import FileToolbar from './FileToolbar.svelte';
	import FileList from './FileList.svelte';
	import FileGrid from './FileGrid.svelte';
	import Breadcrumbs from '$lib/components/layout/Breadcrumbs.svelte';

	// Props
	interface Props {
		folders?: Folder[];
		files?: FileType[];
		folderPath?: Folder[];
		title?: string;
		description?: string;
		emptyTitle?: string;
		emptyDescription?: string;
		emptyActionLabel?: string | null;
		workspaceMode?: 'all' | 'photos' | 'recent' | 'starred' | 'deleted';
		showBreadcrumbs?: boolean;
		canCreateFolder?: boolean;
		canUpload?: boolean;
		allowSelectionMode?: boolean;
		isLoading?: boolean;
		error?: Error | null;
		replicationStatuses?: Record<string, ReplicationStatus>;
		selectionMode?: boolean;
		isUploading?: boolean;
		onRefresh?: () => void;
		onNewFolder?: () => void;
		onUpload?: () => void;
		onToggleSelection?: () => void;
		onSelectAll?: () => void;
		onDeselectAll?: () => void;
		onBulkDelete?: () => void;
		onBulkDownload?: () => void;
		onBulkMove?: () => void;
		onFolderClick?: (folder: Folder) => void;
		onFileClick?: (file: FileType) => void;
		onRenameFile?: ((file: FileType) => void) | ((file: FileType, newName: string) => void);
		onDeleteFile?: (file: FileType) => void;
		onToggleFileStar?: (file: FileType) => void;
		onRestoreFile?: (file: FileType) => void;
		onPermanentDeleteFile?: (file: FileType) => void;
		onShareFile?: (file: FileType) => void;
		onVersionHistory?: (file: FileType) => void;
		onMoveFile?: ((file: FileType) => void) | ((file: FileType, targetFolderId: string | null) => void);
		onDownloadFile?: (file: FileType) => void;
		onReplaceFile?: (file: FileType) => void;
		onEditFile?: (file: FileType) => void;
		onRenameFolder?: ((folder: Folder) => void) | ((folder: Folder, newName: string) => void);
		onDeleteFolder?: (folder: Folder) => void;
		onToggleFolderStar?: (folder: Folder) => void;
		onRestoreFolder?: (folder: Folder) => void;
		onPermanentDeleteFolder?: (folder: Folder) => void;
		onShareFolder?: (folder: Folder) => void;
		onMoveFolder?: ((folder: Folder) => void) | ((folder: Folder, targetFolderId: string | null) => void);
		onbreadcrumbNavigate?: (event: CustomEvent<{ folderId: string | null }>) => void;
	}

	let {
		folders = [],
		files = [],
		folderPath = [],
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
		replicationStatuses = {},
		selectionMode = false,
		isUploading = false,
		onRefresh = () => {},
		onNewFolder = () => {},
		onUpload = () => {},
		onToggleSelection = () => {},
		onSelectAll = () => {},
		onDeselectAll = () => {},
		onBulkDelete = () => {},
		onBulkDownload = () => {},
		onBulkMove = () => {},
		onFolderClick = () => {},
		onFileClick = () => {},
		onRenameFile = () => {},
		onDeleteFile = () => {},
		onToggleFileStar = () => {},
		onRestoreFile = () => {},
		onPermanentDeleteFile = () => {},
		onShareFile = () => {},
		onVersionHistory = () => {},
		onMoveFile = () => {},
		onDownloadFile = () => {},
		onReplaceFile = () => {},
		onEditFile = () => {},
		onRenameFolder = () => {},
		onDeleteFolder = () => {},
		onToggleFolderStar = () => {},
		onRestoreFolder = () => {},
		onPermanentDeleteFolder = () => {},
		onShareFolder = () => {},
		onMoveFolder = () => {},
		onbreadcrumbNavigate = () => {}
	}: Props = $props();
</script>

<div class="flex h-full min-h-0 flex-col bg-base-100">
	<!-- Toolbar -->
	<div class="border-b border-base-300/50 px-4 py-3 md:px-5 lg:px-6">
		<FileToolbar
			{title}
			{description}
			{canCreateFolder}
			{canUpload}
			{allowSelectionMode}
			{selectionMode}
			{isUploading}
			onToggleSelection={onToggleSelection}
			onSelectAll={onSelectAll}
			onDeselectAll={onDeselectAll}
			onBulkDelete={onBulkDelete}
			onBulkDownload={onBulkDownload}
			onBulkMove={onBulkMove}
			onNewFolder={onNewFolder}
			onUpload={onUpload}
		/>
	</div>

	<!-- Breadcrumbs -->
	{#if showBreadcrumbs}
		<div class="border-b border-base-300/50 bg-base-200/30 px-4 py-2 md:px-5 lg:px-6">
			<Breadcrumbs {folderPath} on:navigate={onbreadcrumbNavigate} />
		</div>
	{/if}

	<!-- Content -->
	<div class="flex-1 overflow-auto px-4 py-4 md:px-5 lg:px-6">
		{#if isLoading}
			<div class="flex items-center justify-center h-64">
				<div class="flex items-center gap-3 text-base-content/50">
					<div class="w-6 h-6 border-2 border-brand-500/30 border-t-brand-500 rounded-full animate-spin"></div>
					<span class="text-sm">Loading...</span>
				</div>
			</div>
		{:else if error}
			<div class="flex flex-col items-center justify-center h-64 text-center">
				<div class="w-14 h-14 rounded-xl bg-error/10 flex items-center justify-center mb-3">
					<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="w-7 h-7 text-error">
						<circle cx="12" cy="12" r="10"/>
						<line x1="12" x2="12" y1="8" y2="12"/>
						<line x1="12" x2="12.01" y1="16" y2="16"/>
					</svg>
				</div>
				<h3 class="text-base font-semibold text-base-content mb-1">Failed to load files</h3>
				<p class="text-sm text-base-content/50 mb-4">{error.message || 'Unknown error'}</p>
				<button
					type="button"
					class="px-4 py-2 text-sm font-medium bg-brand-500 hover:bg-brand-600 text-white rounded-lg transition-colors"
					onclick={onRefresh}
				>
					Try again
				</button>
			</div>
		{:else}
			{#if $viewMode === 'grid'}
				<FileGrid
					{folders}
					{files}
					{emptyTitle}
					{emptyDescription}
					{emptyActionLabel}
					{workspaceMode}
					{replicationStatuses}
					{selectionMode}
					onFolderClick={onFolderClick}
					onFileClick={onFileClick}
					onRenameFolder={onRenameFolder}
					onDeleteFolder={onDeleteFolder}
					onToggleFolderStar={onToggleFolderStar}
					onRestoreFolder={onRestoreFolder}
					onPermanentDeleteFolder={onPermanentDeleteFolder}
					onShareFolder={onShareFolder}
					onMoveFolder={onMoveFolder}
					onRenameFile={onRenameFile}
					onDeleteFile={onDeleteFile}
					onToggleFileStar={onToggleFileStar}
					onRestoreFile={onRestoreFile}
					onPermanentDeleteFile={onPermanentDeleteFile}
					onMoveFile={onMoveFile}
					onDownloadFile={onDownloadFile}
					onReplaceFile={onReplaceFile}
					onEditFile={onEditFile}
					onShareFile={onShareFile}
					onVersionHistory={onVersionHistory}
				/>
			{:else}
				<FileList
					{folders}
					{files}
					{emptyTitle}
					{emptyDescription}
					{emptyActionLabel}
					{workspaceMode}
					{replicationStatuses}
					{selectionMode}
					onFolderClick={onFolderClick}
					onFileClick={onFileClick}
					onRenameFolder={onRenameFolder}
					onDeleteFolder={onDeleteFolder}
					onToggleFolderStar={onToggleFolderStar}
					onRestoreFolder={onRestoreFolder}
					onPermanentDeleteFolder={onPermanentDeleteFolder}
					onShareFolder={onShareFolder}
					onMoveFolder={onMoveFolder}
					onRenameFile={onRenameFile}
					onDeleteFile={onDeleteFile}
					onToggleFileStar={onToggleFileStar}
					onRestoreFile={onRestoreFile}
					onPermanentDeleteFile={onPermanentDeleteFile}
					onMoveFile={onMoveFile}
					onDownloadFile={onDownloadFile}
					onReplaceFile={onReplaceFile}
					onEditFile={onEditFile}
					onShareFile={onShareFile}
					onVersionHistory={onVersionHistory}
				/>
			{/if}
		{/if}
	</div>
</div>
