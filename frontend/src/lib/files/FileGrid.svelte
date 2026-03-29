<script lang="ts">
	import type { File as FileType, Folder } from '$lib/api/types';
	import type { ReplicationStatus } from '$lib/stores/replication';
	import { selectionStore } from '$lib/stores/selection';
	import FileGridTile from './FileGridTile.svelte';

	export let folders: Folder[] = [];
	export let files: FileType[] = [];
	export let emptyTitle = 'This folder is empty';
	export let emptyDescription = 'Upload files or create folders to get started';
	export let emptyActionLabel: string | null = 'Upload files';
	export let onFolderClick: (folder: Folder) => void;
	export let onFileClick: (file: FileType) => void;
	export let onRenameFolder: (folder: Folder) => void = () => {};
	export let onDeleteFolder: (folder: Folder) => void = () => {};
	export let onShareFolder: (folder: Folder) => void = () => {};
	export let onMoveFolder: (folder: Folder) => void = () => {};
	export let onRenameFile: (file: FileType) => void = () => {};
	export let onDeleteFile: (file: FileType) => void = () => {};
	export let onShareFile: (file: FileType) => void = () => {};
	export let onVersionHistory: (file: FileType) => void = () => {};
	export let onMoveFile: (file: FileType) => void = () => {};
	export let onDownloadFile: (file: FileType) => void = () => {};
	export let onReplaceFile: (file: FileType) => void = () => {};
	export let selectionMode = false;
	export let replicationStatuses: Record<string, ReplicationStatus> = {};

	function handleFileToggle(file: FileType, event?: MouseEvent) {
		const isShiftKey = event?.shiftKey ?? false;
		const allFileIds = files.map(f => f.id);
		selectionStore.toggleFile(file.id, isShiftKey, allFileIds);
	}

	function handleFolderToggle(folder: Folder, event?: MouseEvent) {
		const isShiftKey = event?.shiftKey ?? false;
		const allFolderIds = folders.map(f => f.id);
		selectionStore.toggleFolder(folder.id, isShiftKey, allFolderIds);
	}
</script>

{#if folders.length === 0 && files.length === 0}
	<div class="flex flex-col items-center justify-center py-16 text-center">
		<div class="w-16 h-16 rounded-2xl bg-base-200 flex items-center justify-center mb-4">
			<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="w-8 h-8 text-base-content/30">
				<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>
			</svg>
		</div>
		<h3 class="text-lg font-semibold text-base-content mb-1">{emptyTitle}</h3>
		<p class="text-sm text-base-content/60 mb-4">{emptyDescription}</p>
		{#if emptyActionLabel}
			<button
				type="button"
				class="px-4 py-2 text-sm font-medium bg-brand-500 hover:bg-brand-600 text-white rounded-lg transition-colors"
				on:click={() => document.getElementById('upload-file-input')?.click()}
			>
				{emptyActionLabel}
			</button>
		{/if}
	</div>
{:else}
	<div class="grid grid-cols-1 gap-4 sm:grid-cols-[repeat(auto-fill,minmax(16rem,1fr))] xl:grid-cols-[repeat(auto-fill,minmax(17rem,1fr))]">
		<!-- Folders -->
		{#each folders as folder (folder.id)}
			<FileGridTile
				item={folder}
				isFolder={true}
				selected={selectionMode && $selectionStore.selectedFolderIds.has(folder.id)}
				{selectionMode}
				onSelect={() => onFolderClick(folder)}
				onToggle={() => handleFolderToggle(folder)}
				onRename={() => onRenameFolder(folder)}
				onDelete={() => onDeleteFolder(folder)}
				onShare={() => onShareFolder(folder)}
				onMove={() => onMoveFolder(folder)}
			/>
		{/each}

		<!-- Files -->
		{#each files as file (file.id)}
			<FileGridTile
				item={file}
				isFolder={false}
				selected={selectionMode && $selectionStore.selectedFileIds.has(file.id)}
				{selectionMode}
				replicationStatus={replicationStatuses[file.id]}
				onSelect={() => onFileClick(file)}
				onToggle={() => handleFileToggle(file)}
				onRename={() => onRenameFile(file)}
				onDelete={() => onDeleteFile(file)}
				onShare={() => onShareFile(file)}
				onMove={() => onMoveFile(file)}
				onDownload={() => onDownloadFile(file)}
				onVersionHistory={() => onVersionHistory(file)}
				onReplace={() => onReplaceFile(file)}
			/>
		{/each}
	</div>
{/if}
