<script lang="ts">
	import type { File as FileType, Folder } from '$lib/api/types';
	import type { ReplicationStatus } from '$lib/stores/replication';
	import { formatFileSize, formatDate, getFileTypeLabel } from '$lib/utils/format';
	import FilePreview from './FilePreview.svelte';
	import ShareIndicator from '$lib/components/files/ShareIndicator.svelte';
	import ContextMenu from '$lib/components/common/ContextMenu.svelte';
	import type { MenuItem } from '$lib/components/common/ContextMenu.svelte';
	import { replicationStateBadgeClass, formatReplicationStateLabel } from '$lib/stores/replication';
	import { 
		MoreVertical, 
		Edit, 
		Trash2, 
		Share2, 
		Move, 
		Download, 
		History, 
		RefreshCw, 
		RotateCcw, 
		Star, 
		Check, 
		X, 
		FolderIcon, 
		FileIcon,
		ChevronRight
	} from 'lucide-svelte';
	import { tick } from 'svelte';

	// Props
	interface Props {
		item: FileType | Folder;
		isFolder: boolean;
		workspaceMode?: 'all' | 'photos' | 'recent' | 'starred' | 'deleted';
		selectionMode?: boolean;
		selected?: boolean;
		replicationStatus?: ReplicationStatus | null;
		isDragging?: boolean;
		isDropTarget?: boolean;
		onSelect?: (e?: MouseEvent) => void;
		onToggleSelect?: () => void;
		onNavigate?: () => void;
		onRename?: (newName: string) => void;
		onDelete?: () => void;
		onToggleStar?: () => void;
		onRestore?: () => void;
		onPermanentDelete?: () => void;
		onShare?: () => void;
		onMove?: () => void;
		onDownload?: () => void;
		onVersionHistory?: () => void;
		onReplace?: () => void;
		onDragStart?: () => void;
		onDragEnd?: () => void;
		onDrop?: () => void;
	}

	let {
		item,
		isFolder,
		workspaceMode = 'all',
		selectionMode = false,
		selected = false,
		replicationStatus = null,
		isDragging = false,
		isDropTarget = false,
		onSelect = () => {},
		onToggleSelect = () => {},
		onNavigate = () => {},
		onRename = () => {},
		onDelete = () => {},
		onToggleStar = () => {},
		onRestore = () => {},
		onPermanentDelete = () => {},
		onShare = () => {},
		onMove = () => {},
		onDownload = () => {},
		onVersionHistory = () => {},
		onReplace = () => {},
		onDragStart = () => {},
		onDragEnd = () => {},
		onDrop = () => {}
	}: Props = $props();

	// Derived values
	let fileItem = $derived(isFolder ? null : (item as FileType));
	let displaySize = $derived(isFolder ? '—' : formatFileSize(fileItem?.size || 0));
	let displayDate = $derived(formatDate(
		workspaceMode === 'deleted'
			? (item.deleted_at ?? (isFolder ? (item as Folder).updated_at : (item as FileType).modified_at))
			: (isFolder ? (item as Folder).updated_at : (item as FileType).modified_at)
	));
	let mimeType = $derived(fileItem?.mime_type || '');
	let fileName = $derived(item?.name || '');
	let fileTypeLabel = $derived(isFolder ? 'Folder' : getFileTypeLabel(mimeType, fileName));
	let isStarred = $derived(Boolean(item?.starred_at));

	// Action menu state
	let showActions = $state(false);
	let actionMenuRef: HTMLDivElement;
	let actionButtonRef: HTMLButtonElement;
	let menuTop = $state(0);
	let menuLeft = $state(0);

	// Context menu state
	let contextMenuVisible = $state(false);
	let contextMenuX = $state(0);
	let contextMenuY = $state(0);

	// Inline rename state
	let isRenaming = $state(false);
	let renameValue = $state('');
	let renameInputRef: HTMLInputElement;

	let menuItems = $derived(buildMenuItems());

	function buildMenuItems(): MenuItem[] {
		const items: MenuItem[] = [];

		if (workspaceMode === 'deleted') {
			items.push(
				{ id: 'restore', label: 'Restore', icon: RotateCcw, onClick: onRestore },
				{ id: 'sep1', label: '', separator: true, onClick: () => {} },
				{ id: 'delete', label: 'Delete permanently', icon: Trash2, danger: true, onClick: onPermanentDelete }
			);
		} else {
			if (isFolder) {
				items.push(
					{ id: 'open', label: 'Open', icon: FolderIcon, shortcut: 'Enter', onClick: onNavigate },
					{ id: 'sep1', label: '', separator: true, onClick: () => {} }
				);
			} else {
				items.push(
					{ id: 'open', label: 'Open', icon: FileIcon, shortcut: 'Enter', onClick: () => onSelect() },
					{ id: 'download', label: 'Download', icon: Download, shortcut: '⌘D', onClick: onDownload },
					{ id: 'sep1', label: '', separator: true, onClick: () => {} }
				);
			}

			items.push(
				{ id: 'rename', label: 'Rename', icon: Edit, shortcut: 'F2', onClick: startRename },
				{ id: 'move', label: 'Move to...', icon: Move, onClick: onMove },
				{ id: 'share', label: 'Share', icon: Share2, onClick: onShare }
			);

			if (!isFolder) {
				items.push(
					{ id: 'versions', label: 'Version history', icon: History, onClick: onVersionHistory },
					{ id: 'replace', label: 'Replace file', icon: RefreshCw, onClick: onReplace }
				);
			}

			items.push(
				{ id: 'star', label: isStarred ? 'Remove from starred' : 'Add to starred', icon: Star, onClick: onToggleStar },
				{ id: 'sep2', label: '', separator: true, onClick: () => {} },
				{ id: 'delete', label: 'Move to trash', icon: Trash2, danger: true, shortcut: 'Del', onClick: onDelete }
			);
		}

		return items;
	}

	function handleClick(e: MouseEvent) {
		if (isRenaming) return;
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

	function handleContextMenu(e: MouseEvent) {
		e.preventDefault();
		contextMenuX = e.clientX;
		contextMenuY = e.clientY;
		contextMenuVisible = true;
		showActions = false;
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

	// Inline rename functions
	function startRename() {
		isRenaming = true;
		renameValue = item.name;
		setTimeout(() => {
			renameInputRef?.focus();
			renameInputRef?.select();
		}, 0);
	}

	function confirmRename() {
		if (renameValue.trim() && renameValue !== item.name) {
			onRename(renameValue.trim());
		}
		isRenaming = false;
	}

	function cancelRename() {
		isRenaming = false;
		renameValue = item.name;
	}

	function handleRenameKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter') {
			confirmRename();
		} else if (e.key === 'Escape') {
			cancelRename();
		}
	}

	// Drag and drop handlers
	function handleDragStart(e: DragEvent) {
		if (e.dataTransfer) {
			e.dataTransfer.effectAllowed = 'move';
			e.dataTransfer.setData('application/json', JSON.stringify({ 
				id: item.id, 
				isFolder,
				name: item.name 
			}));
			onDragStart();
		}
	}

	function handleDragOver(e: DragEvent) {
		if (isFolder && !isDragging) {
			e.preventDefault();
			e.dataTransfer!.dropEffect = 'move';
		}
	}

	function handleDrop(e: DragEvent) {
		e.preventDefault();
		if (isFolder && !isDragging) {
			onDrop();
		}
	}
