<script lang="ts">
	import type { File as FileType, Folder } from '$lib/api/types';
	import type { ReplicationStatus } from '$lib/stores/replication';
	import { formatFileSize, formatDate, getFileTypeLabel } from '$lib/utils/format';
	import { detectEditorType, canEditFileSize } from '$lib/utils/editor';
	import FilePreview from './FilePreview.svelte';
	import ShareIndicator from '$lib/components/files/ShareIndicator.svelte';
	import MenuComponent from '$lib/components/common/ContextMenu.svelte';
	// ContextMenu exports MenuItem inside the module, so we type it locally if needed, or import type { MenuItem } from ContextMenu if exported
	type MenuItem = any;
	import { replicationStateBadgeClass, formatReplicationStateLabel } from '$lib/stores/replication';
	import {
		MoveVertical as MoreVertical,
		CreditCard as Edit,
		CreditCard as Edit3,
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
		Folder as FolderIcon,
		File as FileIcon,
		ChevronRight
	} from 'lucide-svelte';
	import { tick } from 'svelte';

	// Props
	interface Props {
		item: FileType | Folder;
		isFolder: boolean;
		isSharedRoot?: boolean;
		workspaceMode?: 'all' | 'photos' | 'recent' | 'starred' | 'deleted';
		selectionMode?: boolean;
		selected?: boolean;
		replicationStatus?: ReplicationStatus | null;
		isDragging?: boolean;
		isDropTarget?: boolean;
		canDrop?: boolean;
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
		onEdit?: () => void;
		onDragStart?: () => void;
		onDragEnd?: () => void;
		onDrop?: (e: DragEvent) => void;
		onDragOver?: () => void;
		onDragLeave?: () => void;
	}

	let {
		item,
		isFolder,
		isSharedRoot = false,
		workspaceMode = 'all',
		selectionMode = false,
		selected = false,
		replicationStatus = null,
		isDragging = false,
		isDropTarget = false,
		canDrop = true,
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
		onEdit = () => {},
		onDragStart = () => {},
		onDragEnd = () => {},
		onDrop = () => {},
		onDragOver = () => {},
		onDragLeave = () => {}
	}: Props = $props();

	// Derived values
	let fileItem = $derived(isFolder ? null : (item as FileType));
	let displaySize = $derived(
		isFolder
			? typeof (item as Folder).size === 'number'
				? formatFileSize((item as Folder).size as number)
				: '—'
			: formatFileSize(fileItem?.size ?? 0)
	);
	let displayDate = $derived(
		formatDate(
			workspaceMode === 'deleted'
				? (item.deleted_at ??
						(isFolder ? (item as Folder).updated_at : (item as FileType).modified_at))
				: isFolder
					? (item as Folder).updated_at
					: (item as FileType).modified_at
		)
	);
	let mimeType = $derived(fileItem?.mime_type || '');
	let fileName = $derived(item?.name || '');
	let fileTypeLabel = $derived(isFolder ? 'Folder' : getFileTypeLabel(mimeType, fileName));
	let isStarred = $derived(Boolean(item?.starred_at));
	let effectivePermission = $derived(item?.effective_permission || 'Admin');
	let canManage = $derived(effectivePermission === 'Edit' || effectivePermission === 'Admin');
	let canShare = $derived(effectivePermission === 'Admin');

	// Action menu state
	let showActions = $state(false);
	let actionMenuRef = $state<HTMLDivElement | undefined>(undefined);
	let actionButtonRef = $state<HTMLButtonElement | undefined>(undefined);
	let menuTop = $state(0);
	let menuLeft = $state(0);

	// Context menu state
	let contextMenuVisible = $state(false);
	let contextMenuX = $state(0);
	let contextMenuY = $state(0);

	// Inline rename state
	let isRenaming = $state(false);
	let renameValue = $state('');
	let renameInputRef = $state<HTMLInputElement | undefined>(undefined);

	let menuItems = $derived(buildMenuItems());

	function buildMenuItems(): MenuItem[] {
		const items: MenuItem[] = [];

		if (workspaceMode === 'deleted') {
			items.push(
				{ id: 'restore', label: 'Restore', icon: RotateCcw, onClick: onRestore },
				{ id: 'sep1', label: '', separator: true, onClick: () => {} },
				{
					id: 'delete',
					label: 'Delete permanently',
					icon: Trash2,
					danger: true,
					onClick: onPermanentDelete
				}
			);
		} else {
			if (isFolder) {
				items.push(
					{ id: 'open', label: 'Open', icon: FolderIcon, shortcut: 'Enter', onClick: onNavigate },
					{ id: 'sep1', label: '', separator: true, onClick: () => {} }
				);
			} else {
				// Check if file is editable
				const isEditable =
					!isFolder &&
					'deleted_at' in item &&
					!item.deleted_at &&
					detectEditorType(item.name, fileItem?.mime_type || '') !== 'none' &&
					canEditFileSize(fileItem?.size || 0);

				items.push({
					id: 'open',
					label: 'Open',
					icon: FileIcon,
					shortcut: 'Enter',
					onClick: () => onSelect()
				});

				if (isEditable) {
					items.push({
						id: 'edit',
						label: 'Edit',
						icon: Edit3,
						shortcut: '⌘E',
						onClick: () => {
							onEdit();
						}
					});
				}

				items.push(
					{
						id: 'download',
						label: 'Download',
						icon: Download,
						shortcut: '⌘D',
						onClick: onDownload
					},
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
					{
						id: 'star',
						label: isStarred ? 'Remove from starred' : 'Add to starred',
						icon: Star,
						onClick: onToggleStar
					},
					{ id: 'sep2', label: '', separator: true, onClick: () => {} },
					{
						id: 'delete',
						label: 'Move to trash',
						icon: Trash2,
						danger: true,
						shortcut: 'Del',
						onClick: onDelete
					}
				);
			}
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

	function maybeStartRename(e: MouseEvent) {
		e.stopPropagation();
		if (canManage) {
			startRename();
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
			e.dataTransfer.setData(
				'application/json',
				JSON.stringify({
					id: item.id,
					isFolder,
					name: item.name,
					parentFolderId: item.parent_folder_id
				})
			);
			onDragStart();
		}
	}

	function handleDragOver(e: DragEvent) {
		if (isFolder && !isDragging) {
			e.preventDefault();
			// Only show move cursor if this is a valid drop target
			e.dataTransfer!.dropEffect = canDrop ? 'move' : 'none';
			// Notify parent that we're dragging over this folder
			if (canDrop) {
				onDragOver();
			}
		}
	}

	function handleDrop(e: DragEvent) {
		e.preventDefault();
		if (isFolder && !isDragging && canDrop) {
			onDrop?.(e);
		}
	}

	function handleDragLeave(e: DragEvent) {
		if (isFolder && !isDragging) {
			// Only trigger leave if we're actually leaving the row (not entering a child)
			const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
			const x = e.clientX;
			const y = e.clientY;
			if (x < rect.left || x >= rect.right || y < rect.top || y >= rect.bottom) {
				onDragLeave();
			}
		}
	}
</script>

<svelte:window
	onclick={handleClickOutside}
	onresize={handleViewportChange}
	onscroll={handleViewportChange}
/>

<tr
	class="group cursor-pointer transition-colors hover:bg-base-200/60
		{selected ? 'bg-brand-500/5' : ''} 
		{isDragging ? 'opacity-40' : ''} 
		{isDropTarget
		? canDrop
			? 'bg-brand-500/10 ring-1 ring-brand-500/30 ring-inset'
			: 'bg-error/10 ring-1 ring-error/30 ring-inset'
		: ''}
		{!isRenaming ? 'cursor-grab active:cursor-grabbing' : ''}"
	onclick={handleClick}
	oncontextmenu={handleContextMenu}
	draggable={!isRenaming}
	ondragstart={handleDragStart}
	ondragend={onDragEnd}
	ondragover={handleDragOver}
	ondragleave={handleDragLeave}
	ondrop={handleDrop}
	aria-selected={selected}
	aria-grabbed={isDragging}
	aria-dropeffect={isFolder && canDrop ? 'move' : 'none'}
>
	<!-- Checkbox -->
	<td class="w-10 px-3 py-0.5">
		{#if selectionMode}
			<input
				type="checkbox"
				class="h-4 w-4 cursor-pointer rounded border-base-300 bg-base-100 text-brand-500 focus:ring-brand-500"
				checked={selected}
				onchange={handleToggle}
				onclick={(e) => e.stopPropagation()}
			/>
		{/if}
	</td>

	<!-- Preview Icon -->
	<td class="w-10 px-1 py-0.5">
		<div class="flex items-center justify-center">
			<FilePreview
				{item}
				{isFolder}
				{isSharedRoot}
				size="sm"
				showThumbnail={!isFolder && workspaceMode !== 'deleted'}
			/>
		</div>
	</td>

	<!-- Name -->
	<td class="max-w-0 min-w-0 px-2 py-0.5">
		<div class="flex min-w-0 items-center gap-2">
			{#if isRenaming}
				<div class="flex min-w-0 flex-1 items-center gap-1">
					<input
						bind:this={renameInputRef}
						type="text"
						class="min-w-0 flex-1 rounded-md border border-brand-500 bg-base-100 px-2 py-0.5 text-meta focus:ring-2 focus:ring-brand-500/20 focus:outline-hidden"
						value={renameValue}
						oninput={(e) => (renameValue = e.currentTarget.value)}
						onkeydown={handleRenameKeydown}
						onblur={confirmRename}
						onclick={(e) => e.stopPropagation()}
					/>
				</div>
			{:else if isFolder && workspaceMode === 'all'}
				<button
					type="button"
					class="group/link flex min-w-0 items-center gap-1 truncate text-left text-body-sm font-medium text-base-content transition-colors hover:text-brand-500"
					onclick={handleNavigate}
					ondblclick={maybeStartRename}
				>
					<span class="truncate">{item.name}</span>
					<ChevronRight
						size={12}
						class="text-base-content/40 opacity-0 transition-opacity group-hover/link:opacity-100"
					/>
				</button>
			{:else}
				<span
					class="block min-w-0 truncate text-body-sm font-medium text-base-content"
					ondblclick={maybeStartRename}
					role="button"
					tabindex="0"
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
				<Star size={12} class="flex-shrink-0 fill-brand-500 text-brand-500" />
			{/if}
		</div>
	</td>

	<!-- Type -->
	<td class="hidden w-28 px-3 py-0.5 md:table-cell">
		<span
			class="inline-flex items-center rounded-md bg-base-200/70 px-1.5 py-0.5 text-2xs font-medium tracking-tight text-base-content/60 uppercase"
		>
			{fileTypeLabel}
		</span>
	</td>

	<!-- Size -->
	<td class="hidden w-20 px-3 py-0.5 sm:table-cell">
		<span class="font-data text-meta text-base-content/50 tabular-nums">{displaySize}</span>
	</td>

	<!-- Modified -->
	<td class="hidden w-36 px-3 py-0.5 lg:table-cell">
		<span class="font-data text-meta text-base-content/50">{displayDate}</span>
	</td>

	<!-- Replication Status (hidden on smaller screens) -->
	<td class="hidden w-28 px-3 py-0.5 xl:table-cell">
		{#if !isFolder && replicationStatus}
			<span
				class="inline-flex items-center rounded px-1.5 py-0.5 text-2xs font-medium {replicationStateBadgeClass(
					replicationStatus.replicationState
				)}"
			>
				{formatReplicationStateLabel(replicationStatus.replicationState)}
			</span>
		{/if}
	</td>

	<!-- Actions -->
	<td class="w-12 px-3 py-0.5">
		<div class="relative">
			<button
				type="button"
				bind:this={actionButtonRef}
				class="rounded-lg p-1 text-base-content/40 opacity-0 transition-all group-hover:opacity-100 hover:bg-base-200 hover:text-base-content focus:opacity-100"
				onclick={toggleActions}
				aria-label="Actions"
			>
				<MoreVertical size={14} />
			</button>

			{#if showActions}
				<div
					bind:this={actionMenuRef}
					class="fixed z-[70] w-44 rounded-xl border border-base-300/70 bg-base-100 py-1 shadow-xl shadow-black/20"
					style={`top: ${menuTop}px; left: ${menuLeft}px;`}
					onclick={(e) => e.stopPropagation()}
					onkeydown={(e: KeyboardEvent) => {
						if (e.key === 'Escape') {
							e.stopPropagation();
							showActions = false;
						}
					}}
					tabindex="-1"
					role="menu"
				>
					{#if workspaceMode === 'deleted'}
						<button
							type="button"
							class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200/60"
							onclick={(e) => handleAction(e, onRestore)}
						>
							<RotateCcw size={14} />
							Restore
						</button>
						<div class="my-1 border-t border-base-200"></div>
						<button
							type="button"
							class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-error transition-colors hover:bg-error/10"
							onclick={(e) => handleAction(e, onPermanentDelete)}
						>
							<Trash2 size={14} />
							Delete permanently
						</button>
					{:else}
						{#if canManage}
							<button
								type="button"
								class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200/60"
								onclick={(e) => {
									startRename();
									handleAction(e, () => {});
								}}
							>
								<Edit size={14} />
								Rename
							</button>
							<button
								type="button"
								class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200/60"
								onclick={(e) => handleAction(e, onToggleStar)}
							>
								<Star size={14} class={isStarred ? 'fill-brand-500 text-brand-500' : ''} />
								{isStarred ? 'Remove star' : 'Add to starred'}
							</button>
						{/if}
						{#if canShare}
							<button
								type="button"
								class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200/60"
								onclick={(e) => handleAction(e, onShare)}
							>
								<Share2 size={14} />
								Share
							</button>
						{/if}
						{#if !isFolder}
							{#if canManage && fileItem && detectEditorType(item.name, fileItem.mime_type) !== 'none' && canEditFileSize(fileItem.size)}
								<button
									type="button"
									class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200/60"
									onclick={(e) => handleAction(e, onEdit)}
								>
									<Edit3 size={14} />
									Edit
								</button>
							{/if}
							<button
								type="button"
								class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200/60"
								onclick={(e) => handleAction(e, onDownload)}
							>
								<Download size={14} />
								Download
							</button>
							{#if canManage}
								<button
									type="button"
									class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200/60"
									onclick={(e) => handleAction(e, onVersionHistory)}
								>
									<History size={14} />
									Version history
								</button>
								<button
									type="button"
									class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200/60"
									onclick={(e) => handleAction(e, onReplace)}
								>
									<RefreshCw size={14} />
									Replace file
								</button>
							{/if}
						{/if}
						{#if canManage}
							<button
								type="button"
								class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-base-content/80 transition-colors hover:bg-base-200/60"
								onclick={(e) => handleAction(e, onMove)}
							>
								<Move size={14} />
								Move
							</button>
							<div class="my-1 border-t border-base-200"></div>
							<button
								type="button"
								class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-error transition-colors hover:bg-error/10"
								onclick={(e) => handleAction(e, onDelete)}
							>
								<Trash2 size={14} />
								Delete
							</button>
						{/if}
					{/if}
				</div>
			{/if}
		</div>
	</td>
</tr>

<!-- Context Menu -->
<MenuComponent
	items={menuItems}
	x={contextMenuX}
	y={contextMenuY}
	visible={contextMenuVisible}
	onClose={() => (contextMenuVisible = false)}
/>
