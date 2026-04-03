<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import { createQuery } from '@tanstack/svelte-query';
	import { getFolderTree } from '$lib/api/folders';
	import type { FolderTree } from '$lib/api/folders';
	import { Hop as Home, Loader as Loader2, CircleAlert as AlertCircle } from 'lucide-svelte';
	import MoveFolderTreeItem from './MoveFolderTreeItem.svelte';

	export let open = false;
	export let loading = false;
	export let itemName = '';
	export let itemType: 'file' | 'folder' = 'file';
	export let currentFolderId: string | null = null;
	export let itemId: string | null = null;

	type DispatchEvents = {
		close: void;
		confirm: { targetFolderId: string | null };
	}
	const dispatch = createEventDispatcher<DispatchEvents>();

	let selectedFolderId: string | null = null;
	let error = '';
	let invalidFolderIds = new Set<string>();
	let expandedFolders = new Set<string>();

	// Query for folder tree
	$: folderTreeQuery = createQuery({
		queryKey: ['folder-tree'],
		queryFn: getFolderTree,
		enabled: open
	});

	// Build set of invalid folder IDs (folder itself + all descendants) to prevent circular moves
	function getDescendantIds(tree: FolderTree, folderId: string): Set<string> {
		const ids = new Set<string>();

		function traverse(t: FolderTree): boolean {
			if (t.folder.id === folderId) {
				ids.add(t.folder.id);
				t.subfolders.forEach(child => traverseAll(child));
				return true;
			}
			for (const child of t.subfolders) {
				if (traverse(child)) return true;
			}
			return false;
		}

		function traverseAll(t: FolderTree) {
			ids.add(t.folder.id);
			t.subfolders.forEach(child => traverseAll(child));
		}

		traverse(tree);
		return ids;
	}

	// Update invalid folder IDs when folder tree loads
	$: if ($folderTreeQuery.data && itemType === 'folder' && itemId) {
		invalidFolderIds = getDescendantIds($folderTreeQuery.data, itemId);
	} else {
		invalidFolderIds = new Set();
	}

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

		dispatch('confirm', { targetFolderId: selectedFolderId });
	}

	function handleClose() {
		selectedFolderId = null;
		error = '';
		expandedFolders = new Set();
		dispatch('close');
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
	$: if (open) {
		selectedFolderId = null;
		error = '';
		expandedFolders = new Set();
	}
</script>

{#if open}
	<div class="fixed inset-0 z-50 flex items-center justify-center p-4">
		<!-- Backdrop -->
		<button
			type="button"
			class="absolute inset-0 bg-black/60 backdrop-blur-sm cursor-default"
			on:click={handleClose}
			aria-label="Close"
		></button>

		<!-- Modal -->
		<div class="relative bg-base-100 rounded-xl shadow-2xl w-full max-w-md overflow-hidden">
			<!-- Header -->
			<div class="px-5 py-4 border-b border-base-300/50">
				<h3 class="text-lg font-semibold text-base-content">
					Move {itemType === 'folder' ? 'Folder' : 'File'}
				</h3>
				<p class="text-sm text-base-content/60 mt-1">
					Choose a destination for <span class="font-medium text-base-content">{itemName}</span>
				</p>
			</div>

			<!-- Folder Tree -->
			<div class="p-4">
				<div class="border border-base-300/50 rounded-lg bg-base-200/30 max-h-80 overflow-y-auto">
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
							class="w-full flex items-center gap-3 px-4 py-3 text-left transition-colors border-b border-base-300/30
								{selectedFolderId === null 
									? 'bg-brand-500/10 text-brand-600' 
									: 'hover:bg-base-200/50'}"
							on:click={() => selectFolder(null)}
						>
							<Home size={18} />
							<span class="font-medium">Home</span>
							{#if currentFolderId === null}
								<span class="ml-auto text-xs px-2 py-0.5 rounded-full bg-base-300/50 text-base-content/60">Current</span>
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
					<div class="flex items-center gap-2 mt-3 px-3 py-2 bg-error/10 text-error rounded-lg text-sm">
						<AlertCircle size={16} />
						<span>{error}</span>
					</div>
				{/if}
			</div>

			<!-- Actions -->
			<div class="px-5 py-4 border-t border-base-300/50 flex justify-end gap-3">
				<button
					type="button"
					class="px-4 py-2 text-sm font-medium text-base-content/70 hover:text-base-content hover:bg-base-200 rounded-lg transition-colors"
					on:click={handleClose}
					disabled={loading}
				>
					Cancel
				</button>
				<button
					type="button"
					class="px-4 py-2 text-sm font-medium bg-brand-500 hover:bg-brand-600 text-white rounded-lg transition-colors flex items-center gap-2 disabled:opacity-50"
					on:click={handleSubmit}
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
