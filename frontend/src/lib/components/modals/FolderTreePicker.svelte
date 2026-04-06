<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query';
  import { getFolderTree, type FolderTree } from '$lib/api/folders';
  import { Hop as Home, Loader2, AlertCircle, ChevronRight, ChevronDown, Folder, FolderOpen } from 'lucide-svelte';
  import FolderTreeItem from './FolderTreeItem.svelte';

  interface Props {
    selectedFolderId: string | null;
    currentFolderId: string | null;
    expandedFolderIds?: Set<string>;
    disabledFolderIds?: Set<string>;
    onSelect: (folderId: string | null) => void;
    onToggle?: (folderId: string) => void;
  }

  let {
    selectedFolderId,
    currentFolderId,
    expandedFolderIds = new Set(),
    disabledFolderIds = new Set(),
    onSelect,
    onToggle
  }: Props = $props();

  let localExpanded = $state<Set<string>>(new Set());
  
  // Initialize and sync with prop
  $effect(() => {
    localExpanded = new Set(expandedFolderIds);
  });

  let folderTreeQuery = $derived(createQuery({
    queryKey: ['folder-tree'],
    queryFn: getFolderTree,
    staleTime: 0
  }));

  function toggleFolder(folderId: string) {
    const newExpanded = new Set(localExpanded);
    if (newExpanded.has(folderId)) {
      newExpanded.delete(folderId);
    } else {
      newExpanded.add(folderId);
    }
    localExpanded = newExpanded;
    onToggle?.(folderId);
  }

  function selectFolder(folderId: string | null) {
    onSelect(folderId);
  }

  function isDisabled(folderId: string): boolean {
    return disabledFolderIds.has(folderId);
  }
</script>

<div class="border border-base-300/50 rounded-lg bg-base-200/30 max-h-64 overflow-y-auto">
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
      onclick={() => selectFolder(null)}
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
          <FolderTreeItem
            {folder}
            level={0}
            {selectedFolderId}
            {currentFolderId}
            expandedFolders={localExpanded}
            {disabledFolderIds}
            onSelect={selectFolder}
            onToggle={toggleFolder}
          />
        {/each}
      </div>
    {/if}
  {/if}
</div>
