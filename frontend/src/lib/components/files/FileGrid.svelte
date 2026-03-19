<script lang="ts">
  import type { File, Folder } from '$lib/api/types';
  import FileListItem from './FileListItem.svelte';
  import { selectionStore } from '$lib/stores/selection';

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
  export let onMoveFolder: (folder: Folder) => void = () => {};
  export let onMoveFile: (file: File) => void = () => {};
  export let onDownloadFile: (file: File) => void = () => {};
  export let onReplaceFile: (file: File) => void = () => {};
  export let selectionMode = false;

  function handleFileToggle(file: File) {
    selectionStore.toggleFile(file.id);
  }

  function handleFolderToggle(folder: Folder) {
    selectionStore.toggleFolder(folder.id);
  }

  function handleVersionHistoryClick(e: CustomEvent) {
    console.log('[FileGrid] Version History event received:', e.detail);
    onVersionHistory(e.detail.item);
  }
</script>

{#if folders.length === 0 && files.length === 0}
  <div class="text-center py-16 lg:py-24">
    <svg
      xmlns="http://www.w3.org/2000/svg"
      fill="none"
      viewBox="0 0 24 24"
      stroke-width="1.5"
      stroke="currentColor"
      class="w-20 h-20 lg:w-24 lg:h-24 mx-auto text-base-content/20 mb-4"
    >
      <path
        stroke-linecap="round"
        stroke-linejoin="round"
        d="M2.25 12.75V12A2.25 2.25 0 014.5 9.75h15A2.25 2.25 0 0121.75 12v.75m-8.69-6.44l-2.12-2.12a1.5 1.5 0 00-1.061-.44H4.5A2.25 2.25 0 002.25 6v12a2.25 2.25 0 002.25 2.25h15A2.25 2.25 0 0021.75 18V9a2.25 2.25 0 00-2.25-2.25h-5.379a1.5 1.5 0 01-1.06-.44z"
      />
    </svg>
    <p class="text-lg lg:text-xl text-base-content/60 mb-2">This folder is empty</p>
    <p class="text-sm text-base-content/40">Upload files or create folders to get started</p>
  </div>
{:else}
  <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
    {#each folders as folder}
      <FileListItem
        item={folder}
        isFolder={true}
        onSelect={() => selectionMode ? handleFolderToggle(folder) : onFolderClick(folder)}
        selected={selectionMode && $selectionStore.selectedFolderIds.has(folder.id)}
        {selectionMode}
        on:rename={(e) => e.detail.isFolder && onRenameFolder(folder)}
        on:delete={(e) => e.detail.isFolder && onDeleteFolder(folder)}
        on:move={(e) => e.detail.isFolder && onMoveFolder(folder)}
      />
    {/each}

    {#each files as file}
      <FileListItem
        item={file}
        isFolder={false}
        onSelect={() => selectionMode ? handleFileToggle(file) : onFileClick(file)}
        selected={selectionMode && $selectionStore.selectedFileIds.has(file.id)}
        {selectionMode}
        on:rename={(e) => !e.detail.isFolder && onRenameFile(file)}
        on:delete={(e) => !e.detail.isFolder && onDeleteFile(file)}
        on:share={(e) => onShareFile(e.detail.item)}
        on:versionHistory={handleVersionHistoryClick}
        on:move={(e) => !e.detail.isFolder && onMoveFile(file)}
        on:download={(e) => onDownloadFile(e.detail.item)}
        on:replace={(e) => onReplaceFile(e.detail.item)}
      />
    {/each}
  </div>
{/if}
