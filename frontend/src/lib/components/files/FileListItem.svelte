<script lang="ts">
	import type { File, Folder } from '$lib/api/types';
	import {
		formatReplicationStateLabel,
		replicationStateBadgeClass,
		type ReplicationStatus
	} from '$lib/stores/replication';
	import { formatFileSize, formatDate } from '$lib/utils/format';
	import FileIcon from '$lib/components/icons/FileIcon.svelte';
	import { detectEditorType } from '$lib/utils/editor';
	import FileThumbnail from './FileThumbnail.svelte';
	import ShareIndicator from './ShareIndicator.svelte';

	interface Props {
		item: File | Folder;
		isFolder: boolean;
		onSelect: (event?: MouseEvent) => void;
		selectionMode?: boolean;
		selected?: boolean;
		replicationStatus?: ReplicationStatus | null;
		onEditFile?: (file: File) => void;
		onRename?: (payload: { item: File | Folder; isFolder: boolean }) => void;
		onDelete?: (payload: { item: File | Folder; isFolder: boolean }) => void;
		onShare?: (payload: { item: File | Folder; isFolder: boolean }) => void;
		onVersionHistory?: (payload: { item: File }) => void;
		onMove?: (payload: { item: File | Folder; isFolder: boolean }) => void;
		onDownload?: (payload: { item: File }) => void;
		onReplace?: (payload: { item: File }) => void;
	}

	let {
		item,
		isFolder,
		onSelect,
		selectionMode = false,
		selected = false,
		replicationStatus = null,
		onEditFile = () => {},
		onRename = () => {},
		onDelete = () => {},
		onShare = () => {},
		onVersionHistory = () => {},
		onMove = () => {},
		onDownload = () => {},
		onReplace = () => {}
	}: Props = $props();

	let fileItem = $derived(isFolder ? undefined : (item as File));
	let displaySize = $derived(
		isFolder
			? typeof (item as Folder).size === 'number'
				? formatFileSize((item as Folder).size as number)
				: '-'
			: formatFileSize(fileItem?.size ?? 0)
	);
	let displayDate = $derived(
		formatDate(isFolder ? (item as Folder).updated_at : (item as File).modified_at)
	);

	function getFileItem(): File {
		return item as File;
	}

	function handleRename(e: Event) {
		e.stopPropagation();
		e.preventDefault();
		onRename({ item, isFolder });
	}

	function handleShare(e: Event) {
		e.stopPropagation();
		e.preventDefault();
		onShare({ item, isFolder });
	}

	function handleVersionHistory(e: Event) {
		e.stopPropagation();
		e.preventDefault();
		onVersionHistory({ item: item as File });
	}

	function handleDelete(e: Event) {
		e.stopPropagation();
		e.preventDefault();
		onDelete({ item, isFolder });
	}

	function handleMove(e: Event) {
		e.stopPropagation();
		e.preventDefault();
		onMove({ item, isFolder });
	}

	function handleDownload(e: Event) {
		e.stopPropagation();
		e.preventDefault();
		onDownload({ item: item as File });
	}

	function handleReplace(e: Event) {
		e.stopPropagation();
		e.preventDefault();
		onReplace({ item: item as File });
	}

	function handleEdit(e: Event) {
		e.stopPropagation();
		e.preventDefault();
		onEditFile(item as File);
	}

	function handleCardActivate(event: MouseEvent) {
		onSelect(event);
	}

	function handleCardKeydown(event: KeyboardEvent) {
		if (event.key === 'Enter' || event.key === ' ') {
			event.preventDefault();
			onSelect();
		}
	}
</script>

<div
	class="group card relative touch-manipulation bg-base-100 shadow-sm transition-shadow hover:shadow-md"
	class:ring-2={selectionMode && selected}
	class:ring-primary={selectionMode && selected}
	onclick={handleCardActivate}
	onkeydown={handleCardKeydown}
	role="button"
	tabindex="0"