</script>

<svelte:window onclick={handleClickOutside} onresize={handleViewportChange} onscroll={handleViewportChange} />

<tr 
	class="group hover:bg-base-200/60 transition-colors border-b border-base-300/30 last:border-b-0
		{selected ? 'bg-brand-500/5' : ''} 
		{isDragging ? 'opacity-40' : ''} 
		{isDropTarget ? 'bg-brand-500/10 ring-1 ring-inset ring-brand-500/30' : ''}"
	onclick={handleClick}
	oncontextmenu={handleContextMenu}
	draggable={!isRenaming}
	ondragstart={handleDragStart}
	ondragend={onDragEnd}
	ondragover={handleDragOver}
	ondrop={handleDrop}
	role="row"
	aria-selected={selected}
>
	<!-- Checkbox -->
	<td class="w-10 px-3 py-2.5">
		{#if selectionMode}
			<input
				type="checkbox"
				class="w-4 h-4 rounded border-base-300 text-brand-500 focus:ring-brand-500 bg-base-100 cursor-pointer"
				checked={selected}
				onchange={handleToggle}
				onclick={(e) => e.stopPropagation()}
			/>
		{/if}
	</td>

	<!-- Preview Icon -->
	<td class="w-12 px-1 py-2.5">
		<div class="flex items-center justify-center">
			<FilePreview {item} {isFolder} size="md" showThumbnail={!isFolder && workspaceMode !== 'deleted'} />
		</div>
	</td>

	<!-- Name -->
	<td class="px-2 py-2.5 min-w-0 max-w-0">
		<div class="flex items-center gap-2 min-w-0">
			{#if isRenaming}
				<div class="flex items-center gap-1 flex-1 min-w-0">
					<input
						bind:this={renameInputRef}
						type="text"
						class="flex-1 min-w-0 px-2 py-1 text-sm bg-base-100 border border-brand-500 rounded-md focus:outline-none focus:ring-2 focus:ring-brand-500/20"
						value={renameValue}
						oninput={(e) => renameValue = e.currentTarget.value}
						onkeydown={handleRenameKeydown}
						onblur={confirmRename}
						onclick={(e) => e.stopPropagation()}
					/>
				</div>
			{:else if isFolder && workspaceMode === 'all'}
				<button
					type="button"
					class="font-medium text-base-content truncate hover:text-brand-500 transition-colors text-left min-w-0 flex items-center gap-1 group/link"
					onclick={handleNavigate}
					ondblclick={(e) => { e.stopPropagation(); startRename(); }}
				>
					<span class="truncate">{item.name}</span>
					<ChevronRight size={14} class="opacity-0 group-hover/link:opacity-100 transition-opacity text-base-content/40" />
				</button>
			{:else}
				<span 
					class="font-medium text-base-content truncate min-w-0 block"
					ondblclick={(e) => { e.stopPropagation(); startRename(); }}
				>
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
				<Star size={12} class="text-brand-500 fill-brand-500 flex-shrink-0" />
			{/if}
		</div>
	</td>

	<!-- Type -->
	<td class="px-3 py-2.5 hidden md:table-cell w-32">
		<span class="inline-flex items-center px-2 py-0.5 rounded-md text-xs font-medium bg-base-200/70 text-base-content/60">
			{fileTypeLabel}
		</span>
	</td>

	<!-- Size -->
	<td class="px-3 py-2.5 hidden sm:table-cell w-24">
		<span class="text-sm text-base-content/50 tabular-nums">{displaySize}</span>
	</td>

	<!-- Modified -->
	<td class="px-3 py-2.5 hidden lg:table-cell w-36">
		<span class="text-sm text-base-content/50">{displayDate}</span>
	</td>

	<!-- Replication Status (hidden on smaller screens) -->
	{#if !isFolder && replicationStatus}
		<td class="px-3 py-2.5 hidden xl:table-cell w-28">
			<span class="inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium {replicationStateBadgeClass(replicationStatus.replicationState)}">
				{formatReplicationStateLabel(replicationStatus.replicationState)}
			</span>
		</td>
	{/if}

	<!-- Actions -->
	<td class="w-12 px-3 py-2.5">
		<div class="relative">
			<button
				type="button"
				bind:this={actionButtonRef}
				class="rounded-lg p-1.5 text-base-content/40 transition-all hover:bg-base-200 hover:text-base-content opacity-0 group-hover:opacity-100 focus:opacity-100"
				onclick={toggleActions}
				aria-label="Actions"
			>
				<MoreVertical size={16} />
			</button>

			{#if showActions}
				<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
				<div
					bind:this={actionMenuRef}
					class="fixed z-[70] w-44 rounded-xl border border-base-300/70 bg-base-100 py-1 shadow-xl shadow-black/20"
					style={`top: ${menuTop}px; left: ${menuLeft}px;`}
					onclick={(e) => e.stopPropagation()}
				>
					{#if workspaceMode === 'deleted'}
						<button
							type="button"
							class="w-full flex items-center gap-2 px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200/60"
							onclick={(e) => handleAction(e, onRestore)}
						>
							<RotateCcw size={14} />
							Restore
						</button>
						<div class="my-1 border-t border-base-200"></div>
						<button
							type="button"
							class="w-full flex items-center gap-2 px-4 py-2 text-left text-sm text-error transition-colors hover:bg-error/10"
							onclick={(e) => handleAction(e, onPermanentDelete)}
						>
							<Trash2 size={14} />
							Delete permanently
						</button>
					{:else}
						<button
							type="button"
							class="w-full flex items-center gap-2 px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200/60"
							onclick={(e) => { startRename(); handleAction(e, () => {}); }}
						>
							<Edit size={14} />
							Rename
						</button>
						<button
							type="button"
							class="w-full flex items-center gap-2 px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200/60"
							onclick={(e) => handleAction(e, onToggleStar)}
						>
							<Star size={14} class={isStarred ? 'fill-brand-500 text-brand-500' : ''} />
							{isStarred ? 'Remove star' : 'Add to starred'}
						</button>
						<button
							type="button"
							class="w-full flex items-center gap-2 px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200/60"
							onclick={(e) => handleAction(e, onShare)}
						>
							<Share2 size={14} />
							Share
						</button>
						{#if !isFolder}
							<button
								type="button"
								class="w-full flex items-center gap-2 px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200/60"
								onclick={(e) => handleAction(e, onDownload)}
							>
								<Download size={14} />
								Download
							</button>
							<button
								type="button"
								class="w-full flex items-center gap-2 px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200/60"
								onclick={(e) => handleAction(e, onVersionHistory)}
							>
								<History size={14} />
								Version history
							</button>
							<button
								type="button"
								class="w-full flex items-center gap-2 px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200/60"
								onclick={(e) => handleAction(e, onReplace)}
							>
								<RefreshCw size={14} />
								Replace file
							</button>
						{/if}
						<button
							type="button"
							class="w-full flex items-center gap-2 px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200/60"
							onclick={(e) => handleAction(e, onMove)}
						>
							<Move size={14} />
							Move
						</button>
						<div class="my-1 border-t border-base-200"></div>
						<button
							type="button"
							class="w-full flex items-center gap-2 px-4 py-2 text-left text-sm text-error transition-colors hover:bg-error/10"
							onclick={(e) => handleAction(e, onDelete)}
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

<!-- Context Menu -->
<ContextMenu
	items={menuItems}
	x={contextMenuX}
	y={contextMenuY}
	visible={contextMenuVisible}
	onClose={() => contextMenuVisible = false}
/>
