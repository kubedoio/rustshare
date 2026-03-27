<script lang="ts">
	import type { File, Folder } from '$lib/api/types';
	import {
		formatReplicationStateLabel,
		replicationStateBadgeClass,
		type ReplicationStatus
	} from '$lib/stores/replication';
	import { selectionStore } from '$lib/stores/selection';
	import ShareIndicator from './ShareIndicator.svelte';

	export let folders: Folder[] = [];
	export let files: File[] = [];
	export let onFolderClick: (folder: Folder) => void = () => {};
	export let onFileClick: (file: File) => void = () => {};
	export let onRenameFolder: (folder: Folder) => void = () => {};
	export let onDeleteFolder: (folder: Folder) => void = () => {};
	export let onShareFolder: (folder: Folder) => void = () => {};
	export let onRenameFile: (file: File) => void = () => {};
	export let onDeleteFile: (file: File) => void = () => {};
	export let onShareFile: (file: File) => void = () => {};
	export let onVersionHistory: (file: File) => void = () => {};
	export let onMoveFolder: (folder: Folder) => void = () => {};
	export let onMoveFile: (file: File) => void = () => {};
	export let onDownloadFile: (file: File) => void = () => {};
	export let onReplaceFile: (file: File) => void = () => {};
	export let selectionMode = false;
	export let replicationStatuses: Record<string, ReplicationStatus> = {};

	let hoveredRow: string | null = null;

	function handleFileToggle(file: File) {
		selectionStore.toggleFile(file.id);
	}

	function handleFolderToggle(folder: Folder) {
		selectionStore.toggleFolder(folder.id);
	}

	function formatBytes(bytes: number): string {
		if (bytes === 0) return '0 Bytes';
		const k = 1024;
		const sizes = ['Bytes', 'KB', 'MB', 'GB', 'TB'];
		const i = Math.floor(Math.log(bytes) / Math.log(k));
		return Math.round((bytes / Math.pow(k, i)) * 100) / 100 + ' ' + sizes[i];
	}

	function formatDate(dateString: string): string {
		const date = new Date(dateString);
		return date.toLocaleDateString('en-US', {
			year: 'numeric',
			month: 'short',
			day: 'numeric'
		});
	}

	function getFileIcon(mimeType: string): string {
		if (mimeType.startsWith('image/')) return '🖼️';
		if (mimeType.startsWith('video/')) return '🎥';
		if (mimeType.startsWith('audio/')) return '🎵';
		if (mimeType.includes('pdf')) return '📄';
		if (mimeType.includes('zip') || mimeType.includes('tar')) return '📦';
		if (mimeType.includes('word') || mimeType.includes('document')) return '📝';
		if (mimeType.includes('sheet') || mimeType.includes('excel')) return '📊';
		if (mimeType.includes('presentation')) return '📽️';
		if (mimeType.includes('text/')) return '📃';
		return '📄';
	}

	function isSelected(item: File | Folder, isFolder: boolean): boolean {
		if (isFolder) {
			return $selectionStore.selectedFolderIds.has(item.id);
		}
		return $selectionStore.selectedFileIds.has(item.id);
	}

	function handleRowClick(item: File | Folder, isFolderItem: boolean, event: MouseEvent) {
		if (selectionMode) {
			if (isFolderItem) {
				handleFolderToggle(item as Folder);
			} else {
				handleFileToggle(item as File);
			}
		} else if (isFolderItem) {
			onFolderClick(item as Folder);
		} else {
			onFileClick(item as File);
		}
	}
</script>

