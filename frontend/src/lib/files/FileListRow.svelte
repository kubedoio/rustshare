<script lang="ts">
	import type { File as FileType, Folder } from '$lib/api/types';
	import type { ReplicationStatus } from '$lib/stores/replication';
	import { formatFileSize, formatDate, getFileTypeLabel } from '$lib/utils/format';
	import FilePreview from './FilePreview.svelte';
	import ShareIndicator from '$lib/components/files/ShareIndicator.svelte';
	import { replicationStateBadgeClass, formatReplicationStateLabel } from '$lib/stores/replication';
	import { MoreVertical, Edit, Trash2, Share2, Move, Download, History, RefreshCw, RotateCcw, Star } from 'lucide-svelte';
	import { tick } from 'svelte';

	export let item: FileType | Folder;
	export let isFolder: boolean;
	export let workspaceMode: 'all' | 'photos' | 'recent' | 'starred' | 'deleted' = 'all';
	export let selectionMode: boolean = false;
	export let selected: boolean = false;
	export let replicationStatus: ReplicationStatus | null = null;

	// Event handlers (callback props)
	export let onSelect: (e?: MouseEvent) => void = () => {};
	export let onToggleSelect: () => void = () => {};
	export let onNavigate: () => void = () => {};
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
	$: folderItem = isFolder ? (item as Folder) : null;
	$: displaySize = isFolder ? '—' : formatFileSize(fileItem?.size || 0);
	$: displayDate = formatDate(
		workspaceMode === 'deleted'
			? (item.deleted_at ?? (isFolder ? (item as Folder).updated_at : (item as FileType).modified_at))
			: (isFolder ? (item as Folder).updated_at : (item as FileType).modified_at)
	);
	$: mimeType = fileItem?.mime_type || '';
	$: fileName = item?.name || '';
	$: fileTypeLabel = isFolder ? 'Folder' : getFileTypeLabel(mimeType, fileName);
	$: isStarred = Boolean(item?.starred_at);

	let showActions = false;
	let actionMenuRef: HTMLDivElement;
	let actionButtonRef: HTMLButtonElement;
	let menuTop = 0;
	let menuLeft = 0;

	function handleClick(e: MouseEvent) {
		if (workspaceMode === 'deleted' && !selectionMode) return;
		if (!selectionMode) {
			if (isFolder && workspaceMode !== 'all') return;
			if (isFolder) {
				onNavigate();
			} else {
				onSelect(e);
			}
		} else {
			onSelect(e);
		}
	}

	function handleToggle(e: Event) {
		e.stopPropagation();
		onToggleSelect();
	}

	function handleNavigate(e: MouseEvent) {
		e.stopPropagation();
		onNavigate();
	}

	function handleAction(e: Event, action: () => void) {
		e.stopPropagation();
		action();
		showActions = false;
	}

	async function toggleActions(e: MouseEvent) {
		e.stopPropagation();
		showActions = !showActions;
		if (showActions) {
			await tick();
			positionActionMenu();
		}
	}

	function positionActionMenu() {
		if (!actionButtonRef || !actionMenuRef) return;

		const buttonRect = actionButtonRef.getBoundingClientRect();
		const menuRect = actionMenuRef.getBoundingClientRect();
		const viewportPadding = 12;
		const menuSpacing = 8;
		const shouldOpenUpward =
			buttonRect.bottom + menuSpacing + menuRect.height > window.innerHeight - viewportPadding &&
			buttonRect.top - menuSpacing - menuRect.height >= viewportPadding;

		menuTop = shouldOpenUpward
			? buttonRect.top - menuRect.height - menuSpacing
			: buttonRect.bottom + menuSpacing;
		menuLeft = Math.min(
			Math.max(viewportPadding, buttonRect.right - menuRect.width),
			window.innerWidth - menuRect.width - viewportPadding
		);
	}

	function handleViewportChange() {
		if (showActions) {
			positionActionMenu();
		}
	}

	// Close actions when clicking outside
	function handleClickOutside(e: MouseEvent) {
		const target = e.target as Node;
		if (
			actionMenuRef &&
			!actionMenuRef.contains(target) &&
			actionButtonRef &&
			!actionButtonRef.contains(target)
		) {
			showActions = false;
		}
	}
