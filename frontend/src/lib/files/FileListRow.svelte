<script lang="ts">
	import type { File as FileType, Folder } from '$lib/api/types';
	import type { ReplicationStatus } from '$lib/stores/replication';
	import { formatFileSize, formatDate } from '$lib/utils/format';
	import FilePreview from './FilePreview.svelte';
	import ShareIndicator from '$lib/components/files/ShareIndicator.svelte';
	import { replicationStateBadgeClass, formatReplicationStateLabel } from '$lib/stores/replication';
	import { MoreVertical, Edit, Trash2, Share2, Move, Download, History, RefreshCw } from 'lucide-svelte';
	import { createEventDispatcher } from 'svelte';

	export let item: FileType | Folder;
	export let isFolder: boolean;
	export let selectionMode: boolean = false;
	export let selected: boolean = false;
	export let replicationStatus: ReplicationStatus | null = null;

	const dispatch = createEventDispatcher();

	$: fileItem = isFolder ? null : (item as FileType);
	$: folderItem = isFolder ? (item as Folder) : null;
	$: displaySize = isFolder ? '—' : formatFileSize(fileItem?.size || 0);
	$: displayDate = formatDate(
		isFolder ? (item as Folder).updated_at : (item as FileType).modified_at
	);
	$: mimeType = fileItem?.mime_type || '';
	$: fileName = item?.name || '';

	let showActions = false;
	let actionMenuRef: HTMLDivElement;

	function handleClick(e: MouseEvent) {
		dispatch('select', e);
	}

	function handleToggle(e: Event) {
		e.stopPropagation();
		dispatch('toggleSelect');
	}

	function handleNavigate(e: MouseEvent) {
		e.stopPropagation();
		dispatch('navigate');
	}

	function handleAction(e: Event, action: string) {
		e.stopPropagation();
		dispatch(action);
		showActions = false;
	}

	// Close actions when clicking outside
	function handleClickOutside(e: MouseEvent) {
		if (actionMenuRef && !actionMenuRef.contains(e.target as Node)) {
			showActions = false;
		}
	}
</script>

<svelte:window on:click={handleClickOutside} />

<tr 
	class="group hover:bg-base-200/50 transition-colors {selected ? 'bg-brand-500/5' : ''}"
	on:click={handleClick}
>
	<td class="px-4 py-3">
		{#if selectionMode}
			<input
				type="checkbox"
				class="w-4 h-4 rounded border-base-300 text-brand-500 focus:ring-brand-500 bg-base-100"
				checked={selected}
				on:change={handleToggle}
				on:click|stopPropagation
			/>
		{/if}
	</td>
	<td class="px-2 py-3">
		<FilePreview {item} {isFolder} size="sm" showThumbnail={!isFolder} />
	</td>
	<td class="px-4 py-3">
		<div class="flex items-center gap-2">
			{#if isFolder}
				<button
					type="button"
					class="font-medium text-base-content truncate hover:text-brand-400 transition-colors text-left"
					on:click={handleNavigate}
				>
					{item.name}
				</button>
			{:else}
				<span class="font-medium text-base-content truncate">
					{item.name}
				</span>
			{/if}
			{#if item.is_shared}
				<ShareIndicator
					isShared={item.is_shared}
					shareCount={item.share_count || 0}
					shareExpiresAt={item.share_expires_at || null}
					size="sm"
				/>
			{/if}
			{#if !isFolder && replicationStatus}
				<span class="inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium {replicationStateBadgeClass(replicationStatus.replicationState)}">
					{formatReplicationStateLabel(replicationStatus.replicationState)}
				</span>
			{/if}
		</div>
	</td>
	<td class="px-4 py-3 hidden md:table-cell">
		<span class="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-base-200 text-base-content/70">
			{#if isFolder}
				Folder
			{:else}
				{mimeType.split('/')[1]?.toUpperCase() || mimeType.split('/')[0]}
			{/if}
		</span>
	</td>
	<td class="px-4 py-3 text-sm text-base-content/60 hidden sm:table-cell">{displaySize}</td>
	<td class="px-4 py-3 text-sm text-base-content/60 hidden lg:table-cell">{displayDate}</td>
	<td class="px-4 py-3">
		<div class="relative" bind:this={actionMenuRef}>
			<button
				type="button"
				class="p-2 text-base-content/40 hover:text-base-content hover:bg-base-200 rounded-lg opacity-0 group-hover:opacity-100 transition-all"
				on:click|stopPropagation={(e) => { e.stopPropagation(); showActions = !showActions; }}
				aria-label="Actions"
			>
				<MoreVertical size={16} />
			</button>
			
			{#if showActions}
				<div class="absolute right-0 top-full mt-1 w-44 bg-base-100 rounded-xl shadow-lg shadow-black/20 border border-base-300 py-1 z-50">
					<button
						type="button"
						class="w-full flex items-center gap-2 px-4 py-2 text-sm text-base-content/80 hover:bg-base-200 transition-colors text-left"
						on:click={(e) => handleAction(e, 'rename')}
					>
						<Edit size={14} />
						Rename
					</button>
					<button
						type="button"
						class="w-full flex items-center gap-2 px-4 py-2 text-sm text-base-content/80 hover:bg-base-200 transition-colors text-left"
						on:click={(e) => handleAction(e, 'share')}
					>
						<Share2 size={14} />
						Share
					</button>
					{#if !isFolder}
						<button
							type="button"
							class="w-full flex items-center gap-2 px-4 py-2 text-sm text-base-content/80 hover:bg-base-200 transition-colors text-left"
							on:click={(e) => handleAction(e, 'download')}
						>
							<Download size={14} />
							Download
						</button>
						<button
							type="button"
							class="w-full flex items-center gap-2 px-4 py-2 text-sm text-base-content/80 hover:bg-base-200 transition-colors text-left"
							on:click={(e) => handleAction(e, 'versionHistory')}
						>
							<History size={14} />
							Version history
						</button>
					{/if}
					<button
						type="button"
						class="w-full flex items-center gap-2 px-4 py-2 text-sm text-base-content/80 hover:bg-base-200 transition-colors text-left"
						on:click={(e) => handleAction(e, 'move')}
					>
						<Move size={14} />
						Move
					</button>
					<div class="border-t border-base-200 my-1"></div>
					<button
						type="button"
						class="w-full flex items-center gap-2 px-4 py-2 text-sm text-error hover:bg-error/10 transition-colors text-left"
						on:click={(e) => handleAction(e, 'delete')}
					>
						<Trash2 size={14} />
						Delete
					</button>
				</div>
			{/if}
		</div>
	</td>
</tr>
