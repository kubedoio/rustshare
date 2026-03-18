<script lang="ts">
  import type { File, Folder } from '$lib/api/types';
  import FileListItem from './FileListItem.svelte';

  export let folders: Folder[] = [];
  export let files: File[] = [];
  export let onFolderClick: (folder: Folder) => void;
  export let onFileClick: (file: File) => void;
  export let onRenameFolder: (folder: Folder) => void = () => {};
  export let onDeleteFolder: (folder: Folder) => void = () => {};
  export let onRenameFile: (file: File) => void = () => {};
  export let onDeleteFile: (file: File) => void = () => {};
  export let onShareFile: (file: File) => void = () => {};
  export let onVersionHistory: (file: File) => void = () => {};
</script>

{#if folders.length === 0 && files.length === 0}
  <div class="text-center py-12">
    <p class="text-base-content/60">No files or folders here</p>
  </div>
{:else}
  <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
    {#each folders as folder}
      <FileListItem
        item={folder}
        isFolder={true}
        onSelect={() => onFolderClick(folder)}
        on:rename={(e) => e.detail.isFolder && onRenameFolder(folder)}
        on:delete={(e) => e.detail.isFolder && onDeleteFolder(folder)}
      />
    {/each}

    {#each files as file}
      <FileListItem
        item={file}
        isFolder={false}
        onSelect={() => onFileClick(file)}
        on:rename={(e) => !e.detail.isFolder && onRenameFile(file)}
        on:delete={(e) => !e.detail.isFolder && onDeleteFile(file)}
        on:share={(e) => onShareFile(e.detail.item)}
        on:versionHistory={(e) => onVersionHistory(e.detail.item)}
      />
    {/each}
  </div>
{/if}