</script>

<svelte:window on:click={handleClickOutside} on:resize={handleViewportChange} on:scroll={handleViewportChange} />

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
		<FilePreview {item} {isFolder} size="sm" showThumbnail={!isFolder && workspaceMode !== 'deleted'} />
	</td>
	<td class="px-4 py-3">
		<div class="flex items-center gap-2">
			{#if isFolder && workspaceMode === 'all'}
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
			{#if isStarred}
				<Star size={14} class="text-brand-500" />
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
			{fileTypeLabel}
		</span>
	</td>
	<td class="px-4 py-3 text-sm text-base-content/60 hidden sm:table-cell">{displaySize}</td>
	<td class="px-4 py-3 text-sm text-base-content/60 hidden lg:table-cell">{displayDate}</td>
		<td class="px-4 py-3">
			<div class="relative">
				<button
					type="button"
					bind:this={actionButtonRef}
					class="rounded-lg p-2 text-base-content/40 opacity-0 transition-all hover:bg-base-200 hover:text-base-content group-hover:opacity-100"
					on:click={toggleActions}
					aria-label="Actions"
				>
					<MoreVertical size={16} />
				</button>

				{#if showActions}
					<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
					<div
						bind:this={actionMenuRef}
						class="fixed z-[70] w-44 rounded-xl border border-base-300 bg-base-100 py-1 shadow-lg shadow-black/20"
						style={`top: ${menuTop}px; left: ${menuLeft}px;`}
						on:click|stopPropagation
					>
						{#if workspaceMode === 'deleted'}
							<button
								type="button"
								class="w-full flex items-center gap-2 px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200"
								on:click={(e) => handleAction(e, onRestore)}
							>
								<RotateCcw size={14} />
								Restore
							</button>
							<div class="my-1 border-t border-base-200"></div>
							<button
								type="button"
								class="w-full flex items-center gap-2 px-4 py-2 text-left text-sm text-error transition-colors hover:bg-error/10"
								on:click={(e) => handleAction(e, onPermanentDelete)}
							>
								<Trash2 size={14} />
								Delete permanently
							</button>
						{:else}
							<button
								type="button"
								class="w-full flex items-center gap-2 px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200"
								on:click={(e) => handleAction(e, onRename)}
							>
								<Edit size={14} />
								Rename
							</button>
							<button
								type="button"
								class="w-full flex items-center gap-2 px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200"
								on:click={(e) => handleAction(e, onToggleStar)}
							>
								<Star size={14} />
								{isStarred ? 'Remove star' : 'Add to starred'}
							</button>
							<button
								type="button"
								class="w-full flex items-center gap-2 px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200"
								on:click={(e) => handleAction(e, onShare)}
							>
								<Share2 size={14} />
								Share
							</button>
							{#if !isFolder}
								<button
									type="button"
									class="w-full flex items-center gap-2 px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200"
									on:click={(e) => handleAction(e, onDownload)}
								>
									<Download size={14} />
									Download
								</button>
								<button
									type="button"
									class="w-full flex items-center gap-2 px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200"
									on:click={(e) => handleAction(e, onVersionHistory)}
								>
									<History size={14} />
									Version history
								</button>
								<button
									type="button"
									class="w-full flex items-center gap-2 px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200"
									on:click={(e) => handleAction(e, onReplace)}
								>
									<RefreshCw size={14} />
									Replace file
								</button>
							{/if}
							<button
								type="button"
								class="w-full flex items-center gap-2 px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200"
								on:click={(e) => handleAction(e, onMove)}
							>
								<Move size={14} />
								Move
							</button>
							<div class="my-1 border-t border-base-200"></div>
							<button
								type="button"
								class="w-full flex items-center gap-2 px-4 py-2 text-left text-sm text-error transition-colors hover:bg-error/10"
								on:click={(e) => handleAction(e, onDelete)}
							>
								<Trash2 size={14} />
								Delete
							</button>
						{/if}
					</div>
				{/if}
			</div>
		</td>
</tr>
