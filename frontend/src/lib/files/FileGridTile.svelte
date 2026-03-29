<script lang="ts">
	import type { File as FileType, Folder } from '$lib/api/types';
	import type { ReplicationStatus } from '$lib/stores/replication';
	import FilePreview from './FilePreview.svelte';
	import ShareIndicator from '$lib/components/files/ShareIndicator.svelte';
	import { replicationStateBadgeClass, formatReplicationStateLabel } from '$lib/stores/replication';
	import { formatFileSize, formatDate } from '$lib/utils/format';
	import { MoreVertical, Edit, Trash2, Share2, Move, Download, History, RefreshCw, RotateCcw, Star } from 'lucide-svelte';

	export let item: FileType | Folder;
	export let isFolder: boolean;
	export let workspaceMode: 'all' | 'photos' | 'recent' | 'starred' | 'deleted' = 'all';
	export let selected: boolean = false;
	export let selectionMode: boolean = false;
	export let replicationStatus: ReplicationStatus | null = null;

	// Event handlers (callback props)
	export let onSelect: (e?: MouseEvent) => void = () => {};
	export let onToggle: () => void = () => {};
	export let onRename: () => void = () => {};
	export let onDelete: () => void = () => {};
	export let onToggleStar: () => void = () => {};
	export let onRestore: () => void = () => {};
	export let onPermanentDelete: () => void = () => {};
	export let onShare: () => void = () => {};
	export let onMove: () => void = () => {};
	export let onDownload: () => void = () => {};
	export let onVersionHistory: () => void = () => {};
	export let onReplace: () => void = () => {};

	$: fileItem = isFolder ? null : (item as FileType);
	$: displaySize = isFolder ? null : formatFileSize(fileItem?.size || 0);
	$: displayDate = formatDate(
		workspaceMode === 'deleted'
			? (item.deleted_at ?? (isFolder ? (item as Folder).updated_at : (item as FileType).modified_at))
			: (isFolder ? (item as Folder).updated_at : (item as FileType).modified_at)
	);
	$: isStarred = Boolean(item?.starred_at);

	let showActions = false;
	let tileRef: HTMLDivElement;

	function handleClick(e: MouseEvent) {
		if (selectionMode) {
			onToggle();
			return;
		}
		if (workspaceMode === 'deleted') return;
		if (isFolder && workspaceMode !== 'all') return;

		onSelect(e);
	}

	function handleAction(action: () => void) {
		action();
		showActions = false;
	}

	// Close actions when clicking outside
	function handleClickOutside(e: MouseEvent) {
		if (tileRef && !tileRef.contains(e.target as Node)) {
			showActions = false;
		}
	}
</script>

<svelte:window on:click={handleClickOutside} />

<div
	bind:this={tileRef}
	role="button"
	tabindex="0"
	class="group relative flex min-h-[15.5rem] flex-col rounded-2xl border p-4 transition-all cursor-pointer
		{selected
			? 'border-brand-500/30 bg-brand-500/10 ring-1 ring-brand-500/30'
			: 'border-base-300/70 bg-base-200/65 hover:border-brand-500/20 hover:bg-base-200'}"
	on:click={handleClick}
	on:keydown={(e) => { if (e.key === 'Enter' || e.key === ' ') handleClick(e as any); }}
