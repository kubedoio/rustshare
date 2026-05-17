<script lang="ts">
	import type { File, Folder } from '$lib/api/types';
	import type { ReplicationStatus } from '$lib/stores/replication';
	import FileListItem from './FileListItem.svelte';
	import { selectionStore } from '$lib/stores/selection';

	interface Props {
		folders?: Folder[];
		files?: File[];
		onFolderClick: (folder: Folder) => void;
		onFileClick: (file: File) => void;
		onRenameFolder?: (folder: Folder) => void;
		onDeleteFolder?: (folder: Folder) => void;
		onShareFolder?: (folder: Folder) => void;
		onRenameFile?: (file: File) => void;
		onDeleteFile?: (file: File) => void;
		onShareFile?: (file: File) => void;
		onVersionHistory?: (file: File) => void;
		onMoveFolder?: (folder: Folder) => void;
		onMoveFile?: (file: File) => void;
		onDownloadFile?: (file: File) => void;
		onReplaceFile?: (file: File) => void;
		onEditFile?: (file: File) => void;
		selectionMode?: boolean;
		replicationStatuses?: Record<string, ReplicationStatus>;
	}

	let {
		folders = [],
		files = [],
		onFolderClick,
		onFileClick,
		onRenameFolder = () => {},
		onDeleteFolder = () => {},
		onShareFolder = () => {},
		onRenameFile = () => {},
		onDeleteFile = () => {},
		onShareFile = () => {},
		onVersionHistory = () => {},
		onMoveFolder = () => {},
		onMoveFile = () => {},
		onDownloadFile = () => {},
		onReplaceFile = () => {},
		onEditFile = () => {},
		selectionMode = false,
		replicationStatuses = {}
	}: Props = $props();

	function handleFileToggle(file: File, event?: MouseEvent) {
		const isShiftKey = event?.shiftKey ?? false;
		const allFileIds = files.map((f) => f.id);
		selectionStore.toggleFile(file.id, isShiftKey, allFileIds);
	}

	function handleFolderToggle(folder: Folder, event?: MouseEvent) {
		const isShiftKey = event?.shiftKey ?? false;
		const allFolderIds = folders.map((f) => f.id);
		selectionStore.toggleFolder(folder.id, isShiftKey, allFolderIds);
	}

	function handleVersionHistoryClick(e: CustomEvent) {
		onVersionHistory(e.detail.item);
	}
</script>

{#if folders.length === 0 && files.length === 0}
	<div class="py-16 text-center lg:py-24">
		<svg
			xmlns="http://www.w3.org/2000/svg"
			fill="none"
			viewBox="0 0 24 24"
			stroke-width="1.5"
			stroke="currentColor"
			class="mx-auto mb-4 h-20 w-20 text-base-content/20 lg:h-24 lg:w-24"
		>
			<path
				stroke-linecap="round"
				stroke-linejoin="round"
				d="M2.25 12.75V12A2.25 2.25 0 014.5 9.75h15A2.25 2.25 0 0121.75 12v.75m-8.69-6.44l-2.12-2.12a1.5 1.5 0 00-1.061-.44H4.5A2.25 2.25 0 002.25 6v12a2.25 2.25 0 002.25 2.25h15A2.25 2.25 0 0021.75 18V9a2.25 2.25 0 00-2.25-2.25h-5.379a1.5 1.5 0 01-1.06-.44z"
			/>
		</svg>
		<p class="mb-2 text-lg text-base-content/60 lg:text-xl">This folder is empty</p>
		<p class="text-sm text-base-content/40">Upload files or create folders to get started</p>
	</div>
{:else}
	<div class="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
		{#each folders as folder}
			<FileListItem
				item={folder}
				isFolder={true}
				onSelect={(e) => (selectionMode ? handleFolderToggle(folder, e) : onFolderClick(folder))}
				selected={selectionMode && $selectionStore.selectedFolderIds.has(folder.id)}
				{selectionMode}
				onRename={() => onRenameFolder(folder)}
				onDelete={() => onDeleteFolder(folder)}
				onShare={() => onShareFolder(folder)}
				onMove={() => onMoveFolder(folder)}
			/>
		{/each}

		{#each files as file}
			<FileListItem
				item={file}
				isFolder={false}
				replicationStatus={replicationStatuses[file.id] ?? null}
				onSelect={(e) => (selectionMode ? handleFileToggle(file, e) : onFileClick(file))}
				selected={selectionMode && $selectionStore.selectedFileIds.has(file.id)}
				{selectionMode}
				onRename={() => onRenameFile(file)}
				onDelete={() => onDeleteFile(file)}
				onShare={() => onShareFile(file)}
				onVersionHistory={() => onVersionHistory(file)}
				onMove={() => onMoveFile(file)}
				onDownload={() => onDownloadFile(file)}
				onReplace={() => onReplaceFile(file)}
				{onEditFile}
			/>
		{/each}
	</div>
{/if}
