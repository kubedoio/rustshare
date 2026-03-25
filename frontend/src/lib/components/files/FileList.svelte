<script lang="ts">
	import type { File, Folder } from '$lib/api/types';
	import {
		formatReplicationStateLabel,
		replicationStateBadgeClass,
		type ReplicationStatus
	} from '$lib/stores/replication';
	import { selectionStore } from '$lib/stores/selection';

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
			day: 'numeric',
			hour: '2-digit',
			minute: '2-digit'
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

	function handleContextMenu(event: MouseEvent, item: File | Folder, type: 'file' | 'folder') {
		event.preventDefault();
		// Context menu handled by FileListItem
	}
</script>

<div class="bg-base-100 rounded-lg shadow overflow-x-auto min-h-[200px]">
	<table class="table-zebra table">
		<thead>
			<tr>
				<th>Name</th>
				<th>Type</th>
				<th>Size</th>
				<th>Modified</th>
				<th class="text-right">Actions</th>
			</tr>
		</thead>
		<tbody>
			<!-- Folders -->
			{#each folders as folder}
				<tr class="hover">
					<td>
						<div class="gap-3 flex items-center">
							{#if selectionMode}
								<input
									type="checkbox"
									class="checkbox checkbox-sm"
									checked={$selectionStore.selectedFolderIds.has(folder.id)}
									on:click|stopPropagation={() => handleFolderToggle(folder)}
								/>
							{/if}
							<button
								type="button"
								class="btn btn-ghost btn-sm"
								on:click|stopPropagation={() => onFolderClick(folder)}
							>
								<span class="text-2xl">📁</span>
								<span class="font-medium">{folder.name}</span>
							</button>
						</div>
					</td>
					<td>
						<span class="badge badge-ghost">Folder</span>
					</td>
					<td>—</td>
					<td>{formatDate(folder.updated_at)}</td>
					<td class="text-right">
						<div class="dropdown dropdown-end dropdown-top">
							<button
								type="button"
								class="btn btn-ghost btn-xs"
								aria-label={`Open actions for folder ${folder.name}`}
								on:click|stopPropagation
							>
								<svg
									xmlns="http://www.w3.org/2000/svg"
									fill="none"
									viewBox="0 0 24 24"
									stroke-width="1.5"
									stroke="currentColor"
									class="w-4 h-4"
								>
									<path
										stroke-linecap="round"
										stroke-linejoin="round"
										d="M12 6.75a.75.75 0 110-1.5.75.75 0 010 1.5zM12 12.75a.75.75 0 110-1.5.75.75 0 010 1.5zM12 18.75a.75.75 0 110-1.5.75.75 0 010 1.5z"
									/>
								</svg>
							</button>
							<ul class="dropdown-content menu p-2 shadow bg-base-100 rounded-box w-52 z-50">
								<li>
									<button type="button" on:click|stopPropagation={() => onRenameFolder(folder)}
										>Rename</button
									>
								</li>
								<li>
									<button type="button" on:click|stopPropagation={() => onShareFolder(folder)}
										>Share</button
									>
								</li>
								<li>
									<button type="button" on:click|stopPropagation={() => onMoveFolder(folder)}
										>Move</button
									>
								</li>
								<li>
									<button
										type="button"
										on:click|stopPropagation={() => onDeleteFolder(folder)}
										class="text-error">Delete</button
									>
								</li>
							</ul>
						</div>
					</td>
				</tr>
			{/each}

			<!-- Files -->
			{#each files as file}
				<tr class="hover">
					<td>
						<div class="gap-3 flex items-center">
							{#if selectionMode}
								<input
									type="checkbox"
									class="checkbox checkbox-sm"
									checked={$selectionStore.selectedFileIds.has(file.id)}
									on:click|stopPropagation={() => handleFileToggle(file)}
								/>
							{/if}
							<span class="text-2xl">{getFileIcon(file.mime_type)}</span>
							<button
								type="button"
								class="font-medium hover:text-primary cursor-pointer text-left"
								on:click|stopPropagation={() => onFileClick(file)}
							>
								{file.name}
							</button>
							{#if replicationStatuses[file.id]}
								<div class="mt-1">
									<span
										class={`badge badge-xs ${replicationStateBadgeClass(replicationStatuses[file.id].replicationState)}`}
									>
										{formatReplicationStateLabel(replicationStatuses[file.id].replicationState)}
									</span>
								</div>
							{/if}
						</div>
					</td>
					<td>
						<span class="badge badge-ghost text-xs">{file.mime_type.split('/')[0]}</span>
					</td>
					<td>{formatBytes(file.size)}</td>
					<td>{formatDate(file.modified_at)}</td>
					<td class="text-right">
						<div class="dropdown dropdown-end dropdown-bottom">
							<button
								type="button"
								class="btn btn-ghost btn-xs"
								aria-label={`Open actions for file ${file.name}`}
								on:click|stopPropagation
							>
								<svg
									xmlns="http://www.w3.org/2000/svg"
									fill="none"
									viewBox="0 0 24 24"
									stroke-width="1.5"
									stroke="currentColor"
									class="w-4 h-4"
								>
									<path
										stroke-linecap="round"
										stroke-linejoin="round"
										d="M12 6.75a.75.75 0 110-1.5.75.75 0 010 1.5zM12 12.75a.75.75 0 110-1.5.75.75 0 010 1.5zM12 18.75a.75.75 0 110-1.5.75.75 0 010 1.5z"
									/>
								</svg>
							</button>
							<ul class="dropdown-content menu p-2 shadow bg-base-100 rounded-box w-52 z-50">
								<li>
									<button type="button" on:click|stopPropagation={() => onRenameFile(file)}
										>Rename</button
									>
								</li>
								<li>
									<button type="button" on:click|stopPropagation={() => onDownloadFile(file)}
										>Download</button
									>
								</li>
								<li>
									<button type="button" on:click|stopPropagation={() => onReplaceFile(file)}
										>Replace File</button
									>
								</li>
								<li>
									<button type="button" on:click|stopPropagation={() => onShareFile(file)}
										>Share</button
									>
								</li>
								<li>
									<button type="button" on:click|stopPropagation={() => onVersionHistory(file)}
										>Version History</button
									>
								</li>
								<li>
									<button type="button" on:click|stopPropagation={() => onMoveFile(file)}
										>Move</button
									>
								</li>
								<li>
									<button
										type="button"
										on:click|stopPropagation={() => onDeleteFile(file)}
										class="text-error">Delete</button
									>
								</li>
							</ul>
						</div>
					</td>
				</tr>
			{/each}

			{#if folders.length === 0 && files.length === 0}
				<tr>
					<td colspan="5" class="py-8 text-base-content/50 text-center"> No files or folders </td>
				</tr>
			{/if}
		</tbody>
	</table>
</div>