>
	<!-- Checkbox (selection mode) -->
	{#if selectionMode}
		<div class="absolute top-2 left-2 z-10">
			<input
				type="checkbox"
				class="w-4 h-4 rounded border-base-300 text-brand-500 focus:ring-brand-500 bg-base-100"
				checked={selected}
				on:click|stopPropagation
				on:change={onToggle}
			/>
		</div>
	{/if}

	<!-- Actions Menu -->
	<div class="absolute top-2 right-2 z-20 opacity-0 transition-opacity group-hover:opacity-100">
		<button
			type="button"
			class="p-1.5 bg-base-100/90 backdrop-blur-sm rounded-lg text-base-content/60 hover:text-base-content shadow-sm"
			on:click|stopPropagation={() => showActions = !showActions}
			aria-label="Actions"
		>
			<MoreVertical size={14} />
		</button>

		{#if showActions}
			<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
			<div
				class="absolute right-0 top-full z-50 mt-1 w-40 rounded-xl border border-base-300 bg-base-100 py-1 shadow-lg shadow-black/20"
				on:click|stopPropagation
			>
				{#if workspaceMode === 'deleted'}
					<button
						type="button"
						class="w-full flex items-center gap-2 px-3 py-2 text-sm text-base-content/80 hover:bg-base-200 transition-colors text-left"
						on:click|stopPropagation={() => handleAction(onRestore)}
					>
						<RotateCcw size={14} />
						Restore
					</button>
					<div class="border-t border-base-200 my-1"></div>
					<button
						type="button"
						class="w-full flex items-center gap-2 px-3 py-2 text-sm text-error hover:bg-error/10 transition-colors text-left"
						on:click|stopPropagation={() => handleAction(onPermanentDelete)}
					>
						<Trash2 size={14} />
						Delete permanently
					</button>
				{:else}
					<button
						type="button"
						class="w-full flex items-center gap-2 px-3 py-2 text-sm text-base-content/80 hover:bg-base-200 transition-colors text-left"
						on:click|stopPropagation={() => handleAction(onRename)}
					>
						<Edit size={14} />
						Rename
					</button>
					<button
						type="button"
						class="w-full flex items-center gap-2 px-3 py-2 text-sm text-base-content/80 hover:bg-base-200 transition-colors text-left"
						on:click|stopPropagation={() => handleAction(onToggleStar)}
					>
						<Star size={14} />
						{isStarred ? 'Remove star' : 'Add to starred'}
					</button>
					<button
						type="button"
						class="w-full flex items-center gap-2 px-3 py-2 text-sm text-base-content/80 hover:bg-base-200 transition-colors text-left"
						on:click|stopPropagation={() => handleAction(onShare)}
					>
						<Share2 size={14} />
						Share
					</button>
					{#if !isFolder}
						<button
							type="button"
							class="w-full flex items-center gap-2 px-3 py-2 text-sm text-base-content/80 hover:bg-base-200 transition-colors text-left"
							on:click|stopPropagation={() => handleAction(onDownload)}
						>
							<Download size={14} />
							Download
						</button>
						<button
							type="button"
							class="w-full flex items-center gap-2 px-3 py-2 text-sm text-base-content/80 hover:bg-base-200 transition-colors text-left"
							on:click|stopPropagation={() => handleAction(onVersionHistory)}
						>
							<History size={14} />
							Version history
						</button>
						<button
							type="button"
							class="w-full flex items-center gap-2 px-3 py-2 text-sm text-base-content/80 hover:bg-base-200 transition-colors text-left"
							on:click|stopPropagation={() => handleAction(onReplace)}
						>
							<RefreshCw size={14} />
							Replace file
						</button>
					{/if}
					<button
						type="button"
						class="w-full flex items-center gap-2 px-3 py-2 text-sm text-base-content/80 hover:bg-base-200 transition-colors text-left"
						on:click|stopPropagation={() => handleAction(onMove)}
					>
						<Move size={14} />
						Move
					</button>
					<div class="border-t border-base-200 my-1"></div>
					<button
						type="button"
						class="w-full flex items-center gap-2 px-3 py-2 text-sm text-error hover:bg-error/10 transition-colors text-left"
						on:click|stopPropagation={() => handleAction(onDelete)}
					>
						<Trash2 size={14} />
						Delete
					</button>
				{/if}
			</div>
		{/if}
	</div>

	<!-- Preview -->
	<div class="mb-4 flex aspect-[4/3] items-center justify-center rounded-xl border border-base-300/60 bg-base-100/80 p-3">
		<FilePreview {item} {isFolder} size="xl" showThumbnail={!isFolder && workspaceMode !== 'deleted'} />
	</div>

	<!-- Info -->
	<div class="min-w-0 space-y-2">
		<div class="flex items-start gap-1.5">
			<p class="flex-1 truncate text-sm font-medium leading-5 text-base-content" title={item.name}>
				{item.name}
			</p>
			{#if item.is_shared}
				<ShareIndicator
					isShared={item.is_shared}
					shareCount={item.share_count || 0}
					shareExpiresAt={item.share_expires_at || null}
					size="sm"
				/>
			{/if}
			{#if isStarred}
				<Star size={14} class="text-brand-500" />
			{/if}
		</div>

		<div class="flex items-center gap-2 text-xs text-base-content/55">
			{#if isFolder}
				<span>Folder</span>
			{:else}
				<span>{displaySize}</span>
				<span>•</span>
				<span>{displayDate}</span>
			{/if}
		</div>

		{#if !isFolder && replicationStatus}
			<div class="mt-2">
				<span class="inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium {replicationStateBadgeClass(replicationStatus.replicationState)}">
					{formatReplicationStateLabel(replicationStatus.replicationState)}
				</span>
			</div>
		{/if}
	</div>
</div>
