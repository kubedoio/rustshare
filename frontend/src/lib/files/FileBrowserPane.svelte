<script lang="ts">
	import type { File as FileType, Folder } from '$lib/api/types';
	import type { ReplicationStatus } from '$lib/stores/replication';
	import type { SortField, SortOrder } from '$lib/stores/fileSort';
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
		isSharedRoot?: boolean;
		isUploading?: boolean;
		activeSortField?: SortField;
		activeSortOrder?: SortOrder;
		onSort?: (field: SortField) => void;
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
		onMoveFile?:
			((file: FileType) => void) | ((file: FileType, targetFolderId: string | null) => void);
		onDownloadFile?: (file: FileType) => void;
		onReplaceFile?: (file: FileType) => void;
		onEditFile?: (file: FileType) => void;
		onRenameFolder?: ((folder: Folder) => void) | ((folder: Folder, newName: string) => void);
		onDeleteFolder?: (folder: Folder) => void;
		onToggleFolderStar?: (folder: Folder) => void;
		onRestoreFolder?: (folder: Folder) => void;
		onPermanentDeleteFolder?: (folder: Folder) => void;
		onShareFolder?: (folder: Folder) => void;
		onMoveFolder?:
			((folder: Folder) => void) | ((folder: Folder, targetFolderId: string | null) => void);
		onbreadcrumbNavigate?: (event: CustomEvent<{ folderId: string | null }>) => void;
		pagination?: import('svelte').Snippet;
	}

	let {
		folders = [],
		files = [],
		folderPath = [],
		rootLabel = 'My Files',
		isSharedRoot = false,
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
		activeSortField = 'name',
		activeSortOrder = 'asc',
		onSort = () => {},
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
		onEditFile: _editFile = () => {},
		onRenameFolder = () => {},
		onDeleteFolder = () => {},
		onToggleFolderStar = () => {},
		onRestoreFolder = () => {},
		onPermanentDeleteFolder = () => {},
		onShareFolder = () => {},
		onMoveFolder = () => {},
		onbreadcrumbNavigate = () => {},
		pagination
	}: Props = $props();
</script>

<div class="flex h-full min-h-0 flex-col bg-base-100">
	<!-- Toolbar -->
	<div class="border-b border-base-300/50 px-3 py-2 md:px-4 lg:px-5">
		<FileToolbar
			{title}
			{description}
			{canCreateFolder}
			{canUpload}
			{allowSelectionMode}
			{selectionMode}
			{isUploading}
			{onToggleSelection}
			{onSelectAll}
			{onDeselectAll}
			{onBulkDelete}
			{onBulkDownload}
			{onBulkMove}
			{onNewFolder}
			{onUpload}
		/>
	</div>

	<!-- Breadcrumbs -->
	{#if showBreadcrumbs}
		<div class="border-b border-base-300/50 bg-base-200/30 px-3 py-1.5 md:px-4 lg:px-5">
			<Breadcrumbs
				{folderPath}
				{rootLabel}
				onNavigate={(payload) =>
					onbreadcrumbNavigate(new CustomEvent('navigate', { detail: payload }))}
			/>
		</div>
	{/if}

	<!-- Content -->
	<div class="flex-1 overflow-auto px-3 py-3 md:px-4 lg:px-5">
		{#if isLoading}
			<div class="flex h-64 items-center justify-center">
				<div class="flex items-center gap-3 text-base-content/50">
					<div
						class="h-6 w-6 animate-spin rounded-full border-2 border-brand-500/30 border-t-brand-500"
					></div>
					<span class="text-sm">Loading...</span>
				</div>
			</div>
		{:else if error}
			<div class="flex h-64 flex-col items-center justify-center text-center">
				<div class="mb-3 flex h-14 w-14 items-center justify-center rounded-xl bg-error/10">
					<svg
						xmlns="http://www.w3.org/2000/svg"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						stroke-width="1.5"
						class="h-7 w-7 text-error"
					>
						<circle cx="12" cy="12" r="10" />
						<line x1="12" x2="12" y1="8" y2="12" />
						<line x1="12" x2="12.01" y1="16" y2="16" />
					</svg>
				</div>
				<h3 class="mb-1 text-base font-semibold text-base-content">Failed to load files</h3>
				<p class="mb-4 text-sm text-base-content/50">{error.message || 'Unknown error'}</p>
				<button
					type="button"
					class="rounded-lg bg-brand-500 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-600"
					onclick={onRefresh}
				>
					Try again
				</button>
			</div>
		{:else if $viewMode === 'grid'}
			<FileGrid
				{folders}
				{files}
				{isSharedRoot}
				{emptyTitle}
				{emptyDescription}
				{emptyActionLabel}
				{workspaceMode}
				{selectionMode}
				{onFolderClick}
				{onFileClick}
				{onRenameFolder}
				{onDeleteFolder}
				{onToggleFolderStar}
				{onRestoreFolder}
				{onPermanentDeleteFolder}
				{onShareFolder}
				{onMoveFolder}
				{onRenameFile}
				{onDeleteFile}
				{onToggleFileStar}
				{onRestoreFile}
				{onPermanentDeleteFile}
				{onMoveFile}
				{onDownloadFile}
				{onReplaceFile}
				onEditFile={(f) => {
					_editFile(f);
				}}
				{onShareFile}
				{onVersionHistory}
			/>
		{:else}
			<FileList
				{folders}
				{files}
				{isSharedRoot}
				{emptyTitle}
				{emptyDescription}
				{emptyActionLabel}
				{workspaceMode}
				{selectionMode}
				{activeSortField}
				{activeSortOrder}
				{onSort}
				{onFolderClick}
				{onFileClick}
				{onRenameFolder}
				{onDeleteFolder}
				{onToggleFolderStar}
				{onRestoreFolder}
				{onPermanentDeleteFolder}
				{onShareFolder}
				{onMoveFolder}
				{onRenameFile}
				{onDeleteFile}
				{onToggleFileStar}
				{onRestoreFile}
				{onPermanentDeleteFile}
				{onMoveFile}
				{onDownloadFile}
				{onReplaceFile}
				onEditFile={(f) => {
					_editFile(f);
				}}
				{onShareFile}
				{onVersionHistory}
			/>
		{/if}
	</div>

	{#if !isLoading && !error && (folders.length > 0 || files.length > 0)}
		<div class="border-t border-base-300/50 px-3 py-2 md:px-4 lg:px-5">
			{@render pagination?.()}
		</div>
	{/if}
</div>
