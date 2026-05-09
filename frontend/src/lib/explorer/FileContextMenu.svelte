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
		Star
	} from 'lucide-svelte';

	interface Props {
		item: FileType | Folder;
		workspaceMode?: 'all' | 'photos' | 'recent' | 'starred' | 'deleted';
		isOpen?: boolean;
		position?: { x: number; y: number };
		onClose?: () => void;
		onAction?: (action: string) => void;
	}

	let {
		item,
		workspaceMode = 'all',
		isOpen = false,
		position = { x: 0, y: 0 },
		onClose = () => {},
		onAction = () => {}
	}: Props = $props();

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
						id: 'move', label: 'Move to...', icon: Move, onClick: () => onAction('move') }
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
				items.push(
					{
						id: 'star',
						label: isStarred ? 'Remove from starred' : 'Add to starred',
						icon: Star,
						onClick: () => onAction('star')
					},
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
</script>

<MenuComponent items={menuItems} x={position.x} y={position.y} visible={isOpen} {onClose} />
