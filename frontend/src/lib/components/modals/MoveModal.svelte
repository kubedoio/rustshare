<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { createQuery } from '@tanstack/svelte-query';
  import { getFolderTree } from '$lib/api/folders';
  import type { FolderTree } from '$lib/api/folders';
  import FolderTreeItem from './FolderTreeItem.svelte';

  export let open = false;
  export let loading = false;
  export let itemName = '';
  export let itemType: 'file' | 'folder' = 'file';
  export let currentFolderId: string | null = null;
  export let itemId: string | null = null;

  const dispatch = createEventDispatcher<{
    close: void;
    confirm: { targetFolderId: string | null };
  }>();

  let selectedFolderId: string | null = null;
  let error = '';
  let invalidFolderIds = new Set<string>();

  // Query for folder tree
  const folderTreeQuery = createQuery({
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
        // Add all descendants
        t.subfolders.forEach(child => traverseAll(child));
        return true;
      }
      // Continue searching in subfolders
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

    // Prevent circular moves for folders
    if (itemType === 'folder' && selectedFolderId && invalidFolderIds.has(selectedFolderId)) {
      error = 'Cannot move a folder into itself or its descendants';
      return;
    }

    dispatch('confirm', { targetFolderId: selectedFolderId });
  }

  function handleClose() {
    selectedFolderId = null;
    error = '';
    dispatch('close');
  }

  function toggleFolder(node: FolderTree, expandedFolders: Set<string>): Set<string> {
    const newSet = new Set(expandedFolders);
    if (newSet.has(node.folder.id)) {
      newSet.delete(node.folder.id);
    } else {
      newSet.add(node.folder.id);
    }
    return newSet;
  }

  let expandedFolders = new Set<string>();

  $: if (open) {
    selectedFolderId = null;
    error = '';
    expandedFolders = new Set();
  }
</script>

<dialog class="modal" class:modal-open={open}>
  <div class="modal-box max-w-2xl">
    <h3 class="font-bold text-lg mb-4">
      Move {itemType === 'folder' ? 'Folder' : 'File'}
    </h3>

    <p class="text-sm text-base-content/70 mb-4">
      Move <strong>{itemName}</strong> to:
    </p>

    <div class="border border-base-300 rounded-lg p-4 max-h-96 overflow-y-auto">
      {#if $folderTreeQuery.isLoading}
        <div class="flex justify-center py-8">
          <span class="loading loading-spinner loading-md"></span>
        </div>
      {:else if $folderTreeQuery.isError}
        <div class="alert alert-error">
          <span>Failed to load folders: {$folderTreeQuery.error?.message}</span>
        </div>
      {:else if $folderTreeQuery.data}
        <!-- Root folder option -->
        <button
          type="button"
          class="flex items-center gap-2 w-full text-left p-2 rounded hover:bg-base-200 transition-colors"
          class:bg-primary={selectedFolderId === null}
          class:text-primary-content={selectedFolderId === null}
          on:click={() => (selectedFolderId = null)}
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            fill="none"
            viewBox="0 0 24 24"
            stroke-width="1.5"
            stroke="currentColor"
            class="w-5 h-5"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              d="M2.25 12l8.954-8.955c.44-.439 1.152-.439 1.591 0L21.75 12M4.5 9.75v10.125c0 .621.504 1.125 1.125 1.125H9.75v-4.875c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125V21h4.125c.621 0 1.125-.504 1.125-1.125V9.75M8.25 21h8.25"
            />
          </svg>
          <span class="font-medium">Root</span>
          {#if currentFolderId === null}
            <span class="badge badge-sm ml-auto">Current</span>
          {/if}
        </button>

        <!-- Folder tree - render subfolders of virtual root directly -->
        {#if $folderTreeQuery.data && $folderTreeQuery.data.subfolders}
          {#each $folderTreeQuery.data.subfolders as subfolder (subfolder.folder.id)}
            <FolderTreeItem
              node={subfolder}
              {selectedFolderId}
              {currentFolderId}
              {expandedFolders}
              {invalidFolderIds}
              on:select={(e) => (selectedFolderId = e.detail)}
              on:toggle={(e) => (expandedFolders = toggleFolder(e.detail, expandedFolders))}
              level={0}
            />
          {/each}
        {/if}
      {/if}
    </div>

    {#if error}
      <div class="alert alert-error mt-4">
        <span>{error}</span>
      </div>
    {/if}

    <div class="modal-action">
      <button
        type="button"
        class="btn btn-ghost"
        on:click={handleClose}
        disabled={loading}
      >
        Cancel
      </button>
      <button
        type="button"
        class="btn btn-primary"
        on:click={handleSubmit}
        disabled={loading}
      >
        {#if loading}
          <span class="loading loading-spinner loading-sm"></span>
        {/if}
        Move Here
      </button>
    </div>
  </div>

  <form method="dialog" class="modal-backdrop">
    <button type="button" on:click={handleClose} disabled={loading}>close</button>
  </form>
</dialog>
