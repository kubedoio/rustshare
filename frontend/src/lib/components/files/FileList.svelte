<script lang="ts">
	import type { File, Folder } from '$lib/api/types';
	import {
		formatReplicationStateLabel,
		replicationStateBadgeClass,
		type ReplicationStatus
	} from '$lib/stores/replication';
	import { selectionStore } from '$lib/stores/selection';
	import { detectEditorType } from '$lib/utils/editor';
	import ShareIndicator from './ShareIndicator.svelte';

	interface Props {
		folders?: Folder[];
		files?: File[];
		onFolderClick?: (folder: Folder) => void;
		onFileClick?: (file: File) => void;
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
		onFolderClick = () => {},
		onFileClick = () => {},
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

<div class="min-h-[200px] overflow-x-auto rounded-lg bg-base-100 shadow">
	<table class="table table-zebra">
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
						<div class="flex items-center gap-3">
							{#if selectionMode}
								<input
									type="checkbox"
									class="checkbox checkbox-sm"
									checked={$selectionStore.selectedFolderIds.has(folder.id)}
									onclick={(e) => { e.stopPropagation(); handleFolderToggle(folder); }}
								/>
							{/if}
							<button
								type="button"
								class="btn btn-ghost btn-sm"
								onclick={(e) => { e.stopPropagation(); onFolderClick(folder); }}
							>
								<span class="text-2xl">📁</span>
								<span class="font-medium">{folder.name}</span>
							</button>
							{#if folder.is_shared}
								<ShareIndicator
									isShared={folder.is_shared}
									shareCount={folder.share_count || 0}
									shareExpiresAt={folder.share_expires_at || null}
									size="sm"
								/>
							{/if}
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
								onclick={(e) => { e.stopPropagation(); }}
							>
								<svg
									xmlns="http://www.w3.org/2000/svg"
									fill="none"
									viewBox="0 0 24 24"
									stroke-width="1.5"
									stroke="currentColor"
									class="h-4 w-4"
								>
									<path
										stroke-linecap="round"
										stroke-linejoin="round"
										d="M12 6.75a.75.75 0 110-1.5.75.75 0 010 1.5zM12 12.75a.75.75 0 110-1.5.75.75 0 010 1.5zM12 18.75a.75.75 0 110-1.5.75.75 0 010 1.5z"
									/>
								</svg>
							</button>
							<ul class="dropdown-content menu z-50 w-52 rounded-box bg-base-100 p-2 shadow">
								<li>
									<button type="button" onclick={(e) => { e.stopPropagation(); onRenameFolder(folder); }}
										>Rename</button
									>
								</li>
								<li>
									<button type="button" onclick={(e) => { e.stopPropagation(); onShareFolder(folder); }}
										>Share</button
									>
								</li>
								<li>
									<button type="button" onclick={(e) => { e.stopPropagation(); onMoveFolder(folder); }}
										>Move</button
									>
								</li>
								<li>
									<button
										type="button"
										onclick={(e) => { e.stopPropagation(); onDeleteFolder(folder); }}
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
						<div class="flex items-center gap-3">
							{#if selectionMode}
								<input
									type="checkbox"
									class="checkbox checkbox-sm"
									checked={$selectionStore.selectedFileIds.has(file.id)}
									onclick={(e) => { e.stopPropagation(); handleFileToggle(file); }}
								/>
							{/if}
							<span class="text-2xl">{getFileIcon(file.mime_type)}</span>
							<button
								type="button"
								class="cursor-pointer text-left font-medium hover:text-primary"
								onclick={(e) => { e.stopPropagation(); onFileClick(file); }}
							>
								{file.name}
							</button>
							{#if file.is_shared}
								<ShareIndicator
									isShared={file.is_shared}
									shareCount={file.share_count || 0}
									shareExpiresAt={file.share_expires_at || null}
									size="sm"
								/>
							{/if}
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
								onclick={(e) => { e.stopPropagation(); }}
							>
								<svg
									xmlns="http://www.w3.org/2000/svg"
									fill="none"
									viewBox="0 0 24 24"
									stroke-width="1.5"
									stroke="currentColor"
									class="h-4 w-4"
								>
									<path
										stroke-linecap="round"
										stroke-linejoin="round"
										d="M12 6.75a.75.75 0 110-1.5.75.75 0 010 1.5zM12 12.75a.75.75 0 110-1.5.75.75 0 010 1.5zM12 18.75a.75.75 0 110-1.5.75.75 0 010 1.5z"
									/>
								</svg>
							</button>
							<ul class="dropdown-content menu z-50 w-52 rounded-box bg-base-100 p-2 shadow">
								<li>
									<button type="button" onclick={(e) => { e.stopPropagation(); onRenameFile(file); }}
										>Rename</button
									>
								</li>
								{#if detectEditorType(file.name, file.mime_type) !== 'none'}
									<li>
										<button type="button" onclick={(e) => { e.stopPropagation(); onEditFile(file); }}
											>Edit</button
										>
									</li>
								{/if}
								<li>
									<button type="button" onclick={(e) => { e.stopPropagation(); onDownloadFile(file); }}
										>Download</button
									>
								</li>
								<li>
									<button type="button" onclick={(e) => { e.stopPropagation(); onReplaceFile(file); }}
										>Replace File</button
									>
								</li>
								<li>
									<button type="button" onclick={(e) => { e.stopPropagation(); onShareFile(file); }}
										>Share</button
									>
								</li>
								<li>
									<button type="button" onclick={(e) => { e.stopPropagation(); onVersionHistory(file); }}
										>Version History</button
									>
								</li>
								<li>
									<button type="button" onclick={(e) => { e.stopPropagation(); onMoveFile(file); }}
										>Move</button
									>
								</li>
								<li>
									<button
										type="button"
										onclick={(e) => { e.stopPropagation(); onDeleteFile(file); }}
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
					<td colspan="5" class="py-8 text-center text-base-content/50"> No files or folders </td>
				</tr>
			{/if}
		</tbody>
	</table>
</div>