<div class="w-full">
	<!-- Header -->
	<div class="grid grid-cols-[auto_1fr_140px_100px_140px_50px] gap-4 px-4 py-2 text-sm text-[#9ca3af] border-b border-[#2a2f35]">
		<div class="w-6"></div>
		<button class="flex items-center gap-1 hover:text-white text-left">
			Name
			<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-4 h-4">
				<path fill-rule="evenodd" d="M5.23 7.21a.75.75 0 011.06.02L10 11.168l3.71-3.938a.75.75 0 111.08 1.04l-4.25 4.5a.75.75 0 01-1.08 0l-4.25-4.5a.75.75 0 01.02-1.06z" clip-rule="evenodd" />
			</svg>
		</button>
		<div></div>
		<div class="text-right">Last modified</div>
		<div class="text-right">Size</div>
		<div class="text-right">Access</div>
	</div>

	<!-- Folders -->
	{#each folders as folder}
		{@const selected = isSelected(folder, true)}
		{@const isHovered = hoveredRow === `folder-${folder.id}`}
		<div
			class="grid grid-cols-[auto_1fr_140px_100px_140px_50px] gap-4 px-4 py-2.5 text-sm items-center cursor-pointer border-b border-transparent transition-colors"
			class:bg-[#1e3a5f]={selected}
			class:bg-opacity-60={selected}
			class:hover:bg-[#1a1d24]={!selected}
			on:mouseenter={() => hoveredRow = `folder-${folder.id}`}
			on:mouseleave={() => hoveredRow = null}
			on:click={(e) => handleRowClick(folder, true, e)}
			role="button"
			tabindex="0"
		>
			<!-- Checkbox or placeholder -->
			<div class="w-6 flex items-center justify-center">
				{#if selectionMode || isHovered || selected}
					<input
						type="checkbox"
						class="w-4 h-4 rounded border-[#4b5563] bg-transparent checked:bg-[#2563eb] checked:border-[#2563eb] focus:ring-0 focus:ring-offset-0 cursor-pointer"
						checked={selected}
						on:click|stopPropagation={() => handleFolderToggle(folder)}
					/>
				{/if}
			</div>

			<!-- Name with folder icon -->
			<div class="flex items-center gap-3 min-w-0">
				<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" class="w-6 h-6 text-[#2563eb] flex-shrink-0">
					<path d="M19.5 21a3 3 0 003-3v-4.5a3 3 0 00-3-3h-15a3 3 0 00-3 3V18a3 3 0 003 3h15zM1.5 10.146V6a3 3 0 013-3h5.379a2.25 2.25 0 011.59.659l2.122 2.121c.14.141.331.22.53.22H19.5a3 3 0 013 3v1.146A4.483 4.483 0 0019.5 9h-15a4.483 4.483 0 00-3 1.146z" />
				</svg>
				<span class="text-[#e5e7eb] truncate font-normal">{folder.name}</span>
			</div>

			<!-- Tags/Share indicator -->
			<div class="flex items-center justify-end">
				{#if folder.is_shared}
					<ShareIndicator
						isShared={folder.is_shared}
						shareCount={folder.share_count || 0}
						shareExpiresAt={folder.share_expires_at || null}
						size="sm"
					/>
				{/if}
			</div>

			<!-- Last modified -->
			<div class="text-[#9ca3af] text-right text-xs">{formatDate(folder.updated_at)}</div>

			<!-- Size -->
			<div class="text-[#9ca3af] text-right text-xs">—</div>

			<!-- Access -->
			<div class="text-[#9ca3af] text-right text-xs">Only you</div>
		</div>
	{/each}

	<!-- Files -->
	{#each files as file}
		{@const selected = isSelected(file, false)}
		{@const isHovered = hoveredRow === `file-${file.id}`}
		<div
			class="grid grid-cols-[auto_1fr_140px_100px_140px_50px] gap-4 px-4 py-2.5 text-sm items-center cursor-pointer border-b border-transparent transition-colors"
			class:bg-[#1e3a5f]={selected}
			class:bg-opacity-60={selected}
			class:hover:bg-[#1a1d24]={!selected}
			on:mouseenter={() => hoveredRow = `file-${file.id}`}
			on:mouseleave={() => hoveredRow = null}
			on:click={(e) => handleRowClick(file, false, e)}
			role="button"
			tabindex="0"
		>
			<!-- Checkbox or placeholder -->
			<div class="w-6 flex items-center justify-center">
				{#if selectionMode || isHovered || selected}
					<input
						type="checkbox"
						class="w-4 h-4 rounded border-[#4b5563] bg-transparent checked:bg-[#2563eb] checked:border-[#2563eb] focus:ring-0 focus:ring-offset-0 cursor-pointer"
						checked={selected}
						on:click|stopPropagation={() => handleFileToggle(file)}
					/>
				{/if}
			</div>

			<!-- Name with file icon -->
			<div class="flex items-center gap-3 min-w-0">
				<span class="text-xl flex-shrink-0">{getFileIcon(file.mime_type)}</span>
				<span class="text-[#e5e7eb] truncate font-normal">{file.name}</span>
				{#if file.is_shared}
					<ShareIndicator
						isShared={file.is_shared}
						shareCount={file.share_count || 0}
						shareExpiresAt={file.share_expires_at || null}
						size="sm"
					/>
				{/if}
				{#if replicationStatuses[file.id]}
					<span class={`badge badge-xs ${replicationStateBadgeClass(replicationStatuses[file.id].replicationState)}`}>
						{formatReplicationStateLabel(replicationStatuses[file.id].replicationState)}
					</span>
				{/if}
			</div>

			<!-- Tags -->
			<div></div>

			<!-- Last modified -->
			<div class="text-[#9ca3af] text-right text-xs">{formatDate(file.modified_at)}</div>

			<!-- Size -->
			<div class="text-[#9ca3af] text-right text-xs">{formatBytes(file.size)}</div>

			<!-- Access -->
			<div class="text-[#9ca3af] text-right text-xs">Only you</div>
		</div>
	{/each}

	{#if folders.length === 0 && files.length === 0}
		<div class="py-16 text-center text-[#6b7280]">
			<p>No files or folders</p>
		</div>
	{/if}
</div>

<style>
	/* Custom checkbox styling */
	input[type="checkbox"] {
		appearance: none;
		background-color: transparent;
		border: 1.5px solid #4b5563;
		border-radius: 3px;
		width: 16px;
		height: 16px;
		cursor: pointer;
		position: relative;
	}

	input[type="checkbox"]:checked {
		background-color: #2563eb;
		border-color: #2563eb;
	}

	input[type="checkbox"]:checked::after {
		content: '';
		position: absolute;
		left: 5px;
		top: 2px;
		width: 4px;
		height: 8px;
		border: solid white;
		border-width: 0 1.5px 1.5px 0;
		transform: rotate(45deg);
	}

	input[type="checkbox"]:hover {
		border-color: #6b7280;
	}

	input[type="checkbox"]:checked:hover {
		border-color: #2563eb;
	}
</style>
