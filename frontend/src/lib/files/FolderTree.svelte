<script lang="ts">
	import { onMount } from 'svelte';
	import { createQuery } from '@tanstack/svelte-query';
	import { Folder, Home, Loader2 } from 'lucide-svelte';
	import { getFolderContents } from '$lib/api/folders';
	import { folderTreeStore, selectedFolder, type FolderNode } from '$lib/stores/folderTree';
	import FolderTreeItem from './FolderTreeItem.svelte';

	export let selectedFolderId: string | null = null;
	export let onSelectFolder: (folderId: string | null, folderPath: FolderNode[]) => void;

	let folderPath: FolderNode[] = [];

	// Query for root folders
	const rootFoldersQuery = createQuery({
		queryKey: ['folder-root-contents'],
		queryFn: () => getFolderContents(null)
	});

	$: if ($rootFoldersQuery.data?.folders) {
		folderTreeStore.setRootFolders($rootFoldersQuery.data.folders);
	}

	// Sync external selection
	$: if (selectedFolderId !== undefined && selectedFolderId !== $folderTreeStore.selectedId) {
		folderTreeStore.selectFolder(selectedFolderId);
	}

	async function handleSelect(folder: FolderNode) {
		folderTreeStore.selectFolder(folder.id);
		
		// Build folder path
		folderPath = buildFolderPath($folderTreeStore.rootFolders, folder.id);
		onSelectFolder(folder.id, folderPath);
	}

	async function handleToggleExpand(folder: FolderNode) {
		const willExpand = !$folderTreeStore.expandedIds.has(folder.id);
		folderTreeStore.toggleExpand(folder.id);

		if (willExpand && !folder.children) {
			folderTreeStore.setLoading(folder.id, true);
			try {
				const contents = await getFolderContents(folder.id);
				folderTreeStore.setFolderChildren(folder.id, contents.folders);
			} catch (error) {
				console.error('Failed to load folder contents:', error);
			} finally {
				folderTreeStore.setLoading(folder.id, false);
			}
		}
	}

	function handleSelectRoot() {
		folderTreeStore.selectFolder(null);
		folderPath = [];
		onSelectFolder(null, []);
	}

	function buildFolderPath(folders: FolderNode[], targetId: string): FolderNode[] {
		for (const folder of folders) {
			if (folder.id === targetId) {
				return [folder];
			}
			if (folder.children) {
				const path = buildFolderPath(folder.children, targetId);
				if (path.length > 0) {
					return [folder, ...path];
				}
			}
		}
		return [];
	}

	// Select root on mount if no selection
	onMount(() => {
		if (!$folderTreeStore.selectedId && !selectedFolderId) {
			handleSelectRoot();
		}
	});
</script>

<div class="h-full flex flex-col bg-[#181b21]">
	<!-- Header -->
	<div class="px-3 py-3 border-b border-[#2a2f35]">
		<h2 class="text-xs font-semibold text-[#6b7280] uppercase tracking-wider px-2">Folders</h2>
	</div>

	<!-- Root Item -->
	<button
		type="button"
		class="flex items-center gap-2 px-3 py-2 mx-2 mt-2 rounded-md text-sm transition-colors
			{$folderTreeStore.selectedId === null 
				? 'bg-[#1e3a5f]/60 text-[#e5e7eb] font-medium' 
				: 'text-[#9ca3af] hover:bg-[#1a1d24] hover:text-[#e5e7eb]'}"
		on:click={handleSelectRoot}
	>
		<Home size={18} class={$folderTreeStore.selectedId === null ? 'text-[#2563eb]' : 'text-[#6b7280]'} />
		<span>Home</span>
	</button>

	<!-- Tree -->
	<div class="flex-1 overflow-y-auto py-2" role="tree" aria-label="Folder tree">
		{#if $rootFoldersQuery.isLoading}
			<div class="px-4 py-4 flex items-center justify-center">
				<Loader2 size={20} class="animate-spin text-[#6b7280]" />
			</div>
		{:else if $rootFoldersQuery.isError}
			<div class="px-4 py-4 text-sm text-[#ef4444]">
				Failed to load folders
			</div>
		{:else if $folderTreeStore.rootFolders.length === 0}
			<div class="px-4 py-8 text-center">
				<div class="w-12 h-12 rounded-xl bg-[#1a1d24] flex items-center justify-center mx-auto mb-3">
					<Folder size={24} class="text-[#6b7280]" />
				</div>
				<p class="text-sm text-[#9ca3af]">No folders yet</p>
				<p class="text-xs text-[#6b7280] mt-1">Create folders to organize your files</p>
			</div>
		{:else}
			{#each $folderTreeStore.rootFolders as folder (folder.id)}
				<FolderTreeItem
					{folder}
					onSelect={handleSelect}
					onToggleExpand={handleToggleExpand}
				/>
			{/each}
		{/if}
	</div>
</div>
