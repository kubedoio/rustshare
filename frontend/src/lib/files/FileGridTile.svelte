<script lang="ts">
	import type { File as FileType, Folder } from '$lib/api/types';
	import type { ReplicationStatus } from '$lib/stores/replication';
	import FilePreview from './FilePreview.svelte';
	import ShareIndicator from '$lib/components/files/ShareIndicator.svelte';
	import MenuComponent from '$lib/components/common/ContextMenu.svelte';
	type MenuItem = any;
	import { replicationStateBadgeClass, formatReplicationStateLabel } from '$lib/stores/replication';
	import { formatFileSize, formatDate } from '$lib/utils/format';
	import { detectEditorType, canEditFileSize } from '$lib/utils/editor';
	import { MoveVertical as MoreVertical, CreditCard as Edit, CreditCard as Edit3, Trash2, Share2, Move, Download, History, RefreshCw, RotateCcw, Star, Folder as FolderIcon, File as FileIcon, Check, X } from 'lucide-svelte';

	// Props
	interface Props {
		item: FileType | Folder;
		isFolder: boolean;
		workspaceMode?: 'all' | 'photos' | 'recent' | 'starred' | 'deleted';
		isSharedRoot?: boolean;
		selected?: boolean;
		selectionMode?: boolean;
		replicationStatus?: ReplicationStatus | null;
		isDragging?: boolean;
		isDropTarget?: boolean;
		onSelect?: (e?: MouseEvent) => void;
		onToggle?: (e?: MouseEvent) => void;
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
		onEdit?: () => void;
		onDragStart?: () => void;
		onDragEnd?: () => void;
		onDragOver?: () => void;
		onDragLeave?: () => void;
		onDrop?: (e: DragEvent) => void;
	}

	let {
		item,
		isFolder,
		workspaceMode = 'all',
		isSharedRoot = false,
		selected = false,
		selectionMode = false,
		replicationStatus = null,
		isDragging = false,
		isDropTarget = false,
		onSelect = () => {},
		onToggle = () => {},
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
		onEdit = () => {},
		onDragStart = () => {},
		onDragEnd = () => {},
		onDragOver = () => {},
		onDragLeave = () => {},
		onDrop = () => {}
	}: Props = $props();

	// Derived values
	let fileItem = $derived(isFolder ? null : (item as FileType));
	let displaySize = $derived(
		isFolder
			? (typeof (item as Folder).size === 'number' ? formatFileSize((item as Folder).size as number) : null)
			: formatFileSize(fileItem?.size ?? 0)
	);
	let displayDate = $derived(formatDate(
		workspaceMode === 'deleted'
			? (item.deleted_at ?? (isFolder ? (item as Folder).updated_at : (item as FileType).modified_at))
			: (isFolder ? (item as Folder).updated_at : (item as FileType).modified_at)
	));
	let isStarred = $derived(Boolean(item?.starred_at));
	let effectivePermission = $derived(item?.effective_permission || 'Admin');
	let canManage = $derived(effectivePermission === 'Edit' || effectivePermission === 'Admin');
	let canShare = $derived(effectivePermission === 'Admin');

	// Context menu state
	let showActions = $state(false);
	let contextMenuVisible = $state(false);
	let contextMenuX = $state(0);
	let contextMenuY = $state(0);
	let tileRef: HTMLDivElement;

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
					{ id: 'open', label: 'Open', icon: FolderIcon, shortcut: 'Enter', onClick: () => onSelect() },
					{ id: 'sep1', label: '', separator: true, onClick: () => {} }
				);
			} else {
				// Check if file is editable
				const isEditable = !isFolder && 
					fileItem && 'deleted_at' in fileItem && 
					!fileItem.deleted_at && 
					detectEditorType(fileItem.name, fileItem.mime_type) !== 'none' &&
					canEditFileSize(fileItem.size);

				items.push(
					{ id: 'open', label: 'Open', icon: FileIcon, shortcut: 'Enter', onClick: () => onSelect() }
				);

				if (isEditable) {
					items.push({ id: 'edit', label: 'Edit', icon: Edit3, onClick: () => { console.log('[FileGridTile] context edit clicked, onEdit=', onEdit); onEdit(); } });
				}

				items.push(
					{ id: 'download', label: 'Download', icon: Download, shortcut: '⌘D', onClick: onDownload },
					{ id: 'sep1', label: '', separator: true, onClick: () => {} }
				);
			}

			if (canManage) {
				items.push(
					{ id: 'rename', label: 'Rename', icon: Edit, shortcut: 'F2', onClick: startRename },
					{ id: 'move', label: 'Move to...', icon: Move, onClick: onMove }
				);
			}
			if (canShare) {
				items.push({ id: 'share', label: 'Share', icon: Share2, onClick: onShare });
			}

			if (!isFolder && canManage) {
				items.push(
					{ id: 'versions', label: 'Version history', icon: History, onClick: onVersionHistory },
					{ id: 'replace', label: 'Replace file', icon: RefreshCw, onClick: onReplace }
				);
			}

			if (canManage) {
				items.push(
					{ id: 'star', label: isStarred ? 'Remove from starred' : 'Add to starred', icon: Star, onClick: onToggleStar },
					{ id: 'sep2', label: '', separator: true, onClick: () => {} },
					{ id: 'delete', label: 'Move to trash', icon: Trash2, danger: true, shortcut: 'Del', onClick: onDelete }
				);
			}
		}

		return items;
	}

	function handleClick(e: MouseEvent) {
		if (isRenaming) return;
		if (selectionMode) {
			onToggle?.(e);
			return;
		}
		if (workspaceMode === 'deleted') return;
		if (isFolder && workspaceMode !== 'all') return;

		onSelect(e);
	}

	function handleContextMenu(e: MouseEvent) {
		e.preventDefault();
		contextMenuX = e.clientX;
		contextMenuY = e.clientY;
		contextMenuVisible = true;
		showActions = false;
	}

	function handleAction(action: () => void) {
		console.log('[FileGridTile] handleAction calling action');
		action();
		showActions = false;
	}

	// Inline rename functions
	function startRename() {
		if (!canManage) return;
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
				parentFolderId: item.parent_folder_id
			}));
			onDragStart();
		}
	}

	function handleDragOver(e: DragEvent) {
		if (isFolder && !isDragging) {
			e.preventDefault();
			e.dataTransfer!.dropEffect = 'move';
			onDragOver?.();
		}
	}

	function handleDragLeave(e: DragEvent) {
		if (isFolder && !isDragging) {
			const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
			const x = e.clientX;
			const y = e.clientY;
			if (x < rect.left || x >= rect.right || y < rect.top || y >= rect.bottom) {
				onDragLeave?.();
			}
		}
	}

	function handleDrop(e: DragEvent) {
		e.preventDefault();
		if (isFolder && !isDragging) {
			onDrop?.(e);
		}
	}

	// Close actions when clicking outside
	function handleClickOutside(e: MouseEvent) {
		if (tileRef && !tileRef.contains(e.target as Node)) {
			showActions = false;
		}
	}
