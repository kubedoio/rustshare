<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query';
  import { getFolderContents } from '$lib/api/folders';
  import { downloadFile } from '$lib/api/files';
  import FileGrid from '$lib/components/files/FileGrid.svelte';
  import type { File, Folder } from '$lib/api/types';

  let currentFolderId: string | null = null;

  // Reactive query key - updates when currentFolderId changes
  $: contentsQuery = createQuery({
    queryKey: ['folder-contents', currentFolderId],
    queryFn: () => getFolderContents(currentFolderId)
  });

  function handleFolderClick(folder: Folder) {
    currentFolderId = folder.id;
  }

  async function handleFileClick(file: File) {
    try {
      const response = await downloadFile(file.id);
      window.open(response.url, '_blank');
    } catch (error) {
      console.error('Download failed:', error);
    }
  }
</script>

<svelte:head>
  <title>My Files - RustShare</title>
</svelte:head>

<div class="space-y-4">
  <div class="flex items-center justify-between">
    <h1 class="text-2xl font-bold">My Files</h1>
    <div class="flex gap-2">
      <button class="btn btn-primary">+ Upload</button>
      <button class="btn btn-outline">+ New Folder</button>
    </div>
  </div>

  {#if $contentsQuery.isLoading}
    <div class="flex justify-center py-12">
      <span class="loading loading-spinner loading-lg"></span>
    </div>
  {:else if $contentsQuery.isError}
    <div class="alert alert-error">
      <span>Failed to load files: {$contentsQuery.error?.message}</span>
    </div>
  {:else if $contentsQuery.data}
    <FileGrid
      folders={$contentsQuery.data.folders}
      files={$contentsQuery.data.files}
      onFolderClick={handleFolderClick}
      onFileClick={handleFileClick}
    />
  {/if}
</div>
