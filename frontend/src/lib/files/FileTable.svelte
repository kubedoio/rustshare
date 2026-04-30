<script lang="ts">
	import type { File, Folder } from '$lib/api/types';
	import { formatFileSize, formatDate } from '$lib/utils/format';
	import { selectionStore } from '$lib/stores/selection';
	import {
		replicationStateBadgeClass,
		formatReplicationStateLabel,
		type ReplicationStatus
	} from '$lib/stores/replication';
	import ShareIndicator from '$lib/components/files/ShareIndicator.svelte';

	export let folders: Folder[] = [];
	export let files: File[] = [];
	export let selectionMode = false;
	export let replicationStatuses: Record<string, ReplicationStatus> = {};
	export let onFolderClick: (folder: Folder) => void = () => {};
	export let onFileClick: (file: File) => void = () => {};
	export let onRenameFolder: (folder: Folder) => void = () => {};
	export let onDeleteFolder: (folder: Folder) => void = () => {};
	export let onShareFolder: (folder: Folder) => void = () => {};
	export let onMoveFolder: (folder: Folder) => void = () => {};
	export let onRenameFile: (file: File) => void = () => {};
	export let onDeleteFile: (file: File) => void = () => {};
	export let onShareFile: (file: File) => void = () => {};
	export let onVersionHistory: (file: File) => void = () => {};
	export const onMoveFile: (file: File) => void = () => {};
	export let onDownloadFile: (file: File) => void = () => {};
	export const onReplaceFile: (file: File) => void = () => {};

	function handleFileToggle(file: File) {
		selectionStore.toggleFile(file.id);
	}

	function handleFolderToggle(folder: Folder) {
		selectionStore.toggleFolder(folder.id);
	}
</script>