</script>

<svelte:window onclick={handleClickOutside} />

<div
	bind:this={tileRef}
	role="button"
	tabindex="0"
	class="group relative flex flex-col rounded-xl border overflow-hidden transition-all cursor-pointer bg-base-100
		{selected
			? 'border-brand-500/40 bg-brand-500/5 ring-1 ring-brand-500/30'
			: 'border-base-300/60 hover:border-brand-500/30 hover:shadow-md hover:shadow-black/5'}
		{isDragging ? 'opacity-40' : ''}
		{isDropTarget ? 'border-brand-500 bg-brand-500/10 ring-2 ring-brand-500/30' : ''}"
	onclick={handleClick}
	onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') handleClick(e as any); }}
	oncontextmenu={handleContextMenu}
	draggable={!isRenaming}
	ondragstart={handleDragStart}
	ondragend={onDragEnd}
	ondragover={handleDragOver}
	ondragleave={handleDragLeave}
	ondrop={handleDrop}
>
	<!-- Checkbox (selection mode) -->
	{#if selectionMode}
		<div class="absolute top-2 left-2 z-10">
			<input
				type="checkbox"
				class="w-4 h-4 rounded border-base-300 text-brand-500 focus:ring-brand-500 bg-base-100 cursor-pointer"
				checked={selected}
				onclick={(e) => e.stopPropagation()}
				onchange={() => onToggle?.()}
			/>
		</div>
	{/if}

	<!-- Actions Menu Button -->
	{#if !isRenaming}
		<div class="absolute top-2 right-2 z-20 opacity-0 transition-opacity group-hover:opacity-100">
			<button
				type="button"
				class="p-1.5 bg-base-100/90 backdrop-blur-sm rounded-lg text-base-content/50 hover:text-base-content shadow-sm border border-base-300/50 transition-colors"
				onclick={(e) => { e.stopPropagation(); showActions = !showActions; }}
				aria-label="Actions"
			>
				<MoreVertical size={14} />
			</button>

			{#if showActions}
				<div
					class="absolute right-0 top-full z-50 mt-1 w-44 rounded-xl border border-base-300/70 bg-base-100 py-1 shadow-xl shadow-black/20"
					onclick={(e) => e.stopPropagation()}
				>
					{#if workspaceMode === 'deleted'}
						<button
							type="button"
							class="w-full flex items-center gap-2 px-3 py-2 text-sm text-base-content/80 hover:bg-base-200/60 transition-colors text-left"
							onclick={(e) => { e.stopPropagation(); handleAction(onRestore); }}
						>
							<RotateCcw size={14} />
							Restore
						</button>
						<div class="border-t border-base-200 my-1"></div>
						<button
							type="button"
							class="w-full flex items-center gap-2 px-3 py-2 text-sm text-error hover:bg-error/10 transition-colors text-left"
							onclick={(e) => { e.stopPropagation(); handleAction(onPermanentDelete); }}
						>
							<Trash2 size={14} />
							Delete permanently
						</button>
					{:else}
						{#if canManage}
							<button
								type="button"
								class="w-full flex items-center gap-2 px-3 py-2 text-sm text-base-content/80 hover:bg-base-200/60 transition-colors text-left"
								onclick={(e) => { e.stopPropagation(); startRename(); handleAction(() => {}); }}
							>
								<Edit size={14} />
								Rename
							</button>
							<button
								type="button"
								class="w-full flex items-center gap-2 px-3 py-2 text-sm text-base-content/80 hover:bg-base-200/60 transition-colors text-left"
								onclick={(e) => { e.stopPropagation(); handleAction(onToggleStar); }}
							>
								<Star size={14} class={isStarred ? 'fill-brand-500 text-brand-500' : ''} />
								{isStarred ? 'Remove star' : 'Add to starred'}
							</button>
						{/if}
						{#if canShare}
							<button
								type="button"
								class="w-full flex items-center gap-2 px-3 py-2 text-sm text-base-content/80 hover:bg-base-200/60 transition-colors text-left"
								onclick={(e) => { e.stopPropagation(); handleAction(onShare); }}
							>
								<Share2 size={14} />
								Share
							</button>
						{/if}
						{#if !isFolder}
							{#if canManage && fileItem && detectEditorType(fileItem.name, fileItem.mime_type) !== 'none' && canEditFileSize(fileItem.size)}
								<button
									type="button"
									class="w-full flex items-center gap-2 px-3 py-2 text-sm text-base-content/80 hover:bg-base-200/60 transition-colors text-left"
									onclick={(e) => { e.stopPropagation(); handleAction(onEdit); }}
								>
									<Edit3 size={14} />
									Edit
								</button>
							{/if}
							<button
								type="button"
								class="w-full flex items-center gap-2 px-3 py-2 text-sm text-base-content/80 hover:bg-base-200/60 transition-colors text-left"
								onclick={(e) => { e.stopPropagation(); handleAction(onDownload); }}
							>
								<Download size={14} />
								Download
							</button>
							{#if canManage}
								<button
									type="button"
									class="w-full flex items-center gap-2 px-3 py-2 text-sm text-base-content/80 hover:bg-base-200/60 transition-colors text-left"
									onclick={(e) => { e.stopPropagation(); handleAction(onVersionHistory); }}
								>
									<History size={14} />
									Version history
								</button>
							{/if}
						{/if}
						{#if canManage}
							<button
								type="button"
								class="w-full flex items-center gap-2 px-3 py-2 text-sm text-base-content/80 hover:bg-base-200/60 transition-colors text-left"
								onclick={(e) => { e.stopPropagation(); handleAction(onMove); }}
							>
								<Move size={14} />
								Move
							</button>
							<div class="border-t border-base-200 my-1"></div>
							<button
								type="button"
								class="w-full flex items-center gap-2 px-3 py-2 text-sm text-error hover:bg-error/10 transition-colors text-left"
								onclick={(e) => { e.stopPropagation(); handleAction(onDelete); }}
							>
								<Trash2 size={14} />
								Delete
							</button>
						{/if}
					{/if}
				</div>
			{/if}
		</div>
	{/if}

	<!-- Preview Area -->
	<div class="aspect-square bg-base-200/50 flex items-center justify-center p-3 border-b border-base-300/30">
		<div class="w-full h-full flex items-center justify-center">
			<FilePreview {item} {isFolder} {isSharedRoot} size="lg" showThumbnail={!isFolder && workspaceMode !== 'deleted'} />
		</div>
	</div>

	<!-- Info Area -->
	<div class="p-2 min-w-0">
		{#if isRenaming}
			<div class="flex items-center gap-1">
				<input
					bind:this={renameInputRef}
					type="text"
					class="flex-1 min-w-0 px-1.5 py-0.5 text-xs bg-base-100 border border-brand-500 rounded-md focus:outline-hidden focus:ring-2 focus:ring-brand-500/20"
					value={renameValue}
					oninput={(e) => renameValue = e.currentTarget.value}
					onkeydown={handleRenameKeydown}
					onblur={confirmRename}
				/>
				<button
					type="button"
					class="p-1 text-success hover:bg-success/10 rounded-md"
					onclick={(e) => { e.stopPropagation(); confirmRename(); }}
				>
					<Check size={14} />
				</button>
				<button
					type="button"
					class="p-1 text-error hover:bg-error/10 rounded-md"
					onclick={(e) => { e.stopPropagation(); cancelRename(); }}
				>
					<X size={14} />
				</button>
			</div>
		{:else}
			<div class="flex items-start gap-1.5 min-w-0">
				<p 
					class="flex-1 truncate text-xs font-medium leading-4 text-base-content" 
					title={item.name}
					ondblclick={(e) => { e.stopPropagation(); startRename(); }}
				>
					{item.name}
				</p>
			</div>
			
			<div class="flex items-center gap-1.5 mt-1">
				{#if item.is_shared}
					<ShareIndicator
						isShared={item.is_shared}
						shareCount={item.share_count || 0}
						shareExpiresAt={item.share_expires_at || null}
						size="xs"
					/>
				{/if}
				{#if isStarred}
					<Star size={10} class="text-brand-500 fill-brand-500" />
				{/if}
				
				<span class="text-2xs text-base-content/50 truncate font-medium font-data">
					{#if isFolder && displaySize === null}
						Folder
					{:else}
						{displaySize} • {displayDate}
					{/if}
				</span>
			</div>
		{/if}
	</div>
</div>

<!-- Context Menu -->
<MenuComponent
	items={menuItems}
	x={contextMenuX}
	y={contextMenuY}
	visible={contextMenuVisible}
	onClose={() => contextMenuVisible = false}
/>
