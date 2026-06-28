<script lang="ts">
	import type { File as FileType, Folder } from '$lib/api/types';
	import { detectEditorType, canEditFileSize } from '$lib/utils/editor';
	import MenuComponent from '$lib/components/common/ContextMenu.svelte';
	import type { ComponentType } from 'svelte';
	import {
		Folder as FolderIcon,
		File as FileIcon,
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
		Palette,
		X
	} from 'lucide-svelte';

	interface Props {
		item: FileType | Folder;
		workspaceMode?: 'all' | 'photos' | 'recent' | 'starred' | 'deleted' | 'week';
		isOpen?: boolean;
		position?: { x: number; y: number };
		onClose?: () => void;
		onAction?: (action: string) => void;
		onSetColor?: (color: string | null) => void;
	}

	let {
		item,
		workspaceMode = 'all',
		isOpen = false,
		position = { x: 0, y: 0 },
		onClose = () => {},
		onAction = () => {},
		onSetColor = () => {}
	}: Props = $props();

	const COLOR_PALETTE = [
		{ key: 'red', class: 'bg-red-500', label: 'Red' },
		{ key: 'orange', class: 'bg-orange-500', label: 'Orange' },
		{ key: 'yellow', class: 'bg-yellow-500', label: 'Yellow' },
		{ key: 'green', class: 'bg-green-500', label: 'Green' },
		{ key: 'blue', class: 'bg-blue-500', label: 'Blue' },
		{ key: 'purple', class: 'bg-purple-500', label: 'Purple' },
		{ key: 'gray', class: 'bg-gray-500', label: 'Gray' }
	];

	let showColorPicker = $state(false);
	let pickerRef = $state<HTMLDivElement | undefined>(undefined);

	let isFolder = $derived('parent_folder_id' in item && !('mime_type' in item));
	let fileItem = $derived(isFolder ? null : (item as FileType));
	let isStarred = $derived(Boolean(item?.starred_at));
	let effectivePermission = $derived(item?.effective_permission || 'Admin');
	let canManage = $derived(effectivePermission === 'Edit' || effectivePermission === 'Admin');
	let canShare = $derived(effectivePermission === 'Admin');

	interface MenuItem {
		id: string;
		label: string;
		icon?: ComponentType;
		shortcut?: string;
		disabled?: boolean;
		danger?: boolean;
		separator?: boolean;
		onClick: () => void;
	}

	let menuItems = $derived(buildMenuItems());

	function buildMenuItems(): MenuItem[] {
		const items: MenuItem[] = [];

		if (workspaceMode === 'deleted') {
			items.push(
				{ id: 'restore', label: 'Restore', icon: RotateCcw, onClick: () => onAction('restore') },
				{ id: 'sep1', label: '', separator: true, onClick: () => {} },
				{
					id: 'permanentDelete',
					label: 'Delete permanently',
					icon: Trash2,
					danger: true,
					onClick: () => onAction('permanentDelete')
				}
			);
		} else {
			if (isFolder) {
				items.push(
					{
						id: 'open',
						label: 'Open',
						icon: FolderIcon,
						shortcut: 'Enter',
						onClick: () => onAction('open')
					},
					{
						id: 'download',
						label: 'Download',
						icon: Download,
						shortcut: '⌘D',
						onClick: () => onAction('download')
					},
					{ id: 'sep1', label: '', separator: true, onClick: () => {} }
				);
			} else {
				const isEditable =
					fileItem &&
					'deleted_at' in fileItem &&
					!fileItem.deleted_at &&
					detectEditorType(fileItem.name, fileItem.mime_type) !== 'none' &&
					canEditFileSize(fileItem.size);

				items.push({
					id: 'open',
					label: 'Open',
					icon: FileIcon,
					shortcut: 'Enter',
					onClick: () => onAction('open')
				});

				if (isEditable) {
					items.push({
						id: 'edit',
						label: 'Edit',
						icon: Edit3,
						onClick: () => onAction('edit')
					});
				}

				items.push(
					{
						id: 'download',
						label: 'Download',
						icon: Download,
						shortcut: '⌘D',
						onClick: () => onAction('download')
					},
					{ id: 'sep1', label: '', separator: true, onClick: () => {} }
				);
			}

			if (canManage) {
				items.push(
					{
						id: 'rename',
						label: 'Rename',
						icon: Edit,
						shortcut: 'F2',
						onClick: () => onAction('rename')
					},
					{
						id: 'move',
						label: 'Move to...',
						icon: Move,
						onClick: () => onAction('move')
					}
				);
			}
			if (canShare) {
				items.push({
					id: 'share',
					label: 'Share',
					icon: Share2,
					onClick: () => onAction('share')
				});
			}

			if (!isFolder && canManage) {
				items.push(
					{
						id: 'versions',
						label: 'Version history',
						icon: History,
						onClick: () => onAction('versions')
					},
					{
						id: 'replace',
						label: 'Replace file',
						icon: RefreshCw,
						onClick: () => onAction('replace')
					}
				);
			}

			if (canManage) {
				items.push({
					id: 'star',
					label: isStarred ? 'Remove from starred' : 'Add to starred',
					icon: Star,
					onClick: () => onAction('star')
				});

				if (!isFolder) {
					items.push({
						id: 'setColor',
						label: 'Set color',
						icon: Palette,
						onClick: () => {
							showColorPicker = true;
						}
					});
				}

				items.push(
					{ id: 'sep2', label: '', separator: true, onClick: () => {} },
					{
						id: 'delete',
						label: 'Move to trash',
						icon: Trash2,
						danger: true,
						shortcut: 'Del',
						onClick: () => onAction('delete')
					}
				);
			}
		}

		return items;
	}

	function handleColorSelect(color: string | null) {
		onSetColor(color);
		showColorPicker = false;
	}

	function handlePickerClickOutside(e: MouseEvent) {
		if (pickerRef && !pickerRef.contains(e.target as Node)) {
			showColorPicker = false;
		}
	}

	function handlePickerKeyDown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			showColorPicker = false;
		}
	}