<div class="overflow-hidden rounded-xl border border-base-300 bg-base-100">
	<table class="w-full">
		<thead>
			<tr class="border-b border-base-300 bg-base-200/50">
				<th class="w-10 px-4 py-3 text-left">
					{#if selectionMode}
						<input
							type="checkbox"
							class="h-4 w-4 rounded border-base-300 text-brand-500 focus:ring-brand-500"
							checked={$selectionStore.selectedFileIds.size +
								$selectionStore.selectedFolderIds.size ===
								files.length + folders.length && files.length + folders.length > 0}
							on:change={() => {
								if (
									$selectionStore.selectedFileIds.size + $selectionStore.selectedFolderIds.size ===
									files.length + folders.length
								) {
									selectionStore.deselectAll();
								} else {
									selectionStore.selectAll(files, folders);
								}
							}}
						/>
					{/if}
				</th>
				<th
					class="px-4 py-3 text-left text-xs font-semibold tracking-wider text-base-content/60 uppercase"
					>Name</th
				>
				<th
					class="hidden px-4 py-3 text-left text-xs font-semibold tracking-wider text-base-content/60 uppercase md:table-cell"
					>Type</th
				>
				<th
					class="hidden px-4 py-3 text-left text-xs font-semibold tracking-wider text-base-content/60 uppercase sm:table-cell"
					>Size</th
				>
				<th
					class="hidden px-4 py-3 text-left text-xs font-semibold tracking-wider text-base-content/60 uppercase lg:table-cell"
					>Modified</th
				>
				<th class="w-10 px-4 py-3"></th>
			</tr>
		</thead>
		<tbody class="divide-y divide-base-300">
			<!-- Folders -->
			{#each folders as folder (folder.id)}
				<tr class="group transition-colors hover:bg-base-200/50">
					<td class="px-4 py-3">
						{#if selectionMode}
							<input
								type="checkbox"
								class="h-4 w-4 rounded border-base-300 text-brand-500 focus:ring-brand-500"
								checked={$selectionStore.selectedFolderIds.has(folder.id)}
								on:change={() => handleFolderToggle(folder)}
							/>
						{/if}
					</td>
					<td class="px-4 py-3">
						<button
							type="button"
							class="group/link flex items-center gap-3 text-left"
							on:click={() => onFolderClick(folder)}
						>
							<div
								class="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-lg bg-brand-500/10"
							>
								<svg
									xmlns="http://www.w3.org/2000/svg"
									viewBox="0 0 24 24"
									fill="none"
									stroke="currentColor"
									stroke-width="2"
									stroke-linecap="round"
									stroke-linejoin="round"
									class="h-5 w-5 text-brand-400"
								>
									<path
										d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"
									/>
								</svg>
							</div>
							<div class="min-w-0">
								<p
									class="truncate font-medium text-base-content transition-colors group-hover/link:text-brand-400"
								>
									{folder.name}
								</p>
								{#if folder.is_shared}
									<div class="mt-0.5">
										<ShareIndicator
											isShared={folder.is_shared}
											shareCount={folder.share_count || 0}
											shareExpiresAt={folder.share_expires_at || null}
											size="sm"
										/>
									</div>
								{/if}
							</div>
						</button>
					</td>
					<td class="hidden px-4 py-3 md:table-cell">
						<span
							class="inline-flex items-center rounded-full bg-base-200 px-2.5 py-0.5 text-xs font-medium text-base-content/70"
						>
							Folder
						</span>
					</td>
					<td class="hidden px-4 py-3 text-sm text-base-content/60 sm:table-cell">—</td>
					<td class="hidden px-4 py-3 text-sm text-base-content/60 lg:table-cell"
						>{formatDate(folder.updated_at)}</td
					>
					<td class="px-4 py-3">
						<div class="relative">
							<button
								type="button"
								class="rounded-lg p-2 text-base-content/40 opacity-0 transition-all group-hover:opacity-100 hover:bg-base-200 hover:text-base-content"
								on:click|stopPropagation
								aria-label="Folder actions"
							>
								<svg
									xmlns="http://www.w3.org/2000/svg"
									viewBox="0 0 24 24"
									fill="none"
									stroke="currentColor"
									stroke-width="2"
									stroke-linecap="round"
									stroke-linejoin="round"
									class="h-4 w-4"
								>
									<circle cx="12" cy="12" r="1" />
									<circle cx="19" cy="12" r="1" />
									<circle cx="5" cy="12" r="1" />
								</svg>
							</button>
							<!-- Dropdown menu (simplified) -->
							<div
								class="absolute top-full right-0 z-10 mt-1 hidden w-48 rounded-lg border border-base-300 bg-base-100 py-1 shadow-lg shadow-black/20 group-hover:block"
							>
								<button
									type="button"
									class="w-full px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200"
									on:click={() => onRenameFolder(folder)}
								>
									Rename
								</button>
								<button
									type="button"
									class="w-full px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200"
									on:click={() => onShareFolder(folder)}
								>
									Share
								</button>
								<button
									type="button"
									class="w-full px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200"
									on:click={() => onMoveFolder(folder)}
								>
									Move
								</button>
								<div class="my-1 border-t border-base-200"></div>
								<button
									type="button"
									class="w-full px-4 py-2 text-left text-sm text-error transition-colors hover:bg-error/10"
									on:click={() => onDeleteFolder(folder)}
								>
									Delete
								</button>
							</div>
						</div>
					</td>
				</tr>
			{/each}

			<!-- Files -->
			{#each files as file (file.id)}
				<tr class="group transition-colors hover:bg-base-200/50">
					<td class="px-4 py-3">
						{#if selectionMode}
							<input
								type="checkbox"
								class="h-4 w-4 rounded border-base-300 text-brand-500 focus:ring-brand-500"
								checked={$selectionStore.selectedFileIds.has(file.id)}
								on:change={() => handleFileToggle(file)}
							/>
						{/if}
					</td>
					<td class="px-4 py-3">
						<button
							type="button"
							class="group/link flex items-center gap-3 text-left"
							on:click={() => onFileClick(file)}
						>
							<div
								class="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-lg bg-base-200"
							>
								{#if file.mime_type.startsWith('image/')}
									<svg
										xmlns="http://www.w3.org/2000/svg"
										viewBox="0 0 24 24"
										fill="none"
										stroke="currentColor"
										stroke-width="2"
										stroke-linecap="round"
										stroke-linejoin="round"
										class="h-5 w-5 text-info"
									>
										<rect width="18" height="18" x="3" y="3" rx="2" ry="2" />
										<circle cx="9" cy="9" r="2" />
										<path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21" />
									</svg>
								{:else if file.mime_type.startsWith('video/')}
									<svg
										xmlns="http://www.w3.org/2000/svg"
										viewBox="0 0 24 24"
										fill="none"
										stroke="currentColor"
										stroke-width="2"
										stroke-linecap="round"
										stroke-linejoin="round"
										class="h-5 w-5 text-red-400"
									>
										<path d="m22 8-6 4 6 4V8Z" />
										<rect width="14" height="12" x="2" y="6" rx="2" ry="2" />
									</svg>
								{:else if file.mime_type.includes('pdf')}
									<svg
										xmlns="http://www.w3.org/2000/svg"
										viewBox="0 0 24 24"
										fill="none"
										stroke="currentColor"
										stroke-width="2"
										stroke-linecap="round"
										stroke-linejoin="round"
										class="h-5 w-5 text-red-500"
									>
										<path
											d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z"
										/>
										<polyline points="14 2 14 8 20 8" />
										<path d="M10 13v-1a2 2 0 0 1 2-2h1" />
									</svg>
								{:else if file.mime_type.includes('word') || file.mime_type.includes('document')}
									<svg
										xmlns="http://www.w3.org/2000/svg"
										viewBox="0 0 24 24"
										fill="none"
										stroke="currentColor"
										stroke-width="2"
										stroke-linecap="round"
										stroke-linejoin="round"
										class="h-5 w-5 text-blue-400"
									>
										<path
											d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z"
										/>
										<polyline points="14 2 14 8 20 8" />
										<line x1="16" x2="8" y1="13" y2="13" />
										<line x1="16" x2="8" y1="17" y2="17" />
										<line x1="10" x2="8" y1="9" y2="9" />
									</svg>
								{:else if file.mime_type.includes('sheet') || file.mime_type.includes('excel')}
									<svg
										xmlns="http://www.w3.org/2000/svg"
										viewBox="0 0 24 24"
										fill="none"
										stroke="currentColor"
										stroke-width="2"
										stroke-linecap="round"
										stroke-linejoin="round"
										class="h-5 w-5 text-green-400"
									>
										<path
											d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z"
										/>
										<polyline points="14 2 14 8 20 8" />
										<path d="M8 13h2" />
										<path d="M8 17h2" />
										<path d="M14 13h2" />
										<path d="M14 17h2" />
									</svg>
								{:else}
									<svg
										xmlns="http://www.w3.org/2000/svg"
										viewBox="0 0 24 24"
										fill="none"
										stroke="currentColor"
										stroke-width="2"
										stroke-linecap="round"
										stroke-linejoin="round"
										class="h-5 w-5 text-base-content/50"
									>
										<path
											d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z"
										/>
										<polyline points="14 2 14 8 20 8" />
									</svg>
								{/if}
							</div>
							<div class="min-w-0">
								<p
									class="truncate font-medium text-base-content transition-colors group-hover/link:text-brand-400"
								>
									{file.name}
								</p>
								{#if file.is_shared || replicationStatuses[file.id]}
									<div class="mt-0.5 flex items-center gap-2">
										{#if file.is_shared}
											<ShareIndicator
												isShared={file.is_shared}
												shareCount={file.share_count || 0}
												shareExpiresAt={file.share_expires_at || null}
												size="sm"
											/>
										{/if}
										{#if replicationStatuses[file.id]}
											<span
												class="inline-flex items-center rounded px-1.5 py-0.5 text-xs font-medium {replicationStateBadgeClass(
													replicationStatuses[file.id].replicationState
												)}"
											>
												{formatReplicationStateLabel(replicationStatuses[file.id].replicationState)}
											</span>
										{/if}
									</div>
								{/if}
							</div>
						</button>
					</td>
					<td class="hidden px-4 py-3 md:table-cell">
						<span
							class="inline-flex items-center rounded-full bg-base-200 px-2.5 py-0.5 text-xs font-medium text-base-content/70"
						>
							{file.mime_type.split('/')[1]?.toUpperCase() || file.mime_type.split('/')[0]}
						</span>
					</td>
					<td class="hidden px-4 py-3 text-sm text-base-content/60 sm:table-cell"
						>{formatFileSize(file.size)}</td
					>
					<td class="hidden px-4 py-3 text-sm text-base-content/60 lg:table-cell"
						>{formatDate(file.modified_at)}</td
					>
					<td class="px-4 py-3">
						<div class="relative">
							<button
								type="button"
								class="rounded-lg p-2 text-base-content/40 opacity-0 transition-all group-hover:opacity-100 hover:bg-base-200 hover:text-base-content"
								on:click|stopPropagation
								aria-label="File actions"
							>
								<svg
									xmlns="http://www.w3.org/2000/svg"
									viewBox="0 0 24 24"
									fill="none"
									stroke="currentColor"
									stroke-width="2"
									stroke-linecap="round"
									stroke-linejoin="round"
									class="h-4 w-4"
								>
									<circle cx="12" cy="12" r="1" />
									<circle cx="19" cy="12" r="1" />
									<circle cx="5" cy="12" r="1" />
								</svg>
							</button>
							<!-- Dropdown menu (simplified) -->
							<div
								class="absolute top-full right-0 z-10 mt-1 hidden w-48 rounded-lg border border-base-300 bg-base-100 py-1 shadow-lg shadow-black/20 group-hover:block"
							>
								<button
									type="button"
									class="w-full px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200"
									on:click={() => onRenameFile(file)}
								>
									Rename
								</button>
								<button
									type="button"
									class="w-full px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200"
									on:click={() => onDownloadFile(file)}
								>
									Download
								</button>
								<button
									type="button"
									class="w-full px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200"
									on:click={() => onShareFile(file)}
								>
									Share
								</button>
								<button
									type="button"
									class="w-full px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200"
									on:click={() => onVersionHistory(file)}
								>
									Version history
								</button>
								<div class="my-1 border-t border-base-200"></div>
								<button
									type="button"
									class="w-full px-4 py-2 text-left text-sm text-error transition-colors hover:bg-error/10"
									on:click={() => onDeleteFile(file)}
								>
									Delete
								</button>
							</div>
						</div>
					</td>
				</tr>
			{/each}
		</tbody>
	</table>

	{#if folders.length === 0 && files.length === 0}
		<div class="flex flex-col items-center justify-center py-16 text-center">
			<div class="mb-4 flex h-16 w-16 items-center justify-center rounded-2xl bg-base-200">
				<svg
					xmlns="http://www.w3.org/2000/svg"
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					stroke-width="1.5"
					stroke-linecap="round"
					stroke-linejoin="round"
					class="h-8 w-8 text-base-content/30"
				>
					<path
						d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"
					/>
				</svg>
			</div>
			<h3 class="mb-1 text-lg font-semibold text-base-content">No files yet</h3>
			<p class="mb-4 text-sm text-base-content/60">Upload your first file to get started</p>
			<button
				type="button"
				class="rounded-lg bg-brand-500 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-600"
				on:click={() => document.getElementById('upload-file-input')?.click()}
			>
				Upload files
			</button>
		</div>
	{/if}
</div>
