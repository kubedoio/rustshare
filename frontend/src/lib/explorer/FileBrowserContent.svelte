<script lang="ts">
	import type { File as FileType, Folder } from '$lib/api/types';
	
	import type { SortField, SortOrder } from '$lib/stores/fileSort';
	import { viewMode } from '$lib/stores/fileBrowserUi';
	import { filterUserVisibleEntries } from '$lib/utils/artifactVisibility';
	import FileList from '$lib/files/FileList.svelte';
	import FileGrid from '$lib/files/FileGrid.svelte';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import FileListSkeleton from '$lib/components/common/FileListSkeleton.svelte';
	import { FolderOpen } from 'lucide-svelte';

	interface Props {
		folders?: Folder[];
		files?: FileType[];
		viewMode?: 'grid' | 'list';
		workspaceMode?: 'all' | 'photos' | 'recent' | 'starred' | 'deleted' | 'week';
		isSharedRoot?: boolean;
		isLoading?: boolean;
		error?: Error | null;
		emptyTitle?: string;
		emptyDescription?: string;
		emptyActionLabel?: string | null;
	
		selectionMode?: boolean;
		activeSortField?: SortField;
		activeSortOrder?: SortOrder;
		onSort?: (field: SortField) => void;
		onRefresh?: () => void;
		onFolderClick?: (folder: Folder) => void;
		onFileClick?: (file: FileType) => void;
		onRenameFile?: (file: FileType, newName: string) => void;
		onDeleteFile?: (file: FileType) => void;
		onToggleFileStar?: (file: FileType) => void;
		onRestoreFile?: (file: FileType) => void;
		onPermanentDeleteFile?: (file: FileType) => void;
		onShareFile?: (file: FileType) => void;
		onVersionHistory?: (file: FileType) => void;
		onMoveFile?: (file: FileType, targetFolderId: string | null) => void;
		onDownloadFile?: (file: FileType) => void;
		onReplaceFile?: (file: FileType) => void;
		onEditFile?: (file: FileType) => void;
		onRenameFolder?: (folder: Folder, newName: string) => void;
		onDeleteFolder?: (folder: Folder) => void;
		onToggleFolderStar?: (folder: Folder) => void;
		onRestoreFolder?: (folder: Folder) => void;
		onPermanentDeleteFolder?: (folder: Folder) => void;
		onShareFolder?: (folder: Folder) => void;
		onMoveFolder?: (folder: Folder, targetFolderId: string | null) => void;
		pagination?: import('svelte').Snippet;
	}

	let {
		folders = [],
		files = [],
		viewMode: viewModeProp = undefined,
		workspaceMode = 'all',
		isSharedRoot = false,
		isLoading = false,
		error = null,
		emptyTitle = 'No files yet',
		emptyDescription = 'Upload your first file to get started',
		emptyActionLabel = 'Upload files',
	
		selectionMode = false,
		activeSortField = 'name',
		activeSortOrder = 'asc',
		onSort = () => {},
		onRefresh = () => {},
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
		pagination
	}: Props = $props();

	let effectiveViewMode = $derived(viewModeProp ?? $viewMode ?? 'list');
	let visibleFolders = $derived(folders);
	let visibleFiles = $derived(files);
</script>

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
	{:else if effectiveViewMode === 'grid'}
		<FileGrid
			folders={visibleFolders}
			files={visibleFiles}
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
				onEditFile(f);
			}}
			{onShareFile}
			{onVersionHistory}
		/>
	{:else}
		<FileList
			folders={visibleFolders}
			files={visibleFiles}
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
				onEditFile(f);
			}}
			{onShareFile}
			{onVersionHistory}
		/>
	{/if}
</div>

{#if !isLoading && !error && (visibleFolders.length > 0 || visibleFiles.length > 0)}
	<div class="border-t border-base-300/50 px-3 py-2 md:px-4 lg:px-5">
		{@render pagination?.()}
	</div>
{/if}