</script>

<svelte:window onclick={handlePickerClickOutside} onkeydown={handlePickerKeyDown} />

<MenuComponent
	items={menuItems}
	x={position.x}
	y={position.y}
	visible={isOpen && !showColorPicker}
	{onClose}
/>

{#if showColorPicker}
	<div
		bind:this={pickerRef}
		class="fixed z-[9999] w-44 rounded-xl border border-base-300/70 bg-base-100 p-2 shadow-xl shadow-black/20"
		style="left: {position.x}px; top: {position.y}px;"
		role="dialog"
		tabindex="-1"
		aria-label="Set color"
		onclick={(e) => e.stopPropagation()}
		onkeydown={(e) => e.stopPropagation()}
	>
		<div class="mb-2 flex items-center justify-between px-1">
			<span class="text-xs font-medium text-base-content/70">Set color</span>
			<button
				type="button"
				class="rounded-md p-1 text-base-content/50 hover:bg-base-200"
				onclick={() => (showColorPicker = false)}
			>
				<X size={12} />
			</button>
		</div>
		<div class="grid grid-cols-4 gap-1.5">
			{#each COLOR_PALETTE as color}
				<button
					type="button"
					class="h-7 w-7 rounded-full {color.class} ring-offset-2 hover:ring-2 hover:ring-base-content/30 focus:outline-hidden focus:ring-2 focus:ring-base-content/30"
					aria-label={color.label}
					onclick={() => handleColorSelect(color.key)}
				></button>
			{/each}
			<button
				type="button"
				class="flex h-7 w-7 items-center justify-center rounded-full border border-base-300 text-base-content/50 hover:bg-base-200"
				aria-label="Clear color"
				onclick={() => handleColorSelect(null)}
			>
				<X size={14} />
			</button>
		</div>
	</div>
{/if}