>
	<div class="card-body p-3 lg:p-4">
		<div class="flex items-center gap-2 lg:gap-3">
			{#if selectionMode}
				<input
					type="checkbox"
					class="checkbox checkbox-sm"
					checked={selected}
					onclick={(e) => {
						e.stopPropagation();
						onSelect(e);
					}}
				/>
			{/if}

			<!-- Thumbnail or icon -->
			{#if isFolder}
				<div
					class="flex cursor-pointer items-center justify-center text-brand-500"
					onclick={(e) => {
						e.stopPropagation();
						onSelect();
					}}
					onkeydown={(e) => {
						if (e.key === 'Enter') {
							e.stopPropagation();
							onSelect();
						}
					}}
					role="button"
					tabindex="0"
				>
					<svg
						class="h-10 w-10 lg:h-12 lg:w-12"
						xmlns="http://www.w3.org/2000/svg"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						stroke-width="1.5"
						stroke-linecap="round"
						stroke-linejoin="round"
					>
						<path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
					</svg>
				</div>
			{:else}
				<FileThumbnail file={getFileItem()} size={'md'} />
			{/if}

			<div class="min-w-0 flex-1">
				<button
					type="button"
					class="w-full text-left"
					onclick={(e) => {
						e.stopPropagation();
						onSelect();
					}}
				>
					<h3
						class="flex cursor-pointer items-center gap-2 truncate text-sm font-semibold hover:text-primary lg:text-base"
					>
						{item.name}
						{#if item.is_shared}
							<ShareIndicator
								isShared={item.is_shared}
								shareCount={item.share_count || 0}
								shareExpiresAt={item.share_expires_at || null}
								size="sm"
							/>
						{/if}
					</h3>
				</button>
				<div class="mt-1 flex items-center justify-between text-xs text-base-content/60 lg:text-sm">
					<span>{displaySize}</span>
					<span class="hidden sm:inline">{displayDate}</span>
				</div>
				{#if !isFolder && replicationStatus}
					<div class="mt-1">
						<span
							class={`badge badge-xs ${replicationStateBadgeClass(replicationStatus.replicationState)}`}
						>
							{formatReplicationStateLabel(replicationStatus.replicationState)}
						</span>
					</div>
				{/if}
			</div>

			<!-- Actions Menu -->
			<div class="dropdown dropdown-end dropdown-top">
				<button
					type="button"
					class="btn btn-circle min-h-[44px] min-w-[44px] opacity-100 btn-ghost transition-opacity btn-sm lg:min-h-0 lg:min-w-0 lg:opacity-0 lg:group-hover:opacity-100"
					aria-label={`Open actions for ${item.name}`}
					onclick={(e) => e.stopPropagation()}
				>
					<svg
						xmlns="http://www.w3.org/2000/svg"
						fill="none"
						viewBox="0 0 24 24"
						stroke-width="1.5"
						stroke="currentColor"
						class="h-5 w-5"
					>
						<path
							stroke-linecap="round"
							stroke-linejoin="round"
							d="M12 6.75a.75.75 0 110-1.5.75.75 0 010 1.5zM12 12.75a.75.75 0 110-1.5.75.75 0 010 1.5zM12 18.75a.75.75 0 110-1.5.75.75 0 010 1.5z"
						/>
					</svg>
				</button>
				<ul class="dropdown-content menu z-[1] w-52 rounded-box bg-base-100 p-2 shadow">
					<li>
						<button
							type="button"
							onclick={(e) => {
								e.stopPropagation();
								handleRename(e);
							}}
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
									d="M16.862 4.487l1.687-1.688a1.875 1.875 0 112.652 2.652L10.582 16.07a4.5 4.5 0 01-1.897 1.13L6 18l.8-2.685a4.5 4.5 0 011.13-1.897l8.932-8.931zm0 0L19.5 7.125M18 14v4.75A2.25 2.25 0 0115.75 21H5.25A2.25 2.25 0 013 18.75V8.25A2.25 2.25 0 015.25 6H10"
								/>
							</svg>
							Rename
						</button>
					</li>
					<li>
						<button
							type="button"
							onclick={(e) => {
								e.stopPropagation();
								handleShare(e);
							}}
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
									d="M13.19 8.688a4.5 4.5 0 011.242 7.244l-4.5 4.5a4.5 4.5 0 01-6.364-6.364l1.757-1.757m13.35-.622l1.757-1.757a4.5 4.5 0 00-6.364-6.364l-4.5 4.5a4.5 4.5 0 001.242 7.244"
								/>
							</svg>
							Share
						</button>
					</li>
					{#if !isFolder}
						<li>
							<button
								type="button"
								onclick={(e) => {
									e.stopPropagation();
									handleDownload(e);
								}}
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
										d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5M16.5 12L12 16.5m0 0L7.5 12m4.5 4.5V3"
									/>
								</svg>
								Download
							</button>
						</li>
						<li>
							<button
								type="button"
								onclick={(e) => {
									e.stopPropagation();
									handleReplace(e);
								}}
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
										d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0l3.181 3.183a8.25 8.25 0 0013.803-3.7M4.031 9.865a8.25 8.25 0 0113.803-3.7l3.181 3.182m0-4.991v4.99"
									/>
								</svg>
								Replace File
							</button>
						</li>
						{#if detectEditorType(getFileItem().name, getFileItem().mime_type) !== 'none'}
							<li>
								<button
									type="button"
									onclick={(e) => {
										e.stopPropagation();
										handleEdit(e);
									}}
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
											d="M16.862 4.487l1.687-1.688a1.875 1.875 0 112.652 2.652L10.582 16.07a4.5 4.5 0 01-1.897 1.13L6 18l.8-2.685a4.5 4.5 0 011.13-1.897l8.932-8.931zm0 0L19.5 7.125M18 14v4.75A2.25 2.25 0 0115.75 21H5.25A2.25 2.25 0 013 18.75V8.25A2.25 2.25 0 015.25 6H10"
										/>
									</svg>
									Edit
								</button>
							</li>
						{/if}
						<li>
							<button
								type="button"
								onclick={(e) => {
									e.stopPropagation();
									handleVersionHistory(e);
								}}
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
										d="M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z"
									/>
								</svg>
								Version History
							</button>
						</li>
					{/if}
					<li>
						<button
							type="button"
							onclick={(e) => {
								e.stopPropagation();
								handleMove(e);
							}}
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
									d="M3.75 9.776c.112-.017.227-.026.344-.026h15.812c.117 0 .232.009.344.026m-16.5 0a2.25 2.25 0 00-1.883 2.542l.857 6a2.25 2.25 0 002.227 1.932H19.05a2.25 2.25 0 002.227-1.932l.857-6a2.25 2.25 0 00-1.883-2.542m-16.5 0V6A2.25 2.25 0 016 3.75h3.879a1.5 1.5 0 011.06.44l2.122 2.12a1.5 1.5 0 001.06.44H18A2.25 2.25 0 0120.25 9v.776"
								/>
							</svg>
							Move
						</button>
					</li>
					<li>
						<button
							type="button"
							onclick={(e) => {
								e.stopPropagation();
								handleDelete(e);
							}}
							class="text-error"
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
									d="M14.74 9l-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 01-2.244 2.077H8.084a2.25 2.25 0 01-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 00-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 013.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 00-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 00-7.5 0"
								/>
							</svg>
							Delete
						</button>
					</li>
				</ul>
			</div>
		</div>
	</div>
</div>
