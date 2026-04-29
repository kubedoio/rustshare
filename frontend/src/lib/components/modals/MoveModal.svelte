<script lang="ts">
	import { createQuery } from '$lib/query-compat';
	import { getFolderTree } from '$lib/api/folders';
	import type { FolderTree } from '$lib/api/folders';
	import { Hop as Home, Loader as Loader2, CircleAlert as AlertCircle } from 'lucide-svelte';
	import MoveFolderTreeItem from './MoveFolderTreeItem.svelte';

	interface Props {
		open?: boolean;
		loading?: boolean;
		itemName?: string;
		itemType?: 'file' | 'folder';
		currentFolderId?: string | null;
		itemId?: string | null;
		onClose?: () => void;
		onConfirm?: (payload: { targetFolderId: string | null }) => void;
	}

	let {
		open = false,
		loading = false,
		itemName = '',
		itemType = 'file',
		currentFolderId = null,
		itemId = null,
		onClose = () => {},
		onConfirm = () => {}
	}: Props = $props();

	let selectedFolderId: string | null = $state(null);
	let error = $state('');
	let invalidFolderIds = $state(new Set<string>());
	let expandedFolders = $state(new Set<string>());

	// Query for folder tree
	let folderTreeQuery = $derived(
		createQuery({
			queryKey: ['folder-tree'],
			queryFn: getFolderTree,
			enabled: open
		})
	);

	// Build set of invalid folder IDs (folder itself + all descendants) to prevent circular moves
	function getDescendantIds(tree: FolderTree, folderId: string): Set<string> {
		const ids = new Set<string>();

		function traverse(t: FolderTree): boolean {
			if (t.folder.id === folderId) {
				ids.add(t.folder.id);
				t.subfolders.forEach((child) => traverseAll(child));
				return true;
			}
			for (const child of t.subfolders) {
				if (traverse(child)) return true;
			}
			return false;
		}

		function traverseAll(t: FolderTree) {
			ids.add(t.folder.id);
			t.subfolders.forEach((child) => traverseAll(child));
		}

		traverse(tree);
		return ids;
	}

	// Update invalid folder IDs when folder tree loads
	$effect(() => {
		if ($folderTreeQuery.data && itemType === 'folder' && itemId) {
			invalidFolderIds = getDescendantIds($folderTreeQuery.data, itemId);
		} else {
			invalidFolderIds = new Set();
		}
	});

	function handleSubmit() {
		error = '';

		if (selectedFolderId === currentFolderId) {
			error = 'Item is already in this folder';
			return;
		}

		if (itemType === 'folder' && selectedFolderId && invalidFolderIds.has(selectedFolderId)) {
			error = 'Cannot move a folder into itself or its descendants';
			return;
		}

		onConfirm({ targetFolderId: selectedFolderId });
	}

	function handleClose() {
		selectedFolderId = null;
		error = '';
		expandedFolders = new Set();
		onClose();
	}

	function toggleFolder(folderId: string) {
		const newExpanded = new Set(expandedFolders);
		if (newExpanded.has(folderId)) {
			newExpanded.delete(folderId);
		} else {
			newExpanded.add(folderId);
		}
		expandedFolders = newExpanded;
	}

	function selectFolder(folderId: string | null) {
		selectedFolderId = folderId;
		error = '';
	}

	// Reset when opened
	$effect(() => {
		if (open) {
			selectedFolderId = null;
			error = '';
			expandedFolders = new Set();
		}
	});
</script>

{#if open}
	<div class="fixed inset-0 z-50 flex items-center justify-center p-4">
		<!-- Backdrop -->
		<button
			type="button"
			class="absolute inset-0 cursor-default bg-black/60 backdrop-blur-sm"
			onclick={handleClose}
			aria-label="Close"
		></button>

		<!-- Modal -->
		<div class="relative w-full max-w-md overflow-hidden rounded-xl bg-base-100 shadow-2xl">
			<!-- Header -->
			<div class="border-b border-base-300/50 px-5 py-4">
				<h3 class="text-lg font-semibold text-base-content">
					Move {itemType === 'folder' ? 'Folder' : 'File'}
				</h3>
				<p class="mt-1 text-sm text-base-content/60">
					Choose a destination for <span class="font-medium text-base-content">{itemName}</span>
				</p>
			</div>

			<!-- Folder Tree -->
			<div class="p-4">
				<div class="max-h-80 overflow-y-auto rounded-lg border border-base-300/50 bg-base-200/30">
					{#if $folderTreeQuery.isLoading}
						<div class="flex items-center justify-center py-8">
							<Loader2 size={24} class="animate-spin text-brand-500" />
						</div>
					{:else if $folderTreeQuery.isError}
						<div class="flex items-center gap-2 px-4 py-4 text-error">
							<AlertCircle size={18} />
							<span>Failed to load folders</span>
						</div>
					{:else if $folderTreeQuery.data}
						<!-- Root option -->
						<button
							type="button"
							class="flex w-full items-center gap-3 border-b border-base-300/30 px-4 py-3 text-left transition-colors
								{selectedFolderId === null ? 'bg-brand-500/10 text-brand-600' : 'hover:bg-base-200/50'}"
							onclick={() => selectFolder(null)}
						>
							<Home size={18} />
							<span class="font-medium">Home</span>
							{#if currentFolderId === null}
								<span
									class="ml-auto rounded-full bg-base-300/50 px-2 py-0.5 text-xs text-base-content/60"
									>Current</span
								>
							{/if}
						</button>

						<!-- Folder tree -->
						{#if $folderTreeQuery.data.subfolders?.length > 0}
							<div class="py-1">
								{#each $folderTreeQuery.data.subfolders as folder (folder.folder.id)}
									<MoveFolderTreeItem
										{folder}
										level={0}
										{selectedFolderId}
										{currentFolderId}
										{invalidFolderIds}
										{expandedFolders}
										onSelect={selectFolder}
										onToggle={toggleFolder}
									/>
								{/each}
							</div>
						{/if}
					{/if}
				</div>

				{#if error}
					<div
						class="mt-3 flex items-center gap-2 rounded-lg bg-error/10 px-3 py-2 text-sm text-error"
					>
						<AlertCircle size={16} />
						<span>{error}</span>
					</div>
				{/if}
			</div>

			<!-- Actions -->
			<div class="flex justify-end gap-3 border-t border-base-300/50 px-5 py-4">
				<button
					type="button"
					class="rounded-lg px-4 py-2 text-sm font-medium text-base-content/70 transition-colors hover:bg-base-200 hover:text-base-content"
					onclick={handleClose}
					disabled={loading}
				>
					Cancel
				</button>
				<button
					type="button"
					class="flex items-center gap-2 rounded-lg bg-brand-500 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-600 disabled:opacity-50"
					onclick={handleSubmit}
					disabled={loading || selectedFolderId === currentFolderId}
				>
					{#if loading}
						<Loader2 size={16} class="animate-spin" />
					{/if}
					Move Here
				</button>
			</div>
		</div>
	</div>
{/if}
