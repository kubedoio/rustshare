<script lang="ts">
	import type { File as FileType, Folder } from '$lib/api/types';
	import type { ReplicationStatus } from '$lib/stores/replication';
	import { fileSortState } from '$lib/stores/fileSort';
	import FileToolbar from './FileToolbar.svelte';
	import FileList from './FileList.svelte';
	import FileGrid from './FileGrid.svelte';
	import Breadcrumbs from '$lib/components/layout/Breadcrumbs.svelte';
	import { createEventDispatcher } from 'svelte';

	export let folders: Folder[] = [];
	export let files: FileType[] = [];
	export let folderPath: Folder[] = [];
	export let isLoading: boolean = false;
	export let error: Error | null = null;
	export let replicationStatuses: Record<string, ReplicationStatus> = {};
	export let selectionMode: boolean = false;
	export let isUploading: boolean = false;

	export let onRefresh: () => void;
	export let onNewFolder: () => void;
	export let onUpload: () => void;
	export let onToggleSelection: () => void;
	export let onSelectAll: () => void;
	export let onDeselectAll: () => void;
	export let onBulkDelete: () => void;
	export let onBulkDownload: () => void;
	export let onBulkMove: () => void;

	export let onFolderClick: (folder: Folder) => void;
	export let onFileClick: (file: FileType) => void;

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

	const dispatch = createEventDispatcher();

	function handleBreadcrumbNavigate(event: CustomEvent<{ folderId: string | null }>) {
		dispatch('breadcrumbNavigate', event.detail);
	}
</script>

<div class="flex h-full min-h-0 flex-col bg-base-100">
	<!-- Toolbar -->
	<div class="border-b border-base-300/80 px-4 py-3 md:px-5 md:py-4 lg:px-6">
		<FileToolbar
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
	<div class="border-b border-base-300/70 bg-base-200/35 px-4 py-2.5 md:px-5 lg:px-6">
		<Breadcrumbs {folderPath} on:navigate={handleBreadcrumbNavigate} />
	</div>

	<!-- Content -->
	<div class="flex-1 overflow-auto px-4 py-4 md:px-5 lg:px-6 lg:py-5 xl:px-8">
		{#if isLoading}
			<div class="flex items-center justify-center h-64">
				<div class="animate-spin w-8 h-8 border-2 border-brand-500 border-t-transparent rounded-full"></div>
			</div>
		{:else if error}
			<div class="flex flex-col items-center justify-center h-64 text-center">
				<div class="w-16 h-16 rounded-2xl bg-error/10 flex items-center justify-center mb-4">
					<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="w-8 h-8 text-error">
						<circle cx="12" cy="12" r="10"/>
						<line x1="12" x2="12" y1="8" y2="12"/>
						<line x1="12" x2="12.01" y1="16" y2="16"/>
					</svg>
				</div>
				<h3 class="text-lg font-semibold text-base-content mb-1">Failed to load files</h3>
				<p class="text-sm text-base-content/60 mb-4">{error.message || 'Unknown error'}</p>
				<button
					type="button"
					class="px-4 py-2 text-sm font-medium bg-brand-500 hover:bg-brand-600 text-white rounded-lg transition-colors"
					on:click={onRefresh}
				>
					Try again
				</button>
			</div>
		{:else}
			{#if $fileSortState.viewMode === 'grid'}
				<FileGrid
					{folders}
					{files}
					{replicationStatuses}
					{selectionMode}
					onFolderClick={onFolderClick}
					onFileClick={onFileClick}
					onRenameFolder={onRenameFolder}
					onDeleteFolder={onDeleteFolder}
					onShareFolder={onShareFolder}
					onMoveFolder={onMoveFolder}
					onRenameFile={onRenameFile}
					onDeleteFile={onDeleteFile}
					onMoveFile={onMoveFile}
					onDownloadFile={onDownloadFile}
					onReplaceFile={onReplaceFile}
					onShareFile={onShareFile}
					onVersionHistory={onVersionHistory}
				/>
			{:else}
				<FileList
					{folders}
					{files}
					{replicationStatuses}
					{selectionMode}
					onFolderClick={onFolderClick}
					onFileClick={onFileClick}
					onRenameFolder={onRenameFolder}
					onDeleteFolder={onDeleteFolder}
					onShareFolder={onShareFolder}
					onMoveFolder={onMoveFolder}
					onRenameFile={onRenameFile}
					onDeleteFile={onDeleteFile}
					onMoveFile={onMoveFile}
					onDownloadFile={onDownloadFile}
					onReplaceFile={onReplaceFile}
					onShareFile={onShareFile}
					onVersionHistory={onVersionHistory}
				/>
			{/if}
		{/if}
	</div>
</div>
